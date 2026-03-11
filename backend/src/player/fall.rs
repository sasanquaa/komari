use opencv::core::Point;

use super::{
    Key, Player,
    moving::Moving,
    timeout::{MovingLifecycle, next_moving_lifecycle_with_axis},
    use_key::UseKey,
};
use crate::{
    ActionKeyWith,
    bridge::KeyKind,
    ecs::Resources,
    minimap::Minimap,
    player::{
        MOVE_TIMEOUT, PlayerAction, PlayerEntity, actions::update_from_auto_mob_action,
        next_action, state::LastMovement, timeout::ChangeAxis,
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
const TELEPORT_FALL_THRESHOLD: i32 = 16;

/// Maximum y distance from the destination allowed to skip normal falling and use teleportation
/// for mage when teleport boost is enabled.
const EXTENDED_TELEPORT_FALL_THRESHOLD: i32 = 20;

#[derive(Clone, Copy, Debug)]
pub struct Falling {
    pub moving: Moving,
    anchor: Point,
    timeout_on_complete: bool,
}

impl Falling {
    pub fn new(moving: Moving, anchor: Point, timeout_on_complete: bool) -> Self {
        Self {
            moving,
            anchor,
            timeout_on_complete,
        }
    }

    fn moving(mut self, moving: Moving) -> Self {
        self.moving = moving;
        self
    }

    fn anchor(mut self, anchor: Point) -> Self {
        self.anchor = anchor;
        self
    }
}

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
pub fn update_falling_state(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
) {
    let Player::Falling(falling) = player.state else {
        panic!("state is not falling")
    };

    match next_moving_lifecycle_with_axis(
        falling.moving,
        player.context.last_known_pos.expect("in positional state"),
        TIMEOUT,
        ChangeAxis::Vertical,
    ) {
        MovingLifecycle::Started(moving) => {
            if player.context.stalling_buffered.stalling() {
                let moving = moving.timeout_started(false);
                let falling = falling.moving(moving);

                player
                    .context
                    .clear_stalling_buffer_states_if_possible(resources);
                player.state = Player::Falling(falling);
                return;
            }

            // Stall until stationary before doing a fall by resetting timeout started
            if !player.context.is_stationary {
                player.state = Player::Falling(
                    falling
                        .moving(moving.timeout_started(false))
                        .anchor(moving.pos),
                );
                return;
            }

            // Check if destination is already reached before starting
            let (y_distance, y_direction) = moving.y_distance_direction_from(true, moving.pos);
            if y_direction >= 0 {
                player.state = Player::Moving(moving.dest, moving.exact, moving.intermediates);
                return;
            }

            // Do the fall
            player.context.last_movement = Some(LastMovement::Falling);
            player.state = Player::Falling(falling.moving(moving));
            resources.input.send_key_down(KeyKind::Down);

            let teleport_fall_threshold = if player.context.config.has_extended_teleport_range {
                EXTENDED_TELEPORT_FALL_THRESHOLD
            } else {
                TELEPORT_FALL_THRESHOLD
            };
            let can_teleport = !player.context.config.disable_teleport_on_fall
                && player.context.config.teleport_key.is_some()
                && y_distance < teleport_fall_threshold;
            if can_teleport {
                resources
                    .input
                    .send_key(player.context.config.teleport_key.unwrap());
            } else {
                resources.input.send_key(player.context.config.jump_key);
            }
        }
        MovingLifecycle::Ended(moving) => {
            resources.input.send_key_up(KeyKind::Down);
            player.state = Player::Moving(moving.dest, moving.exact, moving.intermediates);
        }
        MovingLifecycle::Updated(mut moving) => {
            if moving.timeout.total == STOP_DOWN_KEY_TICK {
                resources.input.send_key_up(KeyKind::Down);
            }
            if !moving.completed {
                let y_changed = moving.pos.y - falling.anchor.y;
                if y_changed < 0 {
                    moving.completed = true;
                }
            } else if falling.timeout_on_complete {
                moving.timeout.current = TIMEOUT;
            }

            // Sets initial next state first
            player.state = Player::Falling(falling.moving(moving));
            update_from_action(resources, player, minimap_state, moving)
        }
    }
}

#[inline]
fn update_from_action(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
    moving: Moving,
) {
    let cur_pos = moving.pos;
    let (y_distance, y_direction) = moving.y_distance_direction_from(true, cur_pos);
    let has_teleport_key = player.context.config.teleport_key.is_some();
    match next_action(&player.context) {
        Some(PlayerAction::AutoMob(mob)) => {
            // Ignore `timeout_on_complete` for auto-mobbing intermediate destination
            if moving.completed && moving.is_destination_intermediate() && y_direction >= 0 {
                resources.input.send_key_up(KeyKind::Down);
                player.state = Player::Moving(moving.dest, moving.exact, moving.intermediates);
                return;
            }

            if has_teleport_key && !moving.completed {
                return;
            }

            let (x_distance, x_direction) = moving.x_distance_direction_from(false, cur_pos);
            let (y_distance, _) = moving.y_distance_direction_from(false, cur_pos);
            update_from_auto_mob_action(
                resources,
                player,
                minimap_state,
                mob,
                x_distance,
                x_direction,
                y_distance,
            )
        }

        Some(PlayerAction::Key(
            key @ Key {
                with: ActionKeyWith::Any,
                ..
            },
        )) => {
            if !has_teleport_key && moving.completed && y_distance < FALLING_TO_USE_KEY_THRESHOLD {
                player.state = Player::UseKey(UseKey::from_key(key));
            }
        }
        Some(
            PlayerAction::Key(Key {
                with: ActionKeyWith::Stationary | ActionKeyWith::DoubleJump,
                ..
            })
            | PlayerAction::PingPong(_)
            | PlayerAction::Move(_)
            | PlayerAction::SolveRune,
        )
        | None => (),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use mockall::predicate::eq;
    use opencv::core::Point;

    use super::*;
    use crate::{
        bridge::{KeyKind, MockInput},
        ecs::Resources,
        minimap::Minimap,
        player::{
            Falling, Player, PlayerContext, PlayerEntity, moving::Moving, state::LastMovement,
            timeout::Timeout,
        },
    };

    const POS: Point = Point { x: 100, y: 100 };

    fn mock_player_entity_with_jump(pos: Point) -> PlayerEntity {
        let mut context = PlayerContext::default();
        context.last_known_pos = Some(pos);
        context.is_stationary = true;
        context.config.jump_key = KeyKind::Space;

        PlayerEntity {
            state: Player::Idle,
            context,
        }
    }

    fn mock_moving(pos: Point, dest: Point) -> Moving {
        Moving {
            pos,
            dest,
            ..Default::default()
        }
    }

    #[test]
    fn update_falling_state_started_presses_down_and_jump() {
        let moving = mock_moving(POS, Point::new(POS.x, POS.y - 5)); // ensures falling
        let mut player = mock_player_entity_with_jump(POS);
        player.state = Player::Falling(Falling {
            moving,
            anchor: Point::default(),
            timeout_on_complete: false,
        });

        let mut keys = MockInput::new();
        keys.expect_send_key_down().once().with(eq(KeyKind::Down));
        keys.expect_send_key().once().with(eq(KeyKind::Space));
        let resources = Resources::new(Some(keys), None);

        update_falling_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(
            player.state,
            Player::Falling(Falling {
                moving: Moving {
                    timeout: Timeout { started: true, .. },
                    ..
                },
                ..
            })
        );
        assert_eq!(player.context.last_movement, Some(LastMovement::Falling));
    }

    #[test]
    fn update_falling_state_started_stalls_when_not_stationary() {
        let moving = mock_moving(POS, Point::new(POS.x, POS.y - 5));
        let mut player = mock_player_entity_with_jump(POS);
        player.context.is_stationary = false;
        player.state = Player::Falling(Falling {
            moving,
            anchor: Point::default(),
            timeout_on_complete: false,
        });

        let mut keys = MockInput::new();
        keys.expect_send_key_down().never();
        keys.expect_send_key().never();
        let resources = Resources::new(Some(keys), None);

        update_falling_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(
            player.state,
            Player::Falling(Falling {
                moving: Moving {
                    timeout: Timeout { started: false, .. },
                    ..
                },
                anchor: POS,
                ..
            })
        );
        assert_eq!(player.context.last_movement, None);
    }

    #[test]
    fn update_falling_state_ended_releases_down_key() {
        let moving = mock_moving(POS, POS)
            .timeout_current(TIMEOUT)
            .timeout_started(true);
        let mut player = mock_player_entity_with_jump(POS);
        player.state = Player::Falling(Falling {
            moving,
            anchor: Point::default(),
            timeout_on_complete: false,
        });

        let mut keys = MockInput::new();
        keys.expect_send_key_up().once().with(eq(KeyKind::Down));
        let resources = Resources::new(Some(keys), None);

        update_falling_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::Moving(_, _, _));
    }

    #[test]
    fn update_falling_updates_releases_down_after_stop_tick() {
        let mut moving = mock_moving(POS, Point::new(POS.x, POS.y - 5)).timeout_started(true);
        moving.timeout.total = STOP_DOWN_KEY_TICK - 1;
        let mut player = mock_player_entity_with_jump(POS);
        player.state = Player::Falling(Falling {
            moving,
            anchor: Point::default(),
            timeout_on_complete: false,
        });

        let mut keys = MockInput::new();
        keys.expect_send_key_up().once().with(eq(KeyKind::Down));
        let resources = Resources::new(Some(keys), None);

        update_falling_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::Falling { .. });
    }

    #[test]
    fn update_falling_completes_and_timeouts_if_enabled() {
        let moving = mock_moving(POS, Point::new(POS.x, POS.y - 5))
            .completed(true)
            .timeout_started(true);
        let mut player = mock_player_entity_with_jump(POS);
        player.state = Player::Falling(Falling {
            moving,
            anchor: Point::default(),
            timeout_on_complete: true,
        });

        let resources = Resources::new(None, None);

        update_falling_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(
            player.state,
            Player::Falling(Falling {
                moving: Moving {
                    completed: true,
                    timeout: Timeout {
                        current: TIMEOUT,
                        ..
                    },
                    ..
                },
                ..
            })
        );
    }

    #[test]
    fn update_falling_completes_without_timeout_if_disabled() {
        let moving = mock_moving(POS, Point::new(POS.x, POS.y - 5))
            .completed(true)
            .timeout_started(true);
        let mut player = mock_player_entity_with_jump(POS);
        player.state = Player::Falling(Falling {
            moving,
            anchor: Point::default(),
            timeout_on_complete: false,
        });

        let resources = Resources::new(None, None);

        update_falling_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(
            player.state,
            Player::Falling(Falling {
                moving: Moving {
                    timeout: Timeout { current: 1, .. },
                    ..
                },
                ..
            })
        );
    }

    // TODO: Add tests for action transitions (AutoMob, UseKey, etc.)
}
