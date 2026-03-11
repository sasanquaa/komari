use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, hash_map::Entry},
    fmt::Debug,
};

use anyhow::{Result, anyhow};
use futures::StreamExt;
use futures::stream::BoxStream;
use log::info;
#[cfg(test)]
use mockall::automock;
#[cfg(windows)]
use platforms::capture::WindowsCaptureKind;
use platforms::{
    CoordinateRelative, Error, Window,
    capture::{Capture as PlatformCapture, Frame},
    input::{
        Input as PlatformInput, InputKind as PlatformInputKind,
        InputReceiver as PlatformInputReceiver, MouseKind as PlatformMouseKind,
    },
};

use crate::{
    grpc::{InputService, input::Coordinate as RpcCoordinate},
    models::{CaptureMode, InputMethod as DatabaseInputMethod, Settings},
    rng::Rng,
    run::MS_PER_TICK_F32,
};

mod convert;

pub use convert::*;

/// Base mean in milliseconds to generate a pair from.
const BASE_MEAN_MS_DELAY: f32 = 100.0;

/// Base standard deviation in milliseconds to generate a pair from.
const BASE_STD_MS_DELAY: f32 = 20.0;

/// The rate at which generated standard deviation will revert to the base [`BASE_STD_MS_DELAY`]
/// over time.
const MEAN_STD_REVERSION_RATE: f32 = 0.2;

/// The rate at which generated mean will revert to the base [`BASE_MEAN_MS_DELAY`] over time.
const MEAN_STD_VOLATILITY: f32 = 3.0;

#[cfg_attr(test, automock)]
pub trait InputReceiver: Debug + 'static {
    fn set_window(&mut self, window: Window);

    fn set_method(&mut self, method: InputMethod);

    fn as_stream(&self) -> BoxStream<'static, KeyKind>;
}

#[derive(Debug)]
pub struct DefaultInputReceiver {
    window: Window,
    kind: PlatformInputKind,
    inner: PlatformInputReceiver,
}

impl DefaultInputReceiver {
    pub fn new(window: Window, kind: PlatformInputKind) -> Self {
        Self {
            window,
            kind,
            inner: PlatformInputReceiver::new(window, kind).expect("supported platform"),
        }
    }
}

impl InputReceiver for DefaultInputReceiver {
    fn set_window(&mut self, window: Window) {
        self.window = window;
        self.inner = PlatformInputReceiver::new(window, self.kind).expect("supported platform")
    }

    fn set_method(&mut self, method: InputMethod) {
        self.kind = match method {
            InputMethod::ForegroundRpc(_) | InputMethod::ForegroundDefault => {
                PlatformInputKind::Foreground
            }
            InputMethod::FocusedRpc(_) | InputMethod::FocusedDefault => PlatformInputKind::Focused,
        };
        self.inner =
            PlatformInputReceiver::new(self.window, self.kind).expect("supported platform");
    }

    fn as_stream(&self) -> BoxStream<'static, KeyKind> {
        self.inner
            .as_stream()
            .expect("supported platform")
            .map(KeyKind::from)
            .boxed()
    }
}

/// Options for key down input.
#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct InputKeyDownOptions {
    /// Whether the down stroke can be repeated even if the key is already down.
    ///
    /// Currently supports only [`InputMethod::Default`].
    repeatable: bool,
}

impl InputKeyDownOptions {
    pub fn repeatable(mut self) -> InputKeyDownOptions {
        self.repeatable = true;
        self
    }
}

/// Input method to use.
///
/// This is a bridge enum between platform-specific, database and gRPC input options.
#[derive(Clone, Debug)]
pub enum InputMethod {
    ForegroundRpc(String),
    FocusedRpc(String),
    ForegroundDefault,
    FocusedDefault,
}

impl From<&Settings> for InputMethod {
    fn from(settings: &Settings) -> Self {
        match (settings.input_method, settings.capture_mode) {
            (DatabaseInputMethod::Default, CaptureMode::BitBltArea) => {
                InputMethod::ForegroundDefault
            }
            (DatabaseInputMethod::Default, _) => InputMethod::FocusedDefault,
            (DatabaseInputMethod::Rpc, CaptureMode::BitBltArea) => {
                InputMethod::ForegroundRpc(settings.input_method_rpc_server_url.clone())
            }
            (DatabaseInputMethod::Rpc, _) => {
                InputMethod::FocusedRpc(settings.input_method_rpc_server_url.clone())
            }
        }
    }
}

/// Inner kind of [`InputMethod`].
///
/// The above [`InputMethod`] will be converted to this inner kind that contains the actual
/// sending structure.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum InputMethodInner {
    Rpc(RefCell<InputService>),
    Default(PlatformInput),
}

/// States of input delay tracking.
#[derive(Debug)]
enum InputDelay {
    Untracked,
    Tracked,
    AlreadyTracked,
}

/// A trait for sending inputs.
#[cfg_attr(test, automock)]
pub trait Input: Send + Debug {
    /// Performs a tick update.
    fn update_tick(&mut self, tick: u64);

    /// Overwrites the current input window with new `window`.
    fn set_window(&mut self, window: Window);

    /// Overwrites the current input method with new `method`.
    fn set_method(&mut self, method: InputMethod);

    /// The current state of input represented as a [`String`].
    fn state(&self) -> String;

    /// Sends mouse `kind` to `(x, y)` relative to the client coordinate (e.g. capture area).
    ///
    /// `(0, 0)` is top-left and `(width, height)` is bottom-right.
    fn send_mouse(&self, x: i32, y: i32, kind: MouseKind);

    /// Presses a single key `kind`.
    fn send_key(&self, kind: KeyKind);

    /// Releases a held key `kind`.
    fn send_key_up(&self, kind: KeyKind);

    /// Holds down key `kind`.
    ///
    /// This key stroke is sent with the default options.
    fn send_key_down(&self, kind: KeyKind) {
        self.send_key_down_with_options(kind, InputKeyDownOptions::default());
    }

    /// Same as [`Self::send_key_down`] but with the provided `options`.
    fn send_key_down_with_options(&self, kind: KeyKind, options: InputKeyDownOptions);

    /// Whether the key `kind` is cleared.
    fn is_key_cleared(&self, kind: KeyKind) -> bool;

    /// Whether all keys are cleared.
    fn all_keys_cleared(&self) -> bool;

    #[cfg(debug_assertions)]
    fn clone(&self) -> Box<dyn Input>;
}

/// Default implementation of [`Input`].
#[derive(Debug)]
pub struct DefaultInput {
    window: Window,
    method: InputMethod,
    method_inner: InputMethodInner,
    delay_rng: Rng,
    delay_mean_std_pair: (f32, f32),
    delay_map: RefCell<HashMap<KeyKind, (u32, bool)>>,
}

impl DefaultInput {
    pub fn new(window: Window, method: InputMethod, rng: Rng) -> Self {
        Self {
            window,
            method: method.clone(),
            method_inner: input_method_inner_from(window, method, rng.rng_seed()),
            delay_rng: rng,
            delay_mean_std_pair: (BASE_MEAN_MS_DELAY, BASE_STD_MS_DELAY),
            delay_map: RefCell::new(HashMap::new()),
        }
    }

    #[inline]
    fn key_state(&self, kind: KeyKind) -> Result<KeyState> {
        match &self.method_inner {
            InputMethodInner::Rpc(service) => service
                .borrow_mut()
                .key_state(kind.into())
                .map(KeyState::from)
                .ok_or(anyhow!("service not connected")),
            InputMethodInner::Default(input) => Ok(input.key_state(kind.into())?.into()),
        }
    }

    #[inline]
    fn send_key_inner(&self, kind: KeyKind) -> Result<()> {
        match &self.method_inner {
            InputMethodInner::Rpc(service) => {
                service
                    .borrow_mut()
                    .send_key(kind.into(), self.random_input_delay_tick_count().0);
            }
            InputMethodInner::Default(input) => match self.track_input_delay(kind) {
                InputDelay::Untracked => input.send_key(kind.into())?,
                InputDelay::Tracked => input.send_key_down(kind.into(), false)?,
                InputDelay::AlreadyTracked => (),
            },
        }

        Ok(())
    }

    #[inline]
    fn send_key_up_inner(&self, kind: KeyKind, forced: bool) -> Result<()> {
        match &self.method_inner {
            InputMethodInner::Rpc(service) => {
                service.borrow_mut().send_key_up(kind.into());
            }
            InputMethodInner::Default(input) => {
                if forced || !self.has_input_delay(kind) {
                    input.send_key_up(kind.into())?;
                }
            }
        }

        Ok(())
    }

    #[inline]
    fn send_key_down_inner(&self, kind: KeyKind, repeatable: bool) -> Result<()> {
        match &self.method_inner {
            // NOTE: For unknown reason, hardware custom input (e.g. KMBox, Arduino) seems to only
            // require sending down stroke once and it will continue correctly. But `SendInput`
            // requires repeatedly sending the stroke to simulate flying for some classes.
            InputMethodInner::Rpc(service) => {
                service.borrow_mut().send_key_down(kind.into());
            }
            InputMethodInner::Default(input) => {
                if !self.has_input_delay(kind) {
                    input.send_key_down(kind.into(), repeatable)?;
                }
            }
        }

        Ok(())
    }

    #[inline]
    fn has_input_delay(&self, kind: KeyKind) -> bool {
        self.delay_map.borrow().contains_key(&kind)
    }

    fn track_input_delay(&self, kind: KeyKind) -> InputDelay {
        let mut map = self.delay_map.borrow_mut();
        let entry = map.entry(kind);
        if matches!(entry, Entry::Occupied(_)) {
            return InputDelay::AlreadyTracked;
        }

        let (_, delay_tick_count) = self.random_input_delay_tick_count();
        if delay_tick_count == 0 {
            return InputDelay::Untracked;
        }

        let _ = entry.insert_entry((delay_tick_count, false));
        InputDelay::Tracked
    }

    #[inline]
    fn update(&mut self, game_tick: u64) {
        const UPDATE_MEAN_STD_PAIR_INTERVAL: u64 = 200;

        if game_tick > 0 && game_tick.is_multiple_of(UPDATE_MEAN_STD_PAIR_INTERVAL) {
            let (mean, std) = self.delay_mean_std_pair;
            self.delay_mean_std_pair = self.delay_rng.random_mean_std_pair(
                BASE_MEAN_MS_DELAY,
                mean,
                BASE_STD_MS_DELAY,
                std,
                MEAN_STD_REVERSION_RATE,
                MEAN_STD_VOLATILITY,
            )
        }

        let mut map = self.delay_map.borrow_mut();
        if map.is_empty() {
            return;
        }

        let mut keys_to_remove = HashSet::new();
        for (kind, (delay, did_send_up)) in map.iter_mut() {
            *delay = delay.saturating_sub(1);

            if *delay == 0 {
                if !*did_send_up {
                    *did_send_up = true;
                    let _ = self.send_key_up_inner(*kind, true);
                }

                if matches!(self.key_state(*kind), Ok(KeyState::Released)) {
                    keys_to_remove.insert(*kind);
                }
            }
        }

        if !keys_to_remove.is_empty() {
            map.retain(|kind, _| !keys_to_remove.contains(kind));
        }
    }

    fn random_input_delay_tick_count(&self) -> (f32, u32) {
        let (mean, std) = self.delay_mean_std_pair;
        self.delay_rng
            .random_delay_tick_count(mean, std, MS_PER_TICK_F32, 80.0, 120.0)
    }
}

impl Input for DefaultInput {
    fn update_tick(&mut self, tick: u64) {
        self.update(tick);
    }

    fn set_window(&mut self, window: Window) {
        self.window = window;
        self.set_method(self.method.clone());
    }

    fn set_method(&mut self, method: InputMethod) {
        self.method = method;
        self.method_inner =
            input_method_inner_from(self.window, self.method.clone(), self.delay_rng.rng_seed());
    }

    fn state(&self) -> String {
        match &self.method_inner {
            InputMethodInner::Rpc(service) => format!("RPC({})", service.borrow().state()),
            InputMethodInner::Default(_) => "SendInput".to_string(),
        }
    }

    fn send_mouse(&self, x: i32, y: i32, kind: MouseKind) {
        match &self.method_inner {
            InputMethodInner::Rpc(service) => {
                let mut borrow = service.borrow_mut();
                let relative = match borrow.mouse_coordinate() {
                    RpcCoordinate::Screen => CoordinateRelative::Monitor,
                    RpcCoordinate::Relative => CoordinateRelative::Window,
                };
                let Ok(coordinates) = self.window.convert_coordinate(x, y, relative) else {
                    return;
                };

                borrow.send_mouse(
                    coordinates.width,
                    coordinates.height,
                    coordinates.x,
                    coordinates.y,
                    kind.into(),
                );
            }
            InputMethodInner::Default(keys) => {
                let kind = match kind {
                    MouseKind::Move => PlatformMouseKind::Move,
                    MouseKind::Click => PlatformMouseKind::Click,
                    MouseKind::Scroll => PlatformMouseKind::Scroll,
                };
                let _ = keys.send_mouse(x, y, kind);
            }
        }
    }

    fn send_key(&self, kind: KeyKind) {
        let _ = self.send_key_inner(kind);
    }

    fn send_key_up(&self, kind: KeyKind) {
        let _ = self.send_key_up_inner(kind, false);
    }

    fn send_key_down_with_options(&self, kind: KeyKind, options: InputKeyDownOptions) {
        let _ = self.send_key_down_inner(kind, options.repeatable);
    }

    fn is_key_cleared(&self, kind: KeyKind) -> bool {
        !self.delay_map.borrow().contains_key(&kind)
    }

    #[inline]
    fn all_keys_cleared(&self) -> bool {
        self.delay_map.borrow().is_empty()
    }

    #[cfg(debug_assertions)]
    fn clone(&self) -> Box<dyn Input> {
        Box::new(DefaultInput::new(
            self.window,
            self.method.clone(),
            self.delay_rng.clone(),
        ))
    }
}

/// A trait for managing different capture modes.
///
/// A bridge trait between platform-specific and database.
#[cfg_attr(test, automock)]
pub trait Capture: Debug + 'static {
    fn grab(&mut self) -> Result<Frame, Error>;

    fn window(&self) -> Window;

    fn set_window(&mut self, window: Window);

    fn set_mode(&mut self, mode: CaptureMode);
}

#[derive(Debug)]
pub struct DefaultCapture {
    inner: PlatformCapture,
}

impl DefaultCapture {
    pub fn new(window: Window) -> Self {
        Self {
            inner: PlatformCapture::new(window).expect("supported platform"),
        }
    }
}

impl Capture for DefaultCapture {
    #[inline]
    fn grab(&mut self) -> Result<Frame, Error> {
        self.inner.grab()
    }

    #[inline]
    fn window(&self) -> Window {
        self.inner.window().expect("supported platform")
    }

    #[inline]
    fn set_window(&mut self, window: Window) {
        self.inner.set_window(window).expect("supported platform");
    }

    #[inline]
    fn set_mode(&mut self, mode: CaptureMode) {
        if cfg!(windows) {
            let kind = match mode {
                CaptureMode::BitBlt => WindowsCaptureKind::BitBlt,
                CaptureMode::WindowsGraphicsCapture => WindowsCaptureKind::Wgc,
                CaptureMode::BitBltArea => WindowsCaptureKind::BitBltArea,
            };
            let _ = self.inner.windows_capture_kind(kind);
        }
    }
}

#[inline]
fn input_method_inner_from(window: Window, method: InputMethod, seed: &[u8]) -> InputMethodInner {
    match method {
        InputMethod::ForegroundRpc(url) | InputMethod::FocusedRpc(url) => {
            let result = InputService::new(url, seed.to_vec());
            if result.is_err() {
                info!(target: "backend/rpc", "failed to connect to input server possibly because of incorrect URL, fallback to default input method...");

                return InputMethodInner::Default(
                    PlatformInput::new(window, PlatformInputKind::Focused)
                        .expect("supported platform"),
                );
            }

            InputMethodInner::Rpc(RefCell::new(result.unwrap()))
        }
        InputMethod::ForegroundDefault => InputMethodInner::Default(
            PlatformInput::new(window, PlatformInputKind::Foreground).expect("supported platform"),
        ),
        InputMethod::FocusedDefault => InputMethodInner::Default(
            PlatformInput::new(window, PlatformInputKind::Focused).expect("supported platform"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use super::*;

    const SEED: [u8; 32] = [
        64, 241, 206, 219, 49, 21, 218, 145, 254, 152, 68, 176, 242, 238, 152, 14, 176, 241, 153,
        64, 44, 192, 172, 191, 191, 157, 107, 206, 193, 55, 115, 68,
    ];

    fn test_key_sender() -> DefaultInput {
        DefaultInput::new(
            Window::new("Handle"),
            InputMethod::FocusedDefault,
            Rng::new(SEED, 1337),
        )
    }

    #[test]
    fn track_input_delay_tracked() {
        let sender = test_key_sender();

        // Force rng to generate delay > 0
        let result = sender.track_input_delay(KeyKind::Ctrl);
        assert_matches!(result, InputDelay::Tracked);
        assert!(sender.has_input_delay(KeyKind::Ctrl));
    }

    #[test]
    fn track_input_delay_already_tracked() {
        let sender = test_key_sender();
        sender
            .delay_map
            .borrow_mut()
            .insert(KeyKind::Ctrl, (3, false));

        let result = sender.track_input_delay(KeyKind::Ctrl);
        assert_matches!(result, InputDelay::AlreadyTracked);
    }

    #[test]
    fn update_input_delay_decrement_and_release_key() {
        let mut sender = test_key_sender();
        let count = 50;
        sender
            .delay_map
            .borrow_mut()
            .insert(KeyKind::Ctrl, (count, false));

        for _ in 0..count {
            sender.update(0);
        }
        // After `count` updates, key should be released and removed
        assert!(!sender.has_input_delay(KeyKind::Ctrl));
    }

    #[test]
    fn update_input_delay_refresh_mean_std_pair_every_interval() {
        let mut sender = test_key_sender();
        let original_pair = sender.delay_mean_std_pair;

        // Simulate tick before the interval: should NOT update
        sender.update(199);
        assert_eq!(sender.delay_mean_std_pair, original_pair);

        // Simulate tick AT the interval: should update
        sender.update(200);
        assert_ne!(sender.delay_mean_std_pair, original_pair);
    }
}
