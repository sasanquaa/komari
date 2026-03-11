use std::{
    cell::RefCell,
    env,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use platforms::Error;
use strum::IntoEnumIterator;
use tokio::sync::broadcast::{Sender, channel};

#[cfg(debug_assertions)]
use crate::ecs::Debug;
use crate::{
    bridge::{Capture, DefaultCapture, DefaultInput, InputMethod},
    buff::{self, Buff, BuffContext, BuffEntity, BuffKind},
    database::{query_and_upsert_seeds, query_or_upsert_localization, query_settings},
    detect::{DefaultDetector, Detector},
    ecs::{Resources, World, WorldEvent},
    mat::OwnedMat,
    minimap::{self, Minimap, MinimapContext, MinimapEntity},
    notification::Notification,
    operation::{Operation, OperationConfiguration, OperationState},
    player::{self, Player, PlayerContext, PlayerEntity},
    rng::Rng,
    rotator::{DefaultRotator, Rotator},
    services::Services,
    skill::{self, Skill, SkillContext, SkillEntity, SkillKind},
    task::{Task, Update, update_detection_task},
};

/// The FPS the bot runs at.
///
/// This must **not** be changed as it affects other ticking systems.
pub const FPS: u32 = 30;

/// Milliseconds per tick as an [`u64`].
pub const MS_PER_TICK: u64 = MS_PER_TICK_F32 as u64;

/// Milliseconds per tick as an [`f32`].
pub const MS_PER_TICK_F32: f32 = 1000.0 / FPS as f32;

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

        ort::init_from(dll.to_str().unwrap()).commit().unwrap();
        platforms::init();
        thread::spawn(|| {
            let tokio_rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            let _tokio_guard = tokio_rt.enter();
            tokio_rt.block_on(async {
                systems_loop();
            });
        });
    }
}

fn systems_loop() {
    let settings = Rc::new(RefCell::new(query_settings()));
    let localization = Rc::new(RefCell::new(Arc::new(query_or_upsert_localization())));
    let seeds = query_and_upsert_seeds();
    let rng = Rng::new(seeds.rng_seed, seeds.perlin_seed);
    let (event_tx, _) = channel::<WorldEvent>(5);

    let mut service = Services::new(settings.clone(), localization.clone(), event_tx.subscribe());
    let window = service.selected_window();
    let mut input = DefaultInput::new(window, InputMethod::FocusedDefault, rng.clone());
    let mut capture = DefaultCapture::new(window);
    service.update_window(&mut input, &mut capture);

    let mut rotator = DefaultRotator::default();
    let notification = Notification::new(settings.clone());
    let mut resources = Resources {
        #[cfg(debug_assertions)]
        debug: Debug::default(),
        input: Box::new(input),
        rng,
        notification,
        detector: None,
        operation: Operation {
            config: OperationConfiguration::from(&*settings.borrow()),
            state: OperationState::Halting,
        },
        tick: 0,
    };

    let minimap = MinimapEntity {
        state: Minimap::Detecting,
        context: MinimapContext::default(),
    };
    let player = PlayerEntity {
        state: Player::Idle,
        context: PlayerContext::default(),
    };
    let skills = SkillKind::iter()
        .map(SkillContext::new)
        .map(|context| SkillEntity {
            state: Skill::Detecting,
            context,
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("matching size");

    let buffs = BuffKind::iter()
        .map(BuffContext::new)
        .map(|context| BuffEntity {
            state: Buff::No,
            context,
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("matching size");
    let mut world = World {
        minimap,
        player,
        skills,
        buffs,
    };
    let mut is_capturing_normally = false;

    let mut lie_detector_shape_event_task = event_task(
        WorldEvent::LieDetectorShapeAppeared,
        event_tx.clone(),
        |detector| detector.detect_lie_detector_shape().is_ok(),
    );
    let mut lie_detector_violleta_event_task = event_task(
        WorldEvent::LieDetectorViolettaAppeared,
        event_tx.clone(),
        |detector| detector.detect_lie_detector_violetta().is_ok(),
    );
    let mut elite_boss_event_task = event_task(
        WorldEvent::EliteBossAppeared,
        event_tx.clone(),
        |detector| detector.detect_elite_boss_bar(),
    );

    loop_with_fps(FPS, || {
        let detector = capture
            .grab()
            .and_then(|frame| OwnedMat::new(frame).map_err(|_| Error::WindowInvalidSize))
            .map(|mat| DefaultDetector::new(mat, localization.borrow().clone()));
        let was_capturing_normally = is_capturing_normally;
        let player_in_cash_shop = matches!(world.player.state, Player::CashShopThenExit(_));

        is_capturing_normally = detector.is_ok()
            || (!player_in_cash_shop
                && !matches!(
                    detector,
                    Err(Error::WindowNotFound | Error::WindowInvalidSize)
                ));
        resources.tick += 1;
        if let Ok(detector) = detector {
            let was_running_cycle =
                matches!(resources.operation.state, OperationState::RunUntil { .. });
            let was_player_alive = !world.player.context.is_dead();
            let was_minimap_idle = matches!(world.minimap.state, Minimap::Idle(_));

            resources.detector = Some(Arc::new(detector));
            resources.operation.update_tick();

            minimap::run_system(&resources, &mut world.minimap, world.player.state.clone());
            player::run_system(&resources, &mut world.player, &world.minimap, &world.buffs);
            for skill in world.skills.iter_mut() {
                skill::run_system(&resources, skill, world.player.state.clone());
            }
            for buff in world.buffs.iter_mut() {
                buff::run_system(&resources, buff, world.player.state.clone());
            }

            rotator.rotate_action(&resources, &mut world);

            let did_cycled_to_stop = resources.operation.halting();
            // Go to town on stop cycle
            if was_running_cycle && did_cycled_to_stop {
                let _ = event_tx.send(WorldEvent::RunTimerEnded);
            }

            let player_died = was_player_alive && world.player.context.is_dead();
            if player_died {
                let _ = event_tx.send(WorldEvent::PlayerDied);
            }

            let minimap_detecting = matches!(world.minimap.state, Minimap::Detecting);
            if was_minimap_idle && minimap_detecting {
                let _ = event_tx.send(WorldEvent::MinimapChanged);
            }

            lie_detector_shape_event_task(&resources);
            lie_detector_violleta_event_task(&resources);
            elite_boss_event_task(&resources);
        }

        if was_capturing_normally && !is_capturing_normally {
            let _ = event_tx.send(WorldEvent::CaptureFailed);
        }

        resources.input.update_tick(resources.tick);
        resources
            .notification
            .update(resources.detector.as_ref().map(|detector| detector.mat()));

        service.poll(&mut resources, &mut world, &mut rotator, &mut capture);
    });
}

fn event_task(
    event: WorldEvent,
    event_tx: Sender<WorldEvent>,
    detect_fn: fn(Arc<dyn Detector>) -> bool,
) -> impl FnMut(&Resources) {
    let mut previous = false;
    let mut task: Option<Task<Result<bool>>> = None;
    let task_fn = move |detector: Arc<dyn Detector>| -> Result<bool> { Ok(detect_fn(detector)) };

    move |resources| {
        if resources.detector.is_none() {
            return;
        }

        match update_detection_task(resources, 5000, &mut task, task_fn) {
            Update::Ok(current) => {
                if current && !previous {
                    let _ = event_tx.send(event);
                }
                previous = current;
            }
            Update::Err(_) | Update::Pending => (),
        }
    }
}

#[inline]
fn loop_with_fps(fps: u32, mut on_tick: impl FnMut()) {
    #[cfg(debug_assertions)]
    const LOG_INTERVAL_SECS: u64 = 5;

    let nanos_per_frame = (1_000_000_000 / fps) as u128;
    #[cfg(debug_assertions)]
    let mut last_logged_instant = Instant::now();

    loop {
        let start = Instant::now();

        on_tick();

        let now = Instant::now();
        let elapsed_duration = now.duration_since(start);
        let elapsed_nanos = elapsed_duration.as_nanos();
        if elapsed_nanos <= nanos_per_frame {
            thread::sleep(Duration::new(0, (nanos_per_frame - elapsed_nanos) as u32));
        } else {
            #[cfg(debug_assertions)]
            if now.duration_since(last_logged_instant).as_secs() >= LOG_INTERVAL_SECS {
                use log::debug;

                last_logged_instant = now;
                debug!(target: "backend/context", "ticking running late at {}ms", elapsed_duration.as_millis());
            }
        }
    }
}
