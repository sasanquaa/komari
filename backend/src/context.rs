use std::{
    cell::RefCell,
    env,
    fs::File,
    io::Write,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use dyn_clone::clone_box;
use opencv::{
    core::{Vector, VectorToVec},
    imgcodecs::imencode_def,
};
use platforms::windows::{self, Handle, KeyInputKind, KeyReceiver};
use strum::IntoEnumIterator;
use tokio::sync::broadcast;

use crate::{
    Action, RequestHandler,
    bridge::{DefaultKeySender, ImageCapture, ImageCaptureKind, KeySender, KeySenderMethod},
    buff::{Buff, BuffKind, BuffState},
    database::{CaptureMode, InputMethod, KeyBinding, query_seeds},
    detect::{CachedDetector, Detector},
    mat::OwnedMat,
    minimap::{Minimap, MinimapState},
    network::{DiscordNotification, NotificationKind},
    player::{Player, PlayerState},
    query_configs, query_settings,
    request_handler::{DefaultRequestHandler, config_buffs},
    rng::Rng,
    rotator::Rotator,
    skill::{Skill, SkillKind, SkillState},
};
#[cfg(test)]
use crate::{Settings, bridge::MockKeySender, detect::MockDetector};

const FPS: u32 = 30;
pub const MS_PER_TICK: u64 = MS_PER_TICK_F32 as u64;
pub const MS_PER_TICK_F32: f32 = 1000.0 / FPS as f32;

/// A control flow to use after a contextual state update.
pub enum ControlFlow<T> {
    /// The contextual state is updated immediately.
    Immediate(T),
    /// The contextual state is updated in the next tick.
    Next(T),
}

/// Represents a contextual state.
pub trait Contextual {
    /// The inner state that is persistent through each [`Contextual::update`] tick.
    type Persistent = ();

    /// Updates the contextual state.
    ///
    /// This is basically a state machine.
    ///
    /// Updating is performed on each tick and the behavior whether to continue
    /// updating in the same tick or next is decided by [`ControlFlow`]. The state
    /// can transition or stay the same.
    fn update(self, context: &Context, persistent: &mut Self::Persistent) -> ControlFlow<Self>
    where
        Self: Sized;
}

/// A struct that stores the game information.
#[derive(Debug)]
pub struct Context {
    /// The `MapleStory` class game handle.
    pub handle: Handle,
    /// A struct to send key inputs.
    pub keys: Box<dyn KeySender>,
    pub rng: Rng,
    /// A struct for sending notifications through web hook.
    pub notification: DiscordNotification,
    /// A struct to detect game information.
    ///
    /// This is [`None`] when no frame as ever been captured.
    pub detector: Option<Box<dyn Detector>>,
    /// The minimap contextual state.
    pub minimap: Minimap,
    /// The player contextual state.
    pub player: Player,
    /// The skill contextual states.
    pub skills: [Skill; SkillKind::COUNT],
    /// The buff contextual states.
    pub buffs: [Buff; BuffKind::COUNT],
    /// Whether the bot is halting.
    pub halting: bool,
    /// The game current tick.
    ///
    /// This is increased on each update tick.
    pub tick: u64,
}

impl Context {
    #[cfg(test)]
    pub fn new(keys: Option<MockKeySender>, detector: Option<MockDetector>) -> Self {
        Context {
            handle: Handle::new(""),
            keys: Box::new(keys.unwrap_or_default()),
            rng: Rng::new(rand::random()),
            notification: DiscordNotification::new(Rc::new(RefCell::new(Settings::default()))),
            detector: detector.map(|detector| Box::new(detector) as Box<dyn Detector>),
            minimap: Minimap::Detecting,
            player: Player::Detecting,
            skills: [Skill::Detecting; SkillKind::COUNT],
            buffs: [Buff::No; BuffKind::COUNT],
            halting: false,
            tick: 0,
        }
    }

    #[inline]
    pub fn detector_unwrap(&self) -> &dyn Detector {
        self.detector
            .as_ref()
            .expect("detector is not available because no frame has ever been captured")
            .as_ref()
    }

    #[inline]
    pub fn detector_cloned_unwrap(&self) -> Box<dyn Detector> {
        clone_box(self.detector_unwrap())
    }
}

pub fn init() {
    static LOOPING: AtomicBool = AtomicBool::new(false);

    if LOOPING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::Acquire)
        .is_ok()
    {
        let dll = env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("onnxruntime.dll");
        if let Ok(mut file) = File::create_new(dll.clone()) {
            file.write_all(include_bytes!(env!("ONNX_RUNTIME")))
                .unwrap();
        }
        ort::init_from(dll.to_str().unwrap()).commit().unwrap();
        windows::init();
        thread::spawn(|| {
            let tokio_rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            let _tokio_guard = tokio_rt.enter();
            tokio_rt.block_on(async {
                update_loop();
            });
        });
    }
}

#[inline]
fn update_loop() {
    // MapleStoryClass <- GMS
    // MapleStoryClassSG <- MSEA
    // MapleStoryClassTW <- TMS
    let handle = Handle::new("MapleStoryClass");
    let mut rotator = Rotator::default();
    let mut actions = Vec::<Action>::new();
    let mut config = query_configs().unwrap().into_iter().next().unwrap(); // Override by UI
    let mut buffs = config_buffs(&config);
    let settings = query_settings(); // Override by UI
    let seeds = query_seeds(); // Fixed, unchanged
    let rng = Rng::new(seeds.seed); // Create one for Context

    let key_sender_method = if let InputMethod::Rpc = settings.input_method {
        KeySenderMethod::Rpc(handle, settings.input_method_rpc_server_url.clone())
    } else {
        match settings.capture_mode {
            CaptureMode::BitBlt | CaptureMode::WindowsGraphicsCapture => {
                KeySenderMethod::Default(handle, KeyInputKind::Fixed)
            }
            // This shouldn't matter because we have to get the Handle from the box capture anyway
            CaptureMode::BitBltArea => KeySenderMethod::Default(handle, KeyInputKind::Foreground),
        }
    };
    let mut keys = DefaultKeySender::new(key_sender_method, seeds);
    let key_sender = broadcast::channel::<KeyBinding>(1).0; // Callback to UI
    let mut key_receiver = KeyReceiver::new(handle, KeyInputKind::Fixed);

    let mut capture_handles = Vec::<(String, Handle)>::new();
    let mut selected_capture_handle = None;
    let mut image_capture = ImageCapture::new(handle, settings.capture_mode);
    if let ImageCaptureKind::BitBltArea(capture) = image_capture.kind() {
        key_receiver = KeyReceiver::new(capture.handle(), KeyInputKind::Foreground);
        keys.set_method(KeySenderMethod::Default(
            capture.handle(),
            KeyInputKind::Foreground,
        ));
    }

    let settings = Rc::new(RefCell::new(settings));
    let mut context = Context {
        handle,
        keys: Box::new(keys),
        rng,
        notification: DiscordNotification::new(settings.clone()),
        detector: None,
        minimap: Minimap::Detecting,
        player: Player::Idle,
        skills: [Skill::Detecting],
        buffs: [Buff::No; BuffKind::COUNT],
        halting: true,
        tick: 0,
    };
    let mut player_state = PlayerState::default();
    let mut minimap_state = MinimapState::default();
    let mut skill_states = SkillKind::iter()
        .map(SkillState::new)
        .collect::<Vec<SkillState>>();
    let mut buff_states = BuffKind::iter()
        .map(BuffState::new)
        .collect::<Vec<BuffState>>();
    buff_states.iter_mut().for_each(|state| {
        state.update_enabled_state(&config, &settings.borrow());
    });

    #[cfg(debug_assertions)]
    let mut recording_images_id = None;
    #[cfg(debug_assertions)]
    let mut infering_rune = None;

    loop_with_fps(FPS, || {
        let mat = image_capture.grab().map(OwnedMat::new);
        let was_player_alive = !player_state.is_dead;
        let was_minimap_idle = matches!(context.minimap, Minimap::Idle(_));
        let detector = mat.map(CachedDetector::new);

        context.tick += 1;
        if let Some(detector) = detector {
            context.detector = Some(Box::new(detector));
            context.minimap = fold_context(&context, context.minimap, &mut minimap_state);
            context.player = fold_context(&context, context.player, &mut player_state);
            for (i, state) in skill_states
                .iter_mut()
                .enumerate()
                .take(context.skills.len())
            {
                context.skills[i] = fold_context(&context, context.skills[i], state);
            }
            for (i, state) in buff_states.iter_mut().enumerate().take(context.buffs.len()) {
                context.buffs[i] = fold_context(&context, context.buffs[i], state);
            }
            // Rotating action must always be done last
            rotator.rotate_action(&context, &mut player_state);
        }
        // TODO: Maybe should not downcast but really don't want to public update_input_delay
        // method
        context
            .keys
            .as_any_mut()
            .downcast_mut::<DefaultKeySender>()
            .unwrap()
            .update_input_delay(context.tick);

        // Poll requests, keys and update scheduled notifications frames
        let mut settings_borrow_mut = settings.borrow_mut();
        // I know what you are thinking...
        let mut handler = DefaultRequestHandler {
            context: &mut context,
            config: &mut config,
            settings: &mut settings_borrow_mut,
            buffs: &mut buffs,
            buff_states: &mut buff_states,
            actions: &mut actions,
            rotator: &mut rotator,
            player: &mut player_state,
            minimap: &mut minimap_state,
            key_sender: &key_sender,
            key_receiver: &mut key_receiver,
            image_capture: &mut image_capture,
            capture_handles: &mut capture_handles,
            selected_capture_handle: &mut selected_capture_handle,
            #[cfg(debug_assertions)]
            recording_images_id: &mut recording_images_id,
            #[cfg(debug_assertions)]
            infering_rune: &mut infering_rune,
        };
        handler.poll_request();
        handler.poll_key();
        #[cfg(debug_assertions)]
        handler.poll_debug();
        handler.context.notification.update_scheduled_frames(|| {
            to_png(
                handler
                    .context
                    .detector
                    .as_ref()
                    .map(|detector| detector.mat()),
            )
        });

        // Upon accidental or white roomed causing map to change,
        // abort actions and send notification
        let minimap_changed =
            was_minimap_idle && matches!(handler.context.minimap, Minimap::Detecting);
        let player_died = was_player_alive && handler.player.is_dead;
        if handler.minimap.data().is_some() && !handler.context.halting {
            if player_died || (minimap_changed && handler.settings.stop_on_fail_or_change_map) {
                handler.on_rotate_actions(true);
            }

            if minimap_changed {
                drop(settings_borrow_mut); // For notification to borrow immutably
                let _ = context
                    .notification
                    .schedule_notification(NotificationKind::FailOrMapChange);
            }
        }
    });
}

#[inline]
fn fold_context<C>(
    context: &Context,
    contextual: C,
    persistent: &mut <C as Contextual>::Persistent,
) -> C
where
    C: Contextual,
{
    let mut control_flow = contextual.update(context, persistent);
    loop {
        match control_flow {
            ControlFlow::Immediate(contextual) => {
                control_flow = contextual.update(context, persistent);
            }
            ControlFlow::Next(contextual) => return contextual,
        }
    }
}

#[inline]
fn loop_with_fps(fps: u32, mut on_tick: impl FnMut()) {
    let nanos_per_frame = (1_000_000_000 / fps) as u128;
    loop {
        let start = Instant::now();

        on_tick();

        let now = Instant::now();
        let elapsed_nanos = now.duration_since(start).as_nanos();
        if elapsed_nanos <= nanos_per_frame {
            thread::sleep(Duration::new(0, (nanos_per_frame - elapsed_nanos) as u32));
        }
    }
}

#[inline]
fn to_png(frame: Option<&OwnedMat>) -> Option<Vec<u8>> {
    frame.and_then(|image| {
        let mut bytes = Vector::new();
        imencode_def(".png", image, &mut bytes).ok()?;
        Some(bytes.to_vec())
    })
}
