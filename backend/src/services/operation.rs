use std::{
    fmt::Debug,
    time::{Duration, Instant},
};

use log::info;
use tokio::{
    spawn,
    sync::broadcast::{self, Receiver, Sender},
    task::JoinHandle,
    time::sleep,
};

use super::EventContext;
use crate::{
    OperationUpdate,
    ecs::Resources,
    operation::{Operation, OperationConfiguration, OperationState},
    player::{Panic, PanicTo, PlayerAction},
    services::{Event, EventHandler},
};

const PENDING_HALT_SECS: u64 = 12;

#[derive(Debug, Clone, Copy)]
pub struct Halt {
    pub go_to_town: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum OperationEvent {
    Halt(Halt),
    Update,
    Configuration,
}

impl Event for OperationEvent {}

/// A service to handle operation-related incoming requests.
pub trait OperationService: Debug {
    /// Subscribes for [`OperationEvent`].
    fn subscribe(&self) -> Receiver<OperationEvent>;

    /// Applies the new `update` to `resources` and sends an [`OperationEvent::Update`] event.
    fn update(&mut self, resources: &mut Resources, update: OperationUpdate);

    /// Applies the new `config` to `resources` and sends an [`OperationEvent::Configuration`]
    /// event.
    fn config(&self, resources: &mut Resources, config: OperationConfiguration);

    /// Queues a [`OperationEvent::Halt`] event.
    fn queue_halt(&mut self, immediate: bool, halt: Halt);

    /// Aborts the previous [`OperationService::queue_halt`] if possible.
    fn abort_halt(&mut self);
}

#[derive(Debug)]
pub struct DefaultOperationService {
    pending_halt: Option<JoinHandle<()>>,
    event_tx: Sender<OperationEvent>,
}

impl Default for DefaultOperationService {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(5);

        Self {
            pending_halt: None,
            event_tx: tx,
        }
    }
}

impl OperationService for DefaultOperationService {
    fn subscribe(&self) -> Receiver<OperationEvent> {
        self.event_tx.subscribe()
    }

    fn update(&mut self, resources: &mut Resources, update: OperationUpdate) {
        resources.operation = update_operation(resources.operation, update);
        if resources.operation.halting() {
            self.abort_halt();
        }
        let _ = self.event_tx.send(OperationEvent::Update);
    }

    fn config(&self, resources: &mut Resources, config: OperationConfiguration) {
        resources.operation = config_operation(resources.operation, config);
        let _ = self.event_tx.send(OperationEvent::Configuration);
    }

    fn queue_halt(&mut self, immediate: bool, halt: Halt) {
        self.abort_halt();

        let event = OperationEvent::Halt(halt);

        if immediate {
            let _ = self.event_tx.send(event);
        } else {
            let tx = self.event_tx.clone();
            let duration = Duration::from_secs(PENDING_HALT_SECS);
            let handle = spawn(async move {
                sleep(duration).await;
                let _ = tx.send(event);
            });

            self.pending_halt = Some(handle);
        }
    }

    fn abort_halt(&mut self) {
        if let Some(handle) = self.pending_halt.take() {
            handle.abort();
        }
    }
}

fn update_operation(mut operation: Operation, update: OperationUpdate) -> Operation {
    match update {
        OperationUpdate::TemporaryHalt => {
            operation.state = if let OperationState::RunUntil { instant } = operation.state {
                OperationState::TemporaryHalting {
                    resume: instant.saturating_duration_since(Instant::now()),
                }
            } else {
                OperationState::Halting
            };
        }
        OperationUpdate::Halt => operation.state = OperationState::Halting,
        OperationUpdate::Run => match operation.state {
            OperationState::TemporaryHalting { resume } => {
                operation.state = OperationState::RunUntil {
                    instant: Instant::now() + resume,
                };
            }
            OperationState::Halting => {
                operation.state = if operation.config.run_timer {
                    OperationState::run_until(operation.config)
                } else {
                    OperationState::Running
                };
            }
            _ => {
                info!(target: "backend/operation", "invalid run update provided for the current state");
            }
        },
    }

    operation
}

fn config_operation(mut operation: Operation, config: OperationConfiguration) -> Operation {
    match operation.state {
        OperationState::TemporaryHalting { resume } => {
            operation.state = if operation.config.run_timer_millis != config.run_timer_millis {
                OperationState::Halting
            } else {
                OperationState::TemporaryHalting { resume }
            };
        }
        OperationState::Halting => operation.state = OperationState::Halting,
        OperationState::Running | OperationState::RunUntil { .. } => {
            if config.run_timer {
                operation.state = OperationState::run_until(config);
            } else {
                operation.state = OperationState::Running;
            }
        }
    }

    operation.config = config;
    operation
}

pub struct OperationEventHandler;

impl EventHandler<OperationEvent> for OperationEventHandler {
    fn handle(&mut self, context: &mut EventContext<'_>, event: OperationEvent) {
        match event {
            OperationEvent::Halt(Halt { go_to_town }) => {
                context.resources.operation =
                    update_operation(context.resources.operation, OperationUpdate::TemporaryHalt);
                context.rotator.reset_queue();
                context
                    .world
                    .player
                    .context
                    .clear_actions_aborted(!go_to_town);

                if go_to_town {
                    context
                        .rotator
                        .inject_action(PlayerAction::Panic(Panic { to: PanicTo::Town }));
                }
            }
            OperationEvent::Update | OperationEvent::Configuration => {
                if context.resources.operation.halting() {
                    context.rotator.reset_queue();
                    context.world.player.context.clear_actions_aborted(true);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        assert_matches::assert_matches,
        time::{Duration, Instant},
    };

    use super::*;

    fn base_config(run_timer: bool) -> OperationConfiguration {
        OperationConfiguration {
            run_timer,
            run_timer_millis: 1_000,
        }
    }

    fn op_with_state(state: OperationState, config: OperationConfiguration) -> Operation {
        Operation { state, config }
    }

    #[test]
    fn update_temporary_halt_from_run_until_keeps_remaining_duration() {
        let config = base_config(true);
        let future = Instant::now() + Duration::from_secs(5);

        let op = op_with_state(OperationState::RunUntil { instant: future }, config);
        let updated = update_operation(op, OperationUpdate::TemporaryHalt);

        match updated.state {
            OperationState::TemporaryHalting { resume } => {
                assert!(resume.as_secs() <= 5);
                assert!(resume.as_secs() > 0);
            }
            _ => panic!("expected TemporaryHalting"),
        }
    }

    #[test]
    fn update_temporary_halt_from_non_run_state_becomes_halting() {
        let config = base_config(true);
        let op = op_with_state(OperationState::Running, config);

        let updated = update_operation(op, OperationUpdate::TemporaryHalt);

        assert_matches!(updated.state, OperationState::Halting);
    }

    #[test]
    fn update_halt_always_forces_halting() {
        let config = base_config(true);
        let op = op_with_state(OperationState::Running, config);

        let updated = update_operation(op, OperationUpdate::Halt);

        assert_matches!(updated.state, OperationState::Halting);
    }

    #[test]
    fn update_run_from_temporary_halting_restores_run_until() {
        let config = base_config(true);
        let resume = Duration::from_secs(3);

        let op = op_with_state(OperationState::TemporaryHalting { resume }, config);

        let updated = update_operation(op, OperationUpdate::Run);

        match updated.state {
            OperationState::RunUntil { instant } => {
                assert!(instant > Instant::now());
            }
            _ => panic!("expected RunUntil"),
        }
    }

    #[test]
    fn update_run_from_halting_no_timer_to_running() {
        let config = base_config(false);
        let op = op_with_state(OperationState::Halting, config);

        let updated = update_operation(op, OperationUpdate::Run);

        assert_matches!(updated.state, OperationState::Running);
    }

    #[test]
    fn config_from_temporary_halting_invalidates_if_run_duration_changes() {
        let old = base_config(true);
        let mut new = base_config(true);
        new.run_timer_millis += 500;

        let op = op_with_state(
            OperationState::TemporaryHalting {
                resume: Duration::from_secs(2),
            },
            old,
        );

        let updated = config_operation(op, new);

        assert_matches!(updated.state, OperationState::Halting);
    }

    #[test]
    fn config_from_running_to_repeat_enters_run_until() {
        let old = base_config(false);
        let new = base_config(true);

        let op = op_with_state(OperationState::Running, old);

        let updated = config_operation(op, new);

        assert_matches!(updated.state, OperationState::RunUntil { .. });
    }
}
