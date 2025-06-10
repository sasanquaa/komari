use platforms::windows::KeyKind;

use super::{
    Player, PlayerState,
    actions::{PanicTo, on_action},
    timeout::{Timeout, update_with_timeout},
};
use crate::{context::Context, minimap::Minimap};

/// Stages of panicking mode.
#[derive(Debug, Clone, Copy)]
enum PanickingStage {
    /// Cycling through channels.
    ChangingChannel(Timeout),
    /// Going to town.
    GoingToTown,
    Completing(Timeout, bool),
}

#[derive(Debug, Clone, Copy)]
pub struct Panicking {
    stage: PanickingStage,
    to: PanicTo,
}

impl Panicking {
    pub fn new(to: PanicTo) -> Self {
        Self {
            stage: match to {
                PanicTo::Channel => PanickingStage::ChangingChannel(Timeout::default()),
                PanicTo::Town => PanickingStage::GoingToTown,
            },
            to,
        }
    }

    #[inline]
    fn stage_changing_channel(self, timeout: Timeout) -> Panicking {
        Panicking {
            stage: PanickingStage::ChangingChannel(timeout),
            ..self
        }
    }

    #[inline]
    fn stage_completing(self, timeout: Timeout, completed: bool) -> Panicking {
        Panicking {
            stage: PanickingStage::Completing(timeout, completed),
            ..self
        }
    }
}

/// Updates [`Player::Panicking`] contextual state.
pub fn update_panicking_context(
    context: &Context,
    state: &mut PlayerState,
    panicking: Panicking,
) -> Player {
    let panicking = match panicking.stage {
        PanickingStage::ChangingChannel(timeout) => {
            update_changing_channel(context, panicking, timeout)
        }
        PanickingStage::GoingToTown => todo!(),
        PanickingStage::Completing(timeout, completed) => {
            update_completing(context, panicking, timeout, completed)
        }
    };
    let next = if matches!(panicking.stage, PanickingStage::Completing(_, true)) {
        Player::Idle
    } else {
        Player::Panicking(panicking)
    };

    on_action(
        state,
        |_| Some((next, matches!(next, Player::Idle))),
        || Player::Idle, // Force cancel if it is not initiated from an action
    )
}

fn update_changing_channel(context: &Context, panicking: Panicking, timeout: Timeout) -> Panicking {
    update_with_timeout(
        timeout,
        30,
        |timeout| {
            let _ = context.keys.send(KeyKind::Period);
            let _ = context.keys.send(KeyKind::Right);
            let _ = context.keys.send(KeyKind::Enter);
            panicking.stage_changing_channel(timeout)
        },
        || {
            if matches!(context.minimap, Minimap::Idle(_)) {
                panicking.stage_changing_channel(Timeout::default())
            } else {
                panicking.stage_completing(Timeout::default(), false)
            }
        },
        |timeout| panicking.stage_changing_channel(timeout),
    )
}

fn update_completing(
    context: &Context,
    panicking: Panicking,
    timeout: Timeout,
    completed: bool,
) -> Panicking {
    if matches!(panicking.to, PanicTo::Town) {
        return panicking.stage_completing(timeout, true);
    }

    update_with_timeout(
        timeout,
        245,
        |timeout| panicking.stage_completing(timeout, completed),
        || {
            if let Minimap::Idle(idle) = context.minimap {
                if idle.has_any_other_player() {
                    panicking.stage_changing_channel(Timeout::default())
                } else {
                    panicking.stage_completing(timeout, true)
                }
            } else {
                panicking.stage_completing(Timeout::default(), false)
            }
        },
        |timeout| panicking.stage_completing(timeout, completed),
    )
}
