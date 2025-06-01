use opencv::core::Point;
use platforms::windows::KeyKind;

use super::{
    Player, PlayerActionKey, PlayerState, actions::on_action_state, moving::Moving, use_key::UseKey,
};
use crate::{
    ActionKeyWith,
    context::Context,
    player::{
        MOVE_TIMEOUT, PlayerAction,
        actions::on_auto_mob_use_key_action,
        state::LastMovement,
        timeout::{ChangeAxis, update_moving_axis_context},
    },
};

/// Minimum y distance from the destination required to perform a fall.
pub const FALLING_THRESHOLD: i32 = 4;

/// Maximum y distance from the destination allowed to transition to [`Player::UseKey`] during
/// a [`PlayerAction::Key`] with [`ActionKeyWith::Any`].
const FALLING_TO_USE_KEY_THRESHOLD: i32 = 5;

/// Tick to stop helding down [`KeyKind::Down`] at.
const STOP_DOWN_KEY_TICK: u32 = 3;

/// Maximum number of ticks before timing out.
const TIMEOUT: u32 = MOVE_TIMEOUT + 3;

/// Maximum y distance from the destination allowed to skip normal falling and use teleportation
/// for mage.
const TELEPORT_FALL_THRESHOLD: i32 = 15;

/// Updates the [`Player::Falling`] contextual state.
///
/// This state performs a drop down action. It is completed as soon as the player current `y`
/// position is below `anchor`. If `timeout_on_complete` is true, it will timeout when the
/// action is complete and return to [`Player::Moving`]. Timing out early is currently used by
/// [`Player::DoubleJumping`] to perform a composite action `drop down and then double jump`.
///
/// Before performing a drop down, it will wait for player to become stationary in case the player
/// is already moving. Or if the player is already at destination or lower, it will returns
/// to [`Player::Moving`].
pub fn update_falling_context(
    context: &Context,
    state: &mut PlayerState,
    moving: Moving,
    anchor: Point,
    timeout_on_complete: bool,
) -> Player {
    let cur_pos = state.last_known_pos.unwrap();
    let (y_distance, y_direction) = moving.y_distance_direction_from(true, cur_pos);
    if !moving.timeout.started {
        // Wait until stationary before doing a fall
        if !state.is_stationary {
            return Player::Falling(moving.pos(cur_pos), cur_pos, timeout_on_complete);
        }
        if y_direction >= 0 {
            return Player::Moving(moving.dest, moving.exact, moving.intermediates);
        }
        state.last_movement = Some(LastMovement::Falling);
    }

    let y_changed = cur_pos.y - anchor.y;
    let jump_key = state.config.jump_key;
    let teleport_key = state.config.teleport_key;

    update_moving_axis_context(
        moving,
        cur_pos,
        TIMEOUT,
        |moving| {
            let _ = context.keys.send_down(KeyKind::Down);
            if let Some(key) = teleport_key
                && y_distance < TELEPORT_FALL_THRESHOLD
            {
                let _ = context.keys.send(key);
            } else {
                let _ = context.keys.send(jump_key);
            }
            Player::Falling(moving, anchor, timeout_on_complete)
        },
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Down);
        }),
        |mut moving| {
            if moving.timeout.total == STOP_DOWN_KEY_TICK {
                let _ = context.keys.send_up(KeyKind::Down);
            }
            if !moving.completed && y_changed < 0 {
                moving = moving.completed(true);
            } else if moving.completed && timeout_on_complete {
                moving = moving.timeout_current(TIMEOUT);
            }

            on_action_state(
                state,
                |state, action| {
                    on_player_action(
                        context,
                        cur_pos,
                        action,
                        moving,
                        state.config.teleport_key.is_some(),
                    )
                },
                || Player::Falling(moving, anchor, timeout_on_complete),
            )
        },
        ChangeAxis::Vertical,
    )
}

#[inline]
fn on_player_action(
    context: &Context,
    cur_pos: Point,
    action: PlayerAction,
    moving: Moving,
    has_teleport_key: bool,
) -> Option<(Player, bool)> {
    let (y_distance, y_direction) = moving.y_distance_direction_from(true, cur_pos);
    match action {
        PlayerAction::AutoMob(_) => {
            // Ignore `timeout_on_complete` for auto-mobbing intermediate destination
            if moving.completed && moving.is_destination_intermediate() && y_direction >= 0 {
                let _ = context.keys.send_up(KeyKind::Down);
                return Some((
                    Player::Moving(moving.dest, moving.exact, moving.intermediates),
                    false,
                ));
            }
            let (x_distance, _) = moving.x_distance_direction_from(false, cur_pos);
            let (y_distance, _) = moving.y_distance_direction_from(false, cur_pos);
            on_auto_mob_use_key_action(context, action, cur_pos, x_distance, y_distance)
        }
        PlayerAction::Key(PlayerActionKey {
            with: ActionKeyWith::Any,
            ..
        }) => {
            if has_teleport_key || !moving.completed || y_distance >= FALLING_TO_USE_KEY_THRESHOLD {
                return None;
            }
            Some((Player::UseKey(UseKey::from_action(action)), false))
        }
        PlayerAction::Key(PlayerActionKey {
            with: ActionKeyWith::Stationary | ActionKeyWith::DoubleJump,
            ..
        })
        | PlayerAction::PingPong(_)
        | PlayerAction::Move(_)
        | PlayerAction::FamiliarsSwapping(_)
        | PlayerAction::SolveRune => None,
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use opencv::core::Point;
    use platforms::windows::KeyKind;

    use super::update_falling_context;
    use crate::{
        bridge::MockKeySender,
        context::Context,
        player::{Player, PlayerState, moving::Moving, timeout::Timeout},
    };

    #[test]
    fn falling_start() {
        let pos = Point::new(5, 5);
        let moving = Moving {
            pos,
            dest: Point::new(pos.x, pos.y - 1), // Ensure player is not already at destination
            ..Default::default()
        };

        let mut state = PlayerState::default();
        state.config.jump_key = KeyKind::Space;
        state.is_stationary = true;
        state.last_known_pos = Some(pos);

        let mut keys = MockKeySender::new();
        keys.expect_send_down()
            .withf(|key| matches!(key, KeyKind::Down))
            .once()
            .returning(|_| Ok(()));
        keys.expect_send()
            .withf(|key| matches!(key, KeyKind::Space))
            .once()
            .returning(|_| Ok(()));
        let context = Context::new(Some(keys), None);

        // (1) Send keys if stationary
        update_falling_context(&context, &mut state, moving, Point::default(), false);
        let _ = context.keys; // Drop for test checkpoint

        // (2) Don't send keys if not stationary
        state.is_stationary = false;

        let mut keys = MockKeySender::new();
        keys.expect_send_down().never();
        keys.expect_send().never();
        let context = Context::new(Some(keys), None);

        update_falling_context(&context, &mut state, moving, Point::default(), false);
        let _ = context.keys; // Drop for test checkpoint

        // (3) Don't send keys if already at destination
        state.is_stationary = true;

        let moving = Moving {
            dest: pos,
            ..moving
        };
        let mut keys = MockKeySender::new();
        keys.expect_send_down().never();
        keys.expect_send().never();
        let context = Context::new(Some(keys), None);

        update_falling_context(&context, &mut state, moving, Point::default(), false);
    }

    #[test]
    fn falling_update() {
        let pos = Point::new(5, 5);
        let anchor = Point::new(pos.x, pos.y + 1);
        let dest = Point::new(pos.x, pos.y - 1);
        let moving = Moving {
            pos,
            dest,
            timeout: Timeout {
                started: true,
                total: 2,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut keys = MockKeySender::new();
        keys.expect_send_up()
            .withf(|key| matches!(key, KeyKind::Down))
            .once()
            .returning(|_| Ok(()));
        let context = Context::new(Some(keys), None);

        let mut state = PlayerState::default();
        state.last_known_pos = Some(pos);
        state.is_stationary = true;

        // (1) Send up key because total == 2, so next tick is total == 3 == STOP_DOWN_KEY_TICK
        update_falling_context(&context, &mut state, moving, anchor, false);
        let _ = context.keys; // Drop for test checkpoint

        // (2) Timeout early after complete if enabled
        let moving = Moving {
            completed: true,
            timeout: Timeout {
                total: 3, // So that down arrow key does not send up
                ..moving.timeout
            },
            ..moving
        };
        let player = update_falling_context(&context, &mut state, moving, anchor, true);
        assert_matches!(
            player,
            Player::Falling(
                Moving {
                    timeout: Timeout { current: 8, .. },
                    ..
                },
                _,
                _
            )
        );

        // (3) Do not timeout early after complete if disabled
        let moving = Moving {
            completed: true,
            timeout: Timeout {
                total: 3, // So that down arrow key does not send up
                ..moving.timeout
            },
            ..moving
        };
        let player = update_falling_context(&context, &mut state, moving, anchor, false);
        assert_matches!(
            player,
            Player::Falling(
                Moving {
                    timeout: Timeout { current: 1, .. },
                    ..
                },
                _,
                _
            )
        );
    }

    // TODO: Add tests for handling actions
}
