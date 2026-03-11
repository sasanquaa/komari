use super::EventContext;
use crate::{
    OperationUpdate,
    ecs::WorldEvent,
    notification::NotificationKind,
    player::{PanicTo, Panicking, Player},
    services::{EventHandler, operation::Halt},
};

pub struct WorldEventHandler;

impl EventHandler<WorldEvent> for WorldEventHandler {
    fn handle(&mut self, context: &mut EventContext<'_>, event: WorldEvent) {
        match event {
            WorldEvent::RunTimerEnded => {
                context
                    .operation_service
                    .queue_halt(true, Halt { go_to_town: true });
                if context
                    .settings_service
                    .settings()
                    .notifications
                    .notify_on_run_timer_end
                {
                    context
                        .resources
                        .notification
                        .schedule_notification(NotificationKind::RunTimerEnded);
                }
            }
            WorldEvent::PlayerDied => {
                if context.settings_service.settings().stop_on_player_die {
                    context
                        .operation_service
                        .queue_halt(true, Halt { go_to_town: false });
                }
            }
            WorldEvent::MinimapChanged => {
                if context.resources.operation.halting() {
                    return;
                }

                let is_panicking = matches!(
                    context.world.player.state,
                    Player::Panicking(Panicking {
                        to: PanicTo::Channel,
                        ..
                    })
                );
                if is_panicking {
                    return;
                }

                context
                    .resources
                    .notification
                    .schedule_notification(NotificationKind::FailOrMapChange);
                context.operation_service.abort_halt();

                if !context
                    .settings_service
                    .settings()
                    .stop_on_fail_or_change_map
                {
                    return;
                }

                context
                    .operation_service
                    .queue_halt(false, Halt { go_to_town: true });
            }
            WorldEvent::CaptureFailed => {
                if context.resources.operation.halting() {
                    return;
                }

                if context
                    .settings_service
                    .settings()
                    .stop_on_fail_or_change_map
                {
                    context
                        .operation_service
                        .update(context.resources, OperationUpdate::TemporaryHalt);
                }
                context
                    .resources
                    .notification
                    .schedule_notification(NotificationKind::FailOrMapChange);
            }
            WorldEvent::LieDetectorShapeAppeared => {
                if !context.resources.operation.halting() {
                    context
                        .resources
                        .notification
                        .schedule_notification(NotificationKind::LieDetectorShapeAppear);
                }
            }
            WorldEvent::LieDetectorViolettaAppeared => {
                if !context.resources.operation.halting() {
                    context
                        .resources
                        .notification
                        .schedule_notification(NotificationKind::LieDetectorViolettaAppear);
                }
            }
            WorldEvent::EliteBossAppeared => {
                if !context.resources.operation.halting() {
                    context
                        .resources
                        .notification
                        .schedule_notification(NotificationKind::EliteBossAppear);
                }
            }
        }
    }
}
