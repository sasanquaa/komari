use std::collections::HashMap;
use std::fmt::Debug;
use std::{any::Any, cell::RefCell};

use anyhow::Result;
#[cfg(test)]
use mockall::automock;
use platforms::windows::{
    BitBltCapture, Frame, Handle, KeyInputKind, KeyKind, Keys, WgcCapture, WindowBoxCapture,
};

use crate::context::MS_PER_TICK_F32;
use crate::database::Seeds;
use crate::rng::Rng;
use crate::{CaptureMode, context::MS_PER_TICK, rpc::KeysService};

/// Base mean in milliseconds to generate a pair from.
pub const BASE_MEAN_MS_DELAY: f32 = 100.0;

/// Base standard deviation in milliseconds to generate a pair from.
pub const BASE_STD_MS_DELAY: f32 = 20.0;

/// The rate at which generated standard deviation will revert to the base [`BASE_STD_MS_DELAY`]
/// over time.
pub const MEAN_STD_REVERSION_RATE: f32 = 0.2;

/// The rate at which generated mean will revert to the base [`BASE_MEAN_MS_DELAY`] over time.
pub const MEAN_STD_VOLATILITY: f32 = 3.0;

/// The input method to use for the key sender.
///
/// This is a bridge enum between platform-specific and gRPC input options.
pub enum KeySenderMethod {
    Rpc(String),
    Default(Handle, KeyInputKind),
}

/// The inner kind of the key sender.
///
/// The above [`KeySenderMethod`] will be converted to this inner kind that contains the actual
/// sending structure.
#[derive(Debug)]
enum KeySenderKind {
    Rpc(Option<RefCell<KeysService>>),
    Default(Keys),
}

/// A trait for sending keys.
#[cfg_attr(test, automock)]
pub trait KeySender: Debug {
    fn set_method(&mut self, method: KeySenderMethod);

    fn send(&self, kind: KeyKind) -> Result<()>;

    fn send_click_to_focus(&self) -> Result<()>;

    fn send_up(&self, kind: KeyKind) -> Result<()>;

    fn send_down(&self, kind: KeyKind) -> Result<()>;

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

#[derive(Debug)]
pub struct DefaultKeySender {
    kind: KeySenderKind,
    delay_rng: RefCell<Rng>,
    delay_mean_std_pair: (f32, f32),
    delay_map: RefCell<HashMap<KeyKind, u32>>,
}

enum InputDelay {
    Untracked,
    Tracked,
    AlreadyTracked,
}

impl DefaultKeySender {
    pub fn new(method: KeySenderMethod, seeds: Seeds) -> Self {
        Self {
            kind: to_key_sender_kind_from(method, &seeds.input_seed),
            delay_rng: RefCell::new(Rng::new(seeds.input_seed)),
            delay_mean_std_pair: (BASE_MEAN_MS_DELAY, BASE_STD_MS_DELAY),
            delay_map: RefCell::new(HashMap::new()),
        }
    }

    #[inline]
    fn send_inner(&self, kind: KeyKind) -> Result<()> {
        match &self.kind {
            KeySenderKind::Rpc(service) => {
                if let Some(cell) = service {
                    cell.borrow_mut()
                        .send(kind, self.random_input_delay_tick_count().0)?;
                }
                Ok(())
            }
            KeySenderKind::Default(keys) => {
                match self.track_input_delay(kind) {
                    InputDelay::Untracked => keys.send(kind)?,
                    InputDelay::Tracked => keys.send_down(kind)?,
                    InputDelay::AlreadyTracked => (),
                }
                Ok(())
            }
        }
    }

    #[inline]
    fn send_up_inner(&self, kind: KeyKind, forced: bool) -> Result<()> {
        match &self.kind {
            KeySenderKind::Rpc(service) => {
                if let Some(cell) = service {
                    cell.borrow_mut().send_up(kind)?;
                }
                Ok(())
            }
            KeySenderKind::Default(keys) => {
                if forced || !self.has_input_delay(kind) {
                    keys.send_up(kind)?;
                }
                Ok(())
            }
        }
    }

    #[inline]
    fn send_down_inner(&self, kind: KeyKind) -> Result<()> {
        match &self.kind {
            KeySenderKind::Rpc(service) => {
                if let Some(cell) = service {
                    cell.borrow_mut().send_down(kind)?;
                }
                Ok(())
            }
            KeySenderKind::Default(keys) => {
                if !self.has_input_delay(kind) {
                    keys.send_down(kind)?;
                }
                Ok(())
            }
        }
    }

    #[inline]
    fn has_input_delay(&self, kind: KeyKind) -> bool {
        self.delay_map.borrow().contains_key(&kind)
    }

    /// Tracks input delay for a key that is about to be pressed for both down and up key strokes.
    ///
    /// Upon returning [`InputDelay::Tracked`], it is expected that only key down is sent. Later,
    /// it will be automatically released by [`Self::update_input_delay`] once the input delay has
    /// timed out. If [`InputDelay::Untracked`] is returned, it is expected that both down and up
    /// key strokes are sent.
    ///
    /// This function should only be used for [`Self::send`] as the other two should be handled
    /// by the external caller.
    fn track_input_delay(&self, kind: KeyKind) -> InputDelay {
        let mut map = self.delay_map.borrow_mut();
        if map.contains_key(&kind) {
            return InputDelay::AlreadyTracked;
        }

        let (_, delay_tick_count) = self.random_input_delay_tick_count();
        if delay_tick_count > 0 {
            let _ = map.insert(kind, delay_tick_count);
            InputDelay::Tracked
        } else {
            InputDelay::Untracked
        }
    }

    /// Updates the input delay (key up timing) for held down keys.
    #[inline]
    pub fn update_input_delay(&mut self, game_tick: u64) {
        const UPDATE_MEAN_STD_PAIR_INTERVAL: u64 = 200;

        if game_tick > 0 && game_tick % UPDATE_MEAN_STD_PAIR_INTERVAL == 0 {
            let (mean, std) = self.delay_mean_std_pair;
            self.delay_mean_std_pair = self.delay_rng.borrow_mut().random_mean_std_pair(
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

        map.retain(|kind, delay| {
            *delay = delay.saturating_sub(1);
            if *delay == 0 {
                let _ = self.send_up_inner(*kind, true);
            }
            *delay != 0
        });
    }

    fn random_input_delay_tick_count(&self) -> (f32, u32) {
        let (mean, std) = self.delay_mean_std_pair;
        self.delay_rng
            .borrow_mut()
            .random_delay_tick_count(mean, std, MS_PER_TICK_F32, 80.0, 120.0)
    }
}

impl KeySender for DefaultKeySender {
    fn set_method(&mut self, method: KeySenderMethod) {
        match &method {
            KeySenderMethod::Rpc(url) => {
                if let KeySenderKind::Rpc(ref option) = self.kind {
                    let service = option.as_ref();
                    let service_borrow = service.map(|service| service.borrow_mut());
                    if let Some(mut borrow) = service_borrow
                        && borrow.url() == url
                    {
                        borrow.reset();
                        borrow.init(self.delay_rng.borrow().seed());
                        return;
                    }
                }
            }
            KeySenderMethod::Default(_, _) => (),
        }
        self.kind = to_key_sender_kind_from(method, self.delay_rng.borrow().seed());
    }

    fn send(&self, kind: KeyKind) -> Result<()> {
        self.send_inner(kind)
    }

    fn send_click_to_focus(&self) -> Result<()> {
        match &self.kind {
            KeySenderKind::Rpc(_) => Ok(()),
            KeySenderKind::Default(keys) => {
                keys.send_click_to_focus()?;
                Ok(())
            }
        }
    }

    fn send_up(&self, kind: KeyKind) -> Result<()> {
        self.send_up_inner(kind, false)
    }

    fn send_down(&self, kind: KeyKind) -> Result<()> {
        self.send_down_inner(kind)
    }

    #[inline]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// A bridge enum between platform-specific and database capture options.
#[derive(Debug)]
pub enum ImageCaptureKind {
    BitBlt(BitBltCapture),
    Wgc(Option<WgcCapture>),
    BitBltArea(WindowBoxCapture),
}

/// A struct for managing different capture modes.
#[derive(Debug)]
pub struct ImageCapture {
    kind: ImageCaptureKind,
}

impl ImageCapture {
    pub fn new(handle: Handle, mode: CaptureMode) -> Self {
        Self {
            kind: to_image_capture_kind_from(handle, mode),
        }
    }

    pub fn kind(&self) -> &ImageCaptureKind {
        &self.kind
    }

    pub fn grab(&mut self) -> Option<Frame> {
        match &mut self.kind {
            ImageCaptureKind::BitBlt(capture) => capture.grab().ok(),
            ImageCaptureKind::Wgc(capture) => {
                capture.as_mut().and_then(|capture| capture.grab().ok())
            }
            ImageCaptureKind::BitBltArea(capture) => capture.grab().ok(),
        }
    }

    pub fn set_mode(&mut self, handle: Handle, mode: CaptureMode) {
        self.kind = to_image_capture_kind_from(handle, mode);
    }
}

#[inline]
fn to_key_sender_kind_from(method: KeySenderMethod, seed: &[u8]) -> KeySenderKind {
    match method {
        KeySenderMethod::Rpc(url) => {
            let mut service = KeysService::connect(url);
            if let Ok(ref mut service) = service {
                service.init(seed);
            }
            KeySenderKind::Rpc(service.ok().map(RefCell::new))
        }
        KeySenderMethod::Default(handle, kind) => KeySenderKind::Default(Keys::new(handle, kind)),
    }
}

#[inline]
fn to_image_capture_kind_from(handle: Handle, mode: CaptureMode) -> ImageCaptureKind {
    match mode {
        CaptureMode::BitBlt => ImageCaptureKind::BitBlt(BitBltCapture::new(handle, false)),
        CaptureMode::WindowsGraphicsCapture => {
            ImageCaptureKind::Wgc(WgcCapture::new(handle, MS_PER_TICK).ok())
        }
        CaptureMode::BitBltArea => ImageCaptureKind::BitBltArea(WindowBoxCapture::default()),
    }
}
