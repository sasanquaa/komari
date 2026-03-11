use super::{
    AutoMob, Player, PlayerAction,
    actions::next_action,
    timeout::{Lifecycle, Timeout, next_timeout_lifecycle},
};
use crate::{Position, player::PlayerEntity};

/// Updates the [`Player::Stalling`] contextual state.
///
/// This state stalls for the specified number of `max_timeout`. Upon timing out,
/// it will return to [`PlayerState::stalling_timeout_state`] if [`Some`] or
/// [`Player::Idle`] if [`None`]. And [`Player::Idle`] is considered the terminal state if
/// there is an action. [`PlayerState::stalling_timeout_state`] is currently only [`Some`] when
/// it is transitioned via [`Player::UseKey`].
///
/// If this state timeout in auto mob with terminal state, it will perform
/// auto mob reachable `y` solidifying if needed.
pub fn update_stalling_state(player: &mut PlayerEntity, timeout: Timeout, max_timeout: u32) {
    let next_state = match next_timeout_lifecycle(timeout, max_timeout) {
        Lifecycle::Started(timeout) => Player::Stalling(timeout, max_timeout),
        Lifecycle::Ended => player
            .context
            .stalling_timeout_state
            .take()
            .unwrap_or(Player::Idle),
        Lifecycle::Updated(timeout) => Player::Stalling(timeout, max_timeout),
    };
    let is_terminal = matches!(next_state, Player::Idle);

    match next_action(&player.context) {
        Some(PlayerAction::AutoMob(AutoMob {
            position: Position { y, .. },
            ..
        })) => {
            if !is_terminal {
                return;
            }

            if player.context.auto_mob_reachable_y_require_update(y) {
                if !player.context.is_stationary {
                    player.state = Player::Stalling(Timeout::default(), max_timeout);
                    return;
                }

                player.context.auto_mob_track_reachable_y(y);
            }

            player.context.clear_action_completed();
        }
        Some(
            action @ (PlayerAction::PingPong(_) | PlayerAction::Key(_) | PlayerAction::Move(_)),
        ) => {
            if is_terminal {
                if !action.is_key_action_without_position() {
                    player.context.clear_unstucking(false);
                }
                player.context.clear_action_completed();
                player.state = next_state;
            }
        }
        Some(PlayerAction::SolveRune) | None => player.state = next_state,
        Some(_) => unreachable!(),
    }
}
