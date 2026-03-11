use std::cmp::Ordering;

use opencv::core::Point;

use super::{
    Key, PingPongDirection, Player, PlayerAction,
    actions::{PingPong, update_from_auto_mob_action},
    moving::Moving,
    timeout::{
        Lifecycle, MovingLifecycle, next_moving_lifecycle_with_axis, next_timeout_lifecycle,
    },
    up_jump::UpJumping,
    use_key::UseKey,
};
use crate::{
    ActionKeyDirection, ActionKeyWith,
    bridge::KeyKind,
    ecs::Resources,
    minimap::Minimap,
    player::{
        PlayerEntity,
        fall::Falling,
        grapple::Grappling,
        moving::MOVE_TIMEOUT,
        next_action,
        state::LastMovement,
        timeout::{ChangeAxis, Timeout},
    },
};

/// Minimum x distance from the destination required to perform a double jump.
pub const DOUBLE_JUMP_THRESHOLD: i32 = 25;

/// Minimum x distance from the destination required to perform a double jump in auto mobbing.
pub const DOUBLE_JUMP_AUTO_MOB_THRESHOLD: i32 = 15;

/// Minimum x distance from the destination required to transition to [`Player::UseKey`].
const USE_KEY_X_THRESHOLD: i32 = DOUBLE_JUMP_THRESHOLD;

/// Minimum y distance from the destination required to transition to [`Player::UseKey`].
const USE_KEY_Y_THRESHOLD: i32 = 10;

/// Maximum number of ticks before timing out.
const TIMEOUT: u32 = MOVE_TIMEOUT;

const TIMEOUT_FORCED: u32 = 20;

/// Number of ticks to wait after a double jump.
///
/// A heuristic to mostly avoid mid-air jump keys sending. The current approach of using velocity
/// does not send much keys after double jumped, but only few are sent mid-air.
const COOLDOWN_TIMEOUT: u32 = MOVE_TIMEOUT;

/// Minimum x distance from the destination required to transition to [`Player::Grappling`].
const GRAPPLING_THRESHOLD: i32 = 4;

/// Minimum x velocity to be considered as double jumped.
const X_VELOCITY_THRESHOLD: f32 = 1.0;

/// Maximum x velocity allowed to be considered as near stationary.
const X_NEAR_STATIONARY_VELOCITY_THRESHOLD: f32 = 0.75;

/// Maximum y velocity allowed to be considered as near stationary.
const Y_NEAR_STATIONARY_VELOCITY_THRESHOLD: f32 = 0.4;

/// Minimum y distance required from the middle y of ping pong bound to allow randomization.
const PING_PONG_IGNORE_RANDOMIZE_Y_THRESHOLD: i32 = 9;

#[derive(Copy, Clone, Debug)]
pub struct DoubleJumping {
    pub moving: Moving,
    /// Whether to force a double jump even when the player current position is already close to
    /// the destination.
    pub forced: bool,
    /// Whether to wait for the player is about to become stationary before sending jump keys.
    require_near_stationary: bool,
    /// Timeout for between double jump cooldown.
    cooldown_timeout: Timeout,
}

impl DoubleJumping {
    pub fn new(moving: Moving, forced: bool, require_stationary: bool) -> Self {
        Self {
            moving,
            forced,
            require_near_stationary: require_stationary,
            cooldown_timeout: Timeout::default(),
        }
    }

    #[inline]
    fn moving(self, moving: Moving) -> DoubleJumping {
        DoubleJumping { moving, ..self }
    }

    #[inline]
    fn update_jump_cooldown(&mut self) {
        self.cooldown_timeout =
            match next_timeout_lifecycle(self.cooldown_timeout, COOLDOWN_TIMEOUT) {
                Lifecycle::Started(timeout) => timeout,
                Lifecycle::Ended => Timeout::default(),
                Lifecycle::Updated(timeout) => timeout,
            };
    }
}

/// Updates the [`Player::DoubleJumping`] contextual state.
///
/// This state continues to double jump as long as the distance x-wise is still
/// `>= DOUBLE_JUMP_THRESHOLD`. Or when [`DoubleJumping::forced`], this state will attempt
/// a single double jump. When [`DoubleJumping::require_stationary`], this state will wait for
/// the player to be stationary before double jumping.
///
/// [`DoubleJumping::forced`] is currently true when it is transitioned
/// from [`Player::Idle`], [`Player::Moving`], [`Player::Adjusting`], and
/// [`Player::UseKey`] with [`PlayerState::last_known_direction`] matches the
/// [`PlayerAction::Key`] direction.
///
/// [`DoubleJumping::require_stationary`] is currently true when it is transitioned
/// from [`Player::Idle`] and [`Player::UseKey`] with [`PlayerState::last_known_direction`] matches
/// the [`PlayerAction::Key`] direction.
pub fn update_double_jumping_state(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
) {
    let Player::DoubleJumping(double_jumping) = player.state else {
        panic!("state is not double jumping")
    };
    let moving = double_jumping.moving;
    let ignore_grappling = player.context.config.disable_grapple_on_double_jumping
        || double_jumping.forced
        || player.context.should_disable_grappling();
    let is_intermediate = moving.is_destination_intermediate();
    let timeout = if double_jumping.forced {
        TIMEOUT_FORCED
    } else {
        TIMEOUT
    };
    let axis = if double_jumping.forced {
        // This ensures it won't double jump forever when jumping towards either
        // edges of the map.
        ChangeAxis::Horizontal
    } else {
        ChangeAxis::Both
    };

    match next_moving_lifecycle_with_axis(
        moving,
        player.context.last_known_pos.expect("in positional state"),
        timeout,
        axis,
    ) {
        MovingLifecycle::Started(moving) => {
            // Stall until near stationary by resetting started
            let (x_velocity, y_velocity) = player.context.velocity;
            if double_jumping.require_near_stationary
                && (x_velocity > X_NEAR_STATIONARY_VELOCITY_THRESHOLD
                    || y_velocity > Y_NEAR_STATIONARY_VELOCITY_THRESHOLD)
            {
                player.state =
                    Player::DoubleJumping(double_jumping.moving(moving.timeout_started(false)));
                return;
            }

            player.context.last_movement = Some(LastMovement::DoubleJumping);
            player.state = Player::DoubleJumping(double_jumping.moving(moving));
        }
        MovingLifecycle::Ended(moving) => {
            resources.input.send_key_up(KeyKind::Right);
            resources.input.send_key_up(KeyKind::Left);
            player.state = Player::Moving(moving.dest, moving.exact, moving.intermediates);
        }
        MovingLifecycle::Updated(mut moving) => {
            let (x_distance, x_direction) = moving.x_distance_direction_from(true, moving.pos);
            let mut double_jumping = double_jumping;

            // Movement logics
            if !moving.completed {
                if !double_jumping.forced || player.context.config.teleport_key.is_some() {
                    let option = match x_direction.cmp(&0) {
                        Ordering::Greater => {
                            Some((KeyKind::Right, KeyKind::Left, ActionKeyDirection::Right))
                        }
                        Ordering::Less => {
                            Some((KeyKind::Left, KeyKind::Right, ActionKeyDirection::Left))
                        }
                        _ => {
                            // Mage teleportation requires a direction
                            if player.context.config.teleport_key.is_some() {
                                get_mage_teleport_direction(player.context.last_known_direction)
                            } else {
                                None
                            }
                        }
                    };
                    if let Some((key_down, key_up, direction)) = option {
                        resources.input.send_key_down(key_down);
                        resources.input.send_key_up(key_up);
                        player.context.last_known_direction = direction;
                    }
                }

                let can_continue = !double_jumping.forced
                    && x_distance >= player.context.double_jump_threshold(is_intermediate);
                let can_press =
                    double_jumping.forced && player.context.velocity.0 <= X_VELOCITY_THRESHOLD;
                if can_continue || can_press {
                    if !double_jumping.cooldown_timeout.started
                        && player.context.velocity.0 <= X_VELOCITY_THRESHOLD
                    {
                        resources.input.send_key(
                            player
                                .context
                                .config
                                .teleport_key
                                .unwrap_or(player.context.config.jump_key),
                        );
                    } else {
                        double_jumping.update_jump_cooldown();
                    }
                } else {
                    resources.input.send_key_up(KeyKind::Right);
                    resources.input.send_key_up(KeyKind::Left);
                    moving.completed = true;
                }
            }

            // Computes and sets initial next state first
            player.state = next_updated_state(double_jumping, moving, x_distance, ignore_grappling);
            update_from_action(
                resources,
                player,
                minimap_state,
                moving,
                double_jumping.forced,
            );
        }
    }
}

fn next_updated_state(
    double_jumping: DoubleJumping,
    moving: Moving,
    x_distance: i32,
    ignore_grappling: bool,
) -> Player {
    if !ignore_grappling && moving.completed && x_distance <= GRAPPLING_THRESHOLD {
        let (_, y_direction) = moving.y_distance_direction_from(true, moving.pos);
        if y_direction > 0 {
            return Player::Grappling(Grappling::new(
                moving.completed(false).timeout(Timeout::default()),
            ));
        }
    }

    if moving.completed {
        Player::DoubleJumping(double_jumping.moving(moving.timeout_current(TIMEOUT)))
    } else {
        Player::DoubleJumping(double_jumping.moving(moving))
    }
}

/// Handles [`PlayerAction`] during double jump.
///
/// It currently handles action for auto mob and a key action with [`ActionKeyWith::Any`] or
/// [`ActionKeyWith::DoubleJump`]. For auto mob, the same handling logics is reused. For the other,
/// it will try to transition to [`Player::UseKey`] when the player is close enough.
fn update_from_action(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
    moving: Moving,
    forced: bool,
) {
    let cur_pos = moving.pos;
    let (x_distance, x_direction) = moving.x_distance_direction_from(false, cur_pos);
    let (y_distance, _) = moving.y_distance_direction_from(false, cur_pos);
    let double_jumped_or_flying = player.context.velocity.0 > X_VELOCITY_THRESHOLD;

    match next_action(&player.context) {
        Some(PlayerAction::PingPong(ping_pong)) => update_from_ping_pong_action(
            resources,
            player,
            ping_pong,
            cur_pos,
            double_jumped_or_flying,
        ),
        Some(PlayerAction::AutoMob(mob)) => update_from_auto_mob_action(
            resources,
            player,
            minimap_state,
            mob,
            x_distance,
            x_direction,
            y_distance,
        ),
        Some(PlayerAction::Key(
            key @ Key {
                with: ActionKeyWith::DoubleJump | ActionKeyWith::Any,
                ..
            },
        )) => {
            if !moving.completed {
                return;
            }

            // Ignore proximity check when it is forced to double jumped as this indicates the
            // player is already near the destination.
            if forced
                || (!moving.exact
                    && x_distance <= USE_KEY_X_THRESHOLD
                    && y_distance <= USE_KEY_Y_THRESHOLD)
            {
                player.state = Player::UseKey(UseKey::from_key(key));
            }
        }
        None
        | Some(
            PlayerAction::Key(Key {
                with: ActionKeyWith::Stationary,
                ..
            })
            | PlayerAction::SolveRune
            | PlayerAction::Move { .. },
        ) => (),
        _ => unreachable!(),
    }
}

/// Handles ping pong action during double jump.
///
/// This function checks for specific conditions to decide whether to:
/// - Transition to [`Player::Idle`] when player hits horizontal bounds
/// - If the player has double jumped or already flying:
///   - Transition to [`Player::Falling`] or [`Player::UpJumping`] with a chance to simulate vertical movement
///   - Transition to [`Player::UseKey`] otherwise
#[inline]
fn update_from_ping_pong_action(
    resources: &Resources,
    player: &mut PlayerEntity,
    ping_pong: PingPong,
    cur_pos: Point,
    double_jumped: bool,
) {
    let bound = ping_pong.bound;
    let hit_x_bound_edge = match ping_pong.direction {
        PingPongDirection::Left => cur_pos.x - bound.x <= 0,
        PingPongDirection::Right => cur_pos.x - bound.x - bound.width >= 0,
    };
    if hit_x_bound_edge {
        player.context.clear_unstucking(false);
        player.state = Player::Idle;
        return;
    }

    if !double_jumped {
        return;
    }

    // TODO: Add test
    if player.context.stalling_buffered.stalling() {
        player.state = Player::Idle;
        return;
    }

    resources.input.send_key_up(KeyKind::Left);
    resources.input.send_key_up(KeyKind::Right);
    let bound_y_max = bound.y + bound.height;
    let bound_y_mid = bound.y + bound.height / 2;

    let has_grappling = player.context.config.grappling_key.is_some();
    let allow_randomize = (cur_pos.y - bound_y_mid).abs() >= PING_PONG_IGNORE_RANDOMIZE_Y_THRESHOLD;
    let upward_bias = allow_randomize && cur_pos.y < bound_y_mid;
    let downward_bias = allow_randomize && cur_pos.y > bound_y_mid;
    let should_upward = upward_bias
        && resources
            .rng
            .random_perlin_bool(cur_pos.x, cur_pos.y, resources.tick, 0.35);
    let should_downward = downward_bias
        && resources
            .rng
            .random_perlin_bool(cur_pos.x, cur_pos.y, resources.tick + 100, 0.25);

    if cur_pos.y < bound.y || should_upward {
        let moving = Moving::new(
            cur_pos,
            Point::new(cur_pos.x, bound.y + bound.height),
            false,
            None,
        );
        player.state = if has_grappling {
            Player::Grappling(Grappling::new(moving))
        } else {
            Player::UpJumping(UpJumping::new(moving, resources, &player.context))
        };
        return;
    }

    if cur_pos.y > bound_y_max || should_downward {
        player.state = Player::Falling(Falling::new(
            Moving::new(cur_pos, Point::new(cur_pos.x, bound.y), false, None),
            cur_pos,
            true,
        ));
        return;
    }

    player.state = Player::UseKey(UseKey::from_ping_pong(ping_pong));
}

/// Gets the mage teleport direction when the player is already at destination.
fn get_mage_teleport_direction(
    last_known_direction: ActionKeyDirection,
) -> Option<(KeyKind, KeyKind, ActionKeyDirection)> {
    // FIXME: Currently, PlayerActionKey with double jump + has position + has direction:
    //  1. Double jump near proximity
    //  2. Transition to UseKey and update direction
    //  3. Transition back to double jump
    //  4. Use last_known_direction to double jump
    //
    // This will cause mage to teleport to the opposite direction of destination, which is not
    // desired. The desired behavior would be to use skill near the destination in the direction
    // specified by PlayerActionKey. HOW TO FIX?
    match last_known_direction {
        // Clueless
        ActionKeyDirection::Any => None,
        ActionKeyDirection::Right => {
            Some((KeyKind::Right, KeyKind::Left, ActionKeyDirection::Right))
        }
        ActionKeyDirection::Left => Some((KeyKind::Left, KeyKind::Right, ActionKeyDirection::Left)),
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use mockall::predicate::eq;
    use opencv::core::{Point, Rect};

    use super::{update_double_jumping_state, update_from_ping_pong_action};
    use crate::{
        ActionKeyDirection,
        bridge::{KeyKind, MockInput},
        ecs::Resources,
        minimap::Minimap,
        player::{
            PingPong, PingPongDirection, Player, PlayerAction, PlayerContext, PlayerEntity,
            double_jump::DoubleJumping, moving::Moving, state::LastMovement, timeout::Timeout,
        },
    };

    fn make_player_with_state(state: Player) -> PlayerEntity {
        PlayerEntity {
            state,
            context: PlayerContext::default(),
        }
    }

    #[test]
    fn update_double_jumping_state_started_sets_last_movement() {
        let pos = Point::new(0, 0);
        let dest = Point::new(30, 0);
        let mut player = make_player_with_state(Player::DoubleJumping(DoubleJumping::new(
            Moving::new(pos, dest, false, None),
            false,
            false,
        )));
        player.context.last_known_pos = Some(pos);
        let resources = Resources::new(None, None);

        update_double_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::DoubleJumping(_));
        assert_eq!(
            player.context.last_movement,
            Some(LastMovement::DoubleJumping)
        );
    }

    #[test]
    fn update_double_jumping_state_updated_sends_correct_direction_and_jump() {
        let pos = Point::new(100, 50);
        let dest = Point::new(50, 50); // Move left
        let moving = Moving {
            pos,
            dest,
            timeout: Timeout {
                started: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut player = make_player_with_state(Player::DoubleJumping(DoubleJumping::new(
            moving, false, false,
        )));
        player.context.last_known_pos = Some(pos);
        player.context.config.jump_key = KeyKind::Space;
        let mut keys = MockInput::new();
        keys.expect_send_key_down()
            .withf(|k| matches!(k, KeyKind::Left))
            .once();
        keys.expect_send_key_up()
            .withf(|k| matches!(k, KeyKind::Right))
            .once();
        keys.expect_send_key()
            .withf(|k| matches!(k, KeyKind::Space))
            .once();
        let resources = Resources::new(Some(keys), None);

        update_double_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::DoubleJumping(_));
    }

    #[test]
    fn update_double_jumping_state_forced_only_presses_jump() {
        let mut player = make_player_with_state(Player::DoubleJumping(DoubleJumping::new(
            Moving::new(Point::new(0, 0), Point::new(0, 0), true, None).timeout_started(true),
            true, // forced
            false,
        )));
        player.context.last_known_pos = Some(Point::new(0, 0));
        player.context.velocity = (0.5, 0.0);
        player.context.config.jump_key = KeyKind::Space;
        let mut keys = MockInput::new();
        keys.expect_send_key().with(eq(KeyKind::Space)).once();
        keys.expect_send_key_down().never();
        keys.expect_send_key_up().never();
        let resources = Resources::new(Some(keys), None);

        update_double_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::DoubleJumping(_));
    }

    #[test]
    fn update_double_jumping_state_started_requires_stationary_and_stalls() {
        let pos = Point::new(0, 0);
        let mut player = make_player_with_state(Player::DoubleJumping(DoubleJumping::new(
            Moving::new(pos, Point::new(50, 0), false, None),
            false,
            true, // require_stationary
        )));
        player.context.last_known_pos = Some(pos);
        player.context.velocity = (1.5, 0.5); // too fast
        let resources = Resources::new(None, None);

        update_double_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::DoubleJumping(DoubleJumping { moving, .. })
            if !moving.timeout.started);
    }

    #[test]
    fn update_double_jumping_state_mage_requires_direction_even_when_x_zero() {
        let pos = Point::new(100, 50);
        let moving = Moving {
            pos,
            dest: pos,
            timeout: Timeout {
                started: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut player = make_player_with_state(Player::DoubleJumping(DoubleJumping::new(
            moving, true, false,
        )));
        player.context.last_known_pos = Some(pos);
        player.context.last_known_direction = ActionKeyDirection::Right;
        player.context.config.teleport_key = Some(KeyKind::Shift);
        let mut keys = MockInput::new();
        keys.expect_send_key_down().with(eq(KeyKind::Right)).once();
        keys.expect_send_key_up().with(eq(KeyKind::Left)).once();
        keys.expect_send_key().with(eq(KeyKind::Shift)).once();
        let resources = Resources::new(Some(keys), None);

        update_double_jumping_state(&resources, &mut player, Minimap::Detecting);
    }

    #[test]
    fn update_from_ping_pong_action_hits_left_bound_goes_idle() {
        let cur_pos = Point::new(10, 100);
        let bound = Rect::new(20, 90, 40, 20);
        let ping_pong = PingPong {
            bound,
            direction: PingPongDirection::Left,
            ..Default::default()
        };
        let mut player = make_player_with_state(Player::DoubleJumping(DoubleJumping::new(
            Moving::new(cur_pos, Point::new(30, 100), false, None),
            false,
            false,
        )));
        player
            .context
            .set_normal_action(None, PlayerAction::PingPong(ping_pong));
        let resources = Resources::new(None, None);

        update_from_ping_pong_action(&resources, &mut player, ping_pong, cur_pos, true);

        assert_matches!(player.state, Player::Idle);
    }

    #[test]
    fn update_from_ping_pong_action_before_double_jump_no_transition() {
        let cur_pos = Point::new(30, 100);
        let bound = Rect::new(20, 90, 40, 20);
        let ping_pong = PingPong {
            bound,
            direction: PingPongDirection::Right,
            ..Default::default()
        };
        let mut player = make_player_with_state(Player::DoubleJumping(DoubleJumping::new(
            Moving::new(cur_pos, Point::new(40, 100), false, None),
            false,
            false,
        )));
        player
            .context
            .set_normal_action(None, PlayerAction::PingPong(ping_pong));
        player.context.config.grappling_key = Some(KeyKind::A);
        let resources = Resources::new(None, None);

        update_from_ping_pong_action(
            &resources,
            &mut player,
            ping_pong,
            cur_pos,
            false, // hasn't jumped yet
        );

        // No state change expected
        assert_matches!(player.state, Player::DoubleJumping(_));
    }

    #[test]
    fn update_from_ping_pong_action_transition_to_upjumping_or_grappling() {
        let cur_pos = Point::new(30, 79);
        let bound = Rect::new(20, 80, 40, 20);
        let ping_pong = PingPong {
            bound,
            direction: PingPongDirection::Right,
            ..Default::default()
        };
        let mut player = make_player_with_state(Player::DoubleJumping(DoubleJumping::new(
            Moving::new(cur_pos, Point::new(40, 79), false, None),
            false,
            false,
        )));
        player
            .context
            .set_normal_action(None, PlayerAction::PingPong(ping_pong));
        let mut keys = MockInput::new();
        keys.expect_send_key_up();
        let resources = Resources::new(Some(keys), None);

        update_from_ping_pong_action(&resources, &mut player, ping_pong, cur_pos, true);

        assert_matches!(player.state, Player::UpJumping(_) | Player::Grappling(_));
    }

    #[test]
    fn update_from_ping_pong_action_transition_to_falling() {
        let cur_pos = Point::new(30, 101);
        let bound = Rect::new(20, 80, 40, 20);
        let ping_pong = PingPong {
            bound,
            direction: PingPongDirection::Right,
            ..Default::default()
        };

        let mut player = make_player_with_state(Player::DoubleJumping(DoubleJumping::new(
            Moving::new(cur_pos, Point::new(40, 101), false, None),
            false,
            false,
        )));
        player
            .context
            .set_normal_action(None, PlayerAction::PingPong(ping_pong));
        let mut keys = MockInput::new();
        keys.expect_send_key_up();
        let resources = Resources::new(Some(keys), None);

        update_from_ping_pong_action(&resources, &mut player, ping_pong, cur_pos, true);

        assert_matches!(player.state, Player::Falling { .. });
    }
}
