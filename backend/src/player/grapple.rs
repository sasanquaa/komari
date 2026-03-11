use super::{
    Player, PlayerAction,
    actions::{update_from_auto_mob_action, update_from_ping_pong_action},
    state::LastMovement,
    timeout::{MovingLifecycle, next_moving_lifecycle_with_axis},
};
use crate::{
    ecs::Resources,
    minimap::Minimap,
    player::{MOVE_TIMEOUT, PlayerEntity, moving::Moving, next_action, timeout::ChangeAxis},
};

/// Minimum y distance from the destination required to perform a grappling hook.
pub const GRAPPLING_THRESHOLD: i32 = 24;

/// Maximum y distance from the destination allowed to perform a grappling hook.
pub const GRAPPLING_MAX_THRESHOLD: i32 = 41;

/// Timeout for when grappling is still casting and y position not changed.
const INITIAL_TIMEOUT: u32 = MOVE_TIMEOUT * 8;

/// Timeout after y position started changing.
const STOPPING_TIMEOUT: u32 = MOVE_TIMEOUT + 3;

/// Maximum y distance allowed to stop grappling.
const STOPPING_THRESHOLD: i32 = 3;

#[derive(Clone, Copy, Debug)]
pub struct Grappling {
    pub moving: Moving,
    did_y_changed: bool,
}

impl Grappling {
    pub fn new(moving: Moving) -> Self {
        Self {
            moving,
            did_y_changed: false,
        }
    }

    fn moving(mut self, moving: Moving) -> Self {
        self.moving = moving;
        self
    }
}

/// Updates the [`Player::Grappling`] contextual state.
///
/// This state can only be transitioned via [`Player::Moving`] or [`Player::DoubleJumping`]
/// when the player has reached or close to the destination x-wise.
///
/// This state will use the Rope Lift skill.
pub fn update_grappling_state(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
) {
    let Player::Grappling(mut grappling) = player.state else {
        panic!("state is not grappling");
    };
    let key = player
        .context
        .config
        .grappling_key
        .expect("cannot transition if not set");
    let prev_pos = grappling.moving.pos;
    let timeout = if grappling.did_y_changed {
        STOPPING_TIMEOUT
    } else {
        INITIAL_TIMEOUT
    };

    match next_moving_lifecycle_with_axis(
        grappling.moving,
        player.context.last_known_pos.expect("in positional state"),
        timeout,
        ChangeAxis::Both,
    ) {
        MovingLifecycle::Started(moving) => {
            if player.context.stalling_buffered.stalling() {
                player
                    .context
                    .clear_stalling_buffer_states_if_possible(resources);
                player.state = Player::Grappling(grappling.moving(moving.timeout_started(false)));
                return;
            }

            resources.input.send_key(key);
            player.context.last_movement = Some(LastMovement::Grappling);
            player.state = Player::Grappling(grappling.moving(moving));
        }
        MovingLifecycle::Ended(moving) => {
            player.state = Player::Moving(moving.dest, moving.exact, moving.intermediates);
        }
        MovingLifecycle::Updated(mut moving) => {
            let cur_pos = moving.pos;
            let (y_distance, y_direction) = moving.y_distance_direction_from(true, cur_pos);
            let y_changed = prev_pos.y != cur_pos.y;

            if !grappling.did_y_changed {
                grappling.did_y_changed = y_changed;
            }

            if !moving.completed
                && (y_direction <= 0 || y_distance <= stopping_threshold(player.context.velocity.1))
            {
                resources.input.send_key(key);
                moving.completed = true;
            }

            // Sets initial next state first
            player.state = Player::Grappling(grappling.moving(moving));
            match next_action(&player.context) {
                Some(PlayerAction::AutoMob(mob)) => {
                    if !moving.completed {
                        return;
                    }

                    if moving.is_destination_intermediate() {
                        player.state =
                            Player::Moving(moving.dest, moving.exact, moving.intermediates);
                        return;
                    }

                    if player.context.config.teleport_key.is_some() && !moving.completed {
                        return;
                    }

                    let (x_distance, x_direction) =
                        moving.x_distance_direction_from(false, cur_pos);
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
                Some(PlayerAction::PingPong(ping_pong)) => {
                    if cur_pos.y < ping_pong.bound.y
                        || !resources.rng.random_perlin_bool(
                            cur_pos.x,
                            cur_pos.y,
                            resources.tick,
                            0.7,
                        )
                    {
                        return;
                    }

                    update_from_ping_pong_action(
                        resources,
                        player,
                        minimap_state,
                        ping_pong,
                        cur_pos,
                    );
                }
                None
                | Some(PlayerAction::Key(_) | PlayerAction::Move(_) | PlayerAction::SolveRune) => {}
                _ => unreachable!(),
            }
        }
    }
}

/// Converts vertical velocity to a stopping threshold.
#[inline]
fn stopping_threshold(velocity: f32) -> i32 {
    (STOPPING_THRESHOLD as f32 + 0.7 * velocity).round() as i32
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use mockall::predicate::eq;
    use opencv::core::Point;

    use super::*;
    use crate::{
        bridge::{KeyKind, MockInput},
        player::{PlayerContext, moving::Moving, timeout::Timeout},
    };

    const POS: Point = Point { x: 100, y: 100 };

    fn mock_player_entity_with_grapple(pos: Point) -> PlayerEntity {
        let mut context = PlayerContext::default();
        context.last_known_pos = Some(pos);
        context.config.grappling_key = Some(KeyKind::F);
        context.config.jump_key = KeyKind::Space;

        PlayerEntity {
            state: Player::Idle,
            context,
        }
    }

    fn mock_moving(pos: Point) -> Moving {
        Moving::new(pos, pos, false, None)
    }

    #[test]
    fn update_grappling_state_started() {
        let moving = mock_moving(POS);
        let mut player = mock_player_entity_with_grapple(POS);
        player.state = Player::Grappling(Grappling::new(moving));

        let mut keys = MockInput::new();
        keys.expect_send_key().once().with(eq(KeyKind::F));
        let resources = Resources::new(Some(keys), None);

        update_grappling_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(
            player.state,
            Player::Grappling(Grappling {
                moving: Moving {
                    timeout: Timeout { started: true, .. },
                    ..
                },
                did_y_changed: false
            })
        );
        assert_eq!(player.context.last_movement, Some(LastMovement::Grappling));
    }

    #[test]
    fn update_grappling_state_updated_auto_complete_on_stopping_threshold() {
        let mut moving = mock_moving(POS);
        moving.timeout.started = true;
        moving.timeout.current = STOPPING_TIMEOUT;
        let mut player = mock_player_entity_with_grapple(moving.pos);
        player.state = Player::Grappling(Grappling::new(moving));

        let mut keys = MockInput::new();
        keys.expect_send_key().once().with(eq(KeyKind::F));
        let resources = Resources::new(Some(keys), None);

        update_grappling_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::Grappling(Grappling {
                moving: Moving {
                    completed: true,
                    ..
                },
                ..
            })
        );
    }

    #[test]
    fn update_grappling_state_sets_did_y_changed() {
        let resources = Resources::new(None, None);
        let mut moving = mock_moving(POS);
        moving.timeout.started = true;
        moving.dest = Point {
            y: POS.y + 100,
            x: POS.x,
        };
        let grappling = Grappling::new(moving);
        let mut player = mock_player_entity_with_grapple(moving.pos);
        player.state = Player::Grappling(grappling);

        update_grappling_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::Grappling(Grappling {
                did_y_changed: false,
                ..
            })
        );

        player.context.last_known_pos = Some(Point { x: 100, y: 150 });
        update_grappling_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::Grappling(Grappling {
                did_y_changed: true,
                ..
            })
        );
    }

    // TODO: Add tests for next_action
}
