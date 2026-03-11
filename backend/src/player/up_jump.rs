use super::{
    Key, Player, PlayerContext,
    actions::update_from_ping_pong_action,
    moving::Moving,
    timeout::{MovingLifecycle, next_moving_lifecycle_with_axis},
    use_key::UseKey,
};
use crate::{
    ActionKeyWith,
    bridge::{InputKeyDownOptions, KeyKind},
    ecs::Resources,
    minimap::Minimap,
    player::{
        MOVE_TIMEOUT, PlayerAction, PlayerEntity, actions::update_from_auto_mob_action,
        next_action, state::LastMovement, timeout::ChangeAxis,
    },
};

/// Number of ticks to wait before spamming jump key.
const SPAM_DELAY: u32 = 7;

/// Number of ticks to wait before spamming jump key for lesser travel distance.
const SOFT_SPAM_DELAY: u32 = 12;

const TIMEOUT: u32 = MOVE_TIMEOUT + 3;

/// Player's `y` velocity to be considered as up jumped.
const UP_JUMPED_Y_VELOCITY_THRESHOLD: f32 = 1.3;

/// Player's `x` velocity to be considered as near stationary.
const X_NEAR_STATIONARY_THRESHOLD: f32 = 0.28;

/// Player's `y` velocity to be considered as near stationary.
const Y_NEAR_STATIONARY_VELOCITY_THRESHOLD: f32 = 0.4;

/// Minimum distance required to perform an up jump using teleport key with jump.
const TELEPORT_WITH_JUMP_THRESHOLD: i32 = 19;

/// Minimum distance required to perform an up jump using teleport key with jump when teleport
/// increase buff is enabled.
const EXTENDED_TELEPORT_WITH_JUMP_THRESHOLD: i32 = 20;

/// Minimum distance required to perform an up jump and then teleport.
const UP_JUMP_AND_TELEPORT_THRESHOLD: i32 = 23;

const SOFT_UP_JUMP_THRESHOLD: i32 = 16;

#[derive(Debug, Clone, Copy)]
struct Mage {
    state: MageState,
    teleport_with_jump_threshold: i32,
}

#[derive(Debug, Clone, Copy)]
enum MageState {
    Teleporting,
    UpJumping,
    Flying,
}

#[derive(Debug, Clone, Copy)]
enum UpJumpingKind {
    Mage(Mage),
    UpArrow,
    JumpKey,
    SpecificKey,
}

#[derive(Debug, Clone, Copy)]
pub struct UpJumping {
    pub moving: Moving,
    /// The kind of up jump.
    kind: UpJumpingKind,
    /// Number of ticks to wait before sending jump key(s).
    spam_delay: u32,
    /// Whether auto-mobbing should wait for up jump completion in non-intermediate destination.
    auto_mob_wait_completion: bool,
}

impl UpJumping {
    pub fn new(moving: Moving, resources: &Resources, player_context: &PlayerContext) -> Self {
        let (y_distance, _) = moving.y_distance_direction_from(true, moving.pos);
        let spam_delay = if !player_context.config.up_jump_specific_key_should_jump
            && y_distance <= SOFT_UP_JUMP_THRESHOLD
        {
            SOFT_SPAM_DELAY
        } else {
            SPAM_DELAY
        };
        let auto_mob_wait_completion =
            player_context.has_auto_mob_action_only() && resources.rng.random_bool(0.5);
        let kind = up_jumping_kind(
            player_context.config.up_jump_key,
            player_context.config.teleport_key.is_some(),
            player_context.config.has_extended_teleport_range,
        );

        Self {
            moving,
            kind,
            spam_delay,
            auto_mob_wait_completion,
        }
    }

    #[inline]
    fn moving(mut self, moving: Moving) -> UpJumping {
        self.moving = moving;
        self
    }
}

/// Updates the [`Player::UpJumping`] contextual state.
///
/// This state can only be transitioned via [`Player::Moving`] when the
/// player has reached the destination x-wise. Before performing an up jump, it will check for
/// stationary state and whether the player is currently near a portal. If the player is near
/// a portal, this action is aborted. The up jump action is made to be adapted for various classes
/// that has different up jump key combination.
pub fn update_up_jumping_state(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
) {
    let Player::UpJumping(mut up_jumping) = player.state else {
        panic!("state is not up jumping");
    };
    let up_jump_key = player.context.config.up_jump_key;
    let jump_key = player.context.config.jump_key;
    let should_jump = player.context.config.up_jump_specific_key_should_jump;
    let is_flight = player.context.config.up_jump_is_flight;

    match next_moving_lifecycle_with_axis(
        up_jumping.moving,
        player
            .context
            .last_known_pos
            .expect("in positional context"),
        TIMEOUT,
        ChangeAxis::Vertical,
    ) {
        MovingLifecycle::Started(moving) => {
            // Stall until near stationary
            let (x_velocity, y_velocity) = player.context.velocity;
            if x_velocity > X_NEAR_STATIONARY_THRESHOLD
                || y_velocity > Y_NEAR_STATIONARY_VELOCITY_THRESHOLD
            {
                let moving = moving.timeout_started(false);
                let up_jumping = up_jumping.moving(moving);

                player.state = Player::UpJumping(up_jumping);
                return;
            }

            let is_inside_portal = match minimap_state {
                Minimap::Idle(idle) => idle.is_position_inside_portal(moving.pos),
                _ => false,
            };
            if is_inside_portal {
                player.state = Player::Idle;
                player.context.clear_action_completed();
                return;
            }

            player.context.last_movement = Some(LastMovement::UpJumping);
            player.state = Player::UpJumping(up_jumping.moving(moving));
            match &mut up_jumping.kind {
                UpJumpingKind::Mage(mage) => {
                    let (y_distance, _) = moving.y_distance_direction_from(true, moving.pos);
                    let teleport_after_up_jump =
                        !is_flight && y_distance >= UP_JUMP_AND_TELEPORT_THRESHOLD;
                    mage.state = if is_flight {
                        MageState::Flying
                    } else if teleport_after_up_jump {
                        MageState::UpJumping
                    } else {
                        MageState::Teleporting
                    };

                    resources.input.send_key_down(KeyKind::Up);
                    let can_jump =
                        y_distance >= mage.teleport_with_jump_threshold && up_jump_key.is_none();
                    if is_flight || can_jump {
                        resources.input.send_key(jump_key);
                    }
                }
                UpJumpingKind::UpArrow => {
                    resources.input.send_key(jump_key);
                }
                UpJumpingKind::JumpKey => {
                    resources.input.send_key_down(KeyKind::Up);
                    resources.input.send_key(jump_key);
                }
                UpJumpingKind::SpecificKey => {
                    resources.input.send_key_down(KeyKind::Up);
                    if is_flight || should_jump {
                        resources.input.send_key(jump_key);
                    }
                }
            }
        }
        MovingLifecycle::Ended(moving) => {
            player.state = Player::Moving(moving.dest, moving.exact, moving.intermediates);
            resources.input.send_key_up(KeyKind::Up);
        }
        MovingLifecycle::Updated(mut moving) => {
            let (y_distance, y_direction) = moving.y_distance_direction_from(true, moving.pos);
            update_up_jump(
                resources,
                &player.context,
                &mut moving,
                &mut up_jumping,
                y_distance,
                y_direction,
            );

            // Sets initial next state first
            player.state = Player::UpJumping(up_jumping.moving(moving));
            update_from_action(
                resources,
                player,
                minimap_state,
                up_jumping,
                moving,
                y_direction,
            );
        }
    }
}

fn update_from_action(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
    up_jumping: UpJumping,
    moving: Moving,
    y_direction: i32,
) {
    let cur_pos = moving.pos;

    match next_action(&player.context) {
        Some(PlayerAction::AutoMob(mob)) => {
            if moving.completed && moving.is_destination_intermediate() && y_direction <= 0 {
                resources.input.send_key_up(KeyKind::Up);
                player.state = Player::Moving(moving.dest, moving.exact, moving.intermediates);
                return;
            }

            if up_jumping.auto_mob_wait_completion && !moving.completed {
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
            if moving.completed && y_direction <= 0 {
                player.state = Player::UseKey(UseKey::from_key(key));
            }
        }

        Some(PlayerAction::PingPong(ping_pong)) => {
            if !moving.completed
                || !resources
                    .rng
                    .random_perlin_bool(cur_pos.x, cur_pos.y, resources.tick, 0.7)
            {
                return;
            }

            update_from_ping_pong_action(resources, player, minimap_state, ping_pong, cur_pos);
        }

        Some(
            PlayerAction::Key(Key {
                with: ActionKeyWith::Stationary | ActionKeyWith::DoubleJump,
                ..
            })
            | PlayerAction::Move(_)
            | PlayerAction::SolveRune,
        )
        | None => (),
        _ => unreachable!(),
    }
}

fn update_up_jump(
    resources: &Resources,
    context: &PlayerContext,
    moving: &mut Moving,
    up_jumping: &mut UpJumping,
    y_distance: i32,
    y_direction: i32,
) {
    let jump_key = context.config.jump_key;
    let up_jump_key = context.config.up_jump_key;
    let should_jump = context.config.up_jump_specific_key_should_jump;
    let is_flight = context.config.up_jump_is_flight;

    if moving.completed {
        resources.input.send_key_up(KeyKind::Up);
        return;
    }

    match &mut up_jumping.kind {
        UpJumpingKind::Mage(mage) => {
            update_mage_up_jump(
                resources,
                context,
                moving,
                mage,
                up_jumping.spam_delay,
                y_distance,
                y_direction,
            );
        }
        UpJumpingKind::UpArrow | UpJumpingKind::JumpKey => {
            if context.velocity.1 <= UP_JUMPED_Y_VELOCITY_THRESHOLD {
                // Spam jump/up arrow key until the player y changes
                // above a threshold as sending jump key twice
                // doesn't work.
                if moving.timeout.total >= up_jumping.spam_delay {
                    if matches!(up_jumping.kind, UpJumpingKind::UpArrow) {
                        resources.input.send_key(KeyKind::Up);
                    } else {
                        resources.input.send_key(jump_key);
                    }
                }
            } else {
                moving.completed = true;
            }
        }
        UpJumpingKind::SpecificKey => {
            if !is_flight {
                if !should_jump || moving.timeout.total >= up_jumping.spam_delay {
                    resources
                        .input
                        .send_key(up_jump_key.expect("has up jump key"));
                    moving.completed = true;
                }
            } else {
                update_flying(
                    resources,
                    moving,
                    y_direction,
                    up_jump_key.expect("has up jump key"),
                );
            }
        }
    }
}

fn update_mage_up_jump(
    resources: &Resources,
    context: &PlayerContext,
    moving: &mut Moving,
    mage: &mut Mage,
    spam_delay: u32,
    y_distance: i32,
    y_direction: i32,
) {
    let jump_key = context.config.jump_key;
    let up_jump_key = context.config.up_jump_key;
    let teleport_key = context.config.teleport_key.expect("has teleport key");

    match mage.state {
        MageState::Teleporting => {
            if y_direction > 0 && y_distance < mage.teleport_with_jump_threshold {
                resources.input.send_key(teleport_key);
                moving.completed = true;
            }
        }
        MageState::UpJumping => match up_jump_key {
            Some(key) => {
                resources.input.send_key(key);
                mage.state = MageState::Teleporting;
            }
            None => {
                if context.velocity.1 <= UP_JUMPED_Y_VELOCITY_THRESHOLD {
                    if moving.timeout.total >= spam_delay {
                        resources.input.send_key(jump_key);
                    }
                } else {
                    mage.state = MageState::Teleporting;
                }
            }
        },
        MageState::Flying => update_flying(
            resources,
            moving,
            y_direction,
            up_jump_key.unwrap_or(teleport_key),
        ),
    }
}

#[inline]
fn update_flying(resources: &Resources, moving: &mut Moving, y_direction: i32, key: KeyKind) {
    if y_direction > 0 {
        resources
            .input
            .send_key_down_with_options(key, InputKeyDownOptions::default().repeatable());
    } else {
        resources.input.send_key_up(key);
        moving.completed = true;
    }
}

#[inline]
fn up_jumping_kind(
    up_jump_key: Option<KeyKind>,
    has_teleport_key: bool,
    has_extended_teleport_range: bool,
) -> UpJumpingKind {
    match (up_jump_key, has_teleport_key) {
        (Some(_), true) | (None, true) => UpJumpingKind::Mage(Mage {
            state: MageState::Teleporting, // Overwrite later
            teleport_with_jump_threshold: if has_extended_teleport_range {
                EXTENDED_TELEPORT_WITH_JUMP_THRESHOLD
            } else {
                TELEPORT_WITH_JUMP_THRESHOLD
            },
        }),
        (Some(KeyKind::Up), false) => UpJumpingKind::UpArrow,
        (None, false) => UpJumpingKind::JumpKey,
        (Some(_), false) => UpJumpingKind::SpecificKey,
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use opencv::core::Point;

    use super::*;
    use crate::bridge::{KeyKind, MockInput};
    use crate::ecs::Resources;
    use crate::player::{Player, PlayerEntity};

    fn setup_player(up_jumping: UpJumping) -> PlayerEntity {
        let mut player = PlayerEntity {
            state: Player::UpJumping(up_jumping),
            context: PlayerContext::default(),
        };
        player.context.last_known_pos = Some(Point::new(0, 0));
        player.context.config.jump_key = KeyKind::Space;
        player
    }

    #[test]
    fn update_up_jumping_state_started_jump_key_presses_up_and_jump() {
        let moving = Moving::new(Point::new(0, 0), Point::new(0, 20), true, None);
        let mut player = setup_player(UpJumping {
            moving,
            kind: UpJumpingKind::JumpKey,
            spam_delay: SPAM_DELAY,
            auto_mob_wait_completion: false,
        });
        let mut keys = MockInput::new();
        keys.expect_send_key_down()
            .withf(|k| *k == KeyKind::Up)
            .once();
        keys.expect_send_key()
            .withf(|k| *k == KeyKind::Space)
            .once();
        let resources = Resources::new(Some(keys), None);

        update_up_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::UpJumping(_));
    }

    #[test]
    fn update_up_jumping_state_started_up_arrow_presses_jump_only() {
        let moving = Moving::new(Point::new(0, 0), Point::new(0, 20), true, None);
        let mut player = setup_player(UpJumping {
            moving,
            kind: UpJumpingKind::UpArrow,
            spam_delay: SPAM_DELAY,
            auto_mob_wait_completion: false,
        });
        let mut keys = MockInput::new();
        keys.expect_send_key()
            .withf(|k| *k == KeyKind::Space)
            .once();
        let resources = Resources::new(Some(keys), None);

        update_up_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::UpJumping(_));
    }

    #[test]
    fn update_up_jumping_state_started_specific_key_presses_up_only() {
        let moving = Moving::new(Point::new(0, 0), Point::new(0, 20), true, None);
        let mut player = setup_player(UpJumping {
            moving,
            kind: UpJumpingKind::SpecificKey,
            spam_delay: SPAM_DELAY,
            auto_mob_wait_completion: false,
        });
        player.context.config.up_jump_key = Some(KeyKind::C);
        let mut keys = MockInput::new();
        keys.expect_send_key_down()
            .withf(|k| *k == KeyKind::Up)
            .once();
        let resources = Resources::new(Some(keys), None);

        update_up_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::UpJumping(_));
    }

    #[test]
    fn update_up_jumping_state_started_mage_up_and_jump() {
        let moving = Moving::new(Point::new(0, 0), Point::new(0, 25), true, None);
        let mut player = setup_player(UpJumping {
            moving,
            kind: UpJumpingKind::Mage(Mage {
                state: MageState::Teleporting,
                teleport_with_jump_threshold: TELEPORT_WITH_JUMP_THRESHOLD,
            }),
            spam_delay: SPAM_DELAY,
            auto_mob_wait_completion: false,
        });
        player.context.config.teleport_key = Some(KeyKind::Shift);
        let mut keys = MockInput::new();
        keys.expect_send_key_down()
            .withf(|k| *k == KeyKind::Up)
            .once();
        keys.expect_send_key()
            .withf(|k| *k == KeyKind::Space)
            .once();
        let resources = Resources::new(Some(keys), None);

        update_up_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::UpJumping(_));
    }

    #[test]
    fn update_up_jumping_state_updated_velocity_marks_completed() {
        let mut moving = Moving::new(Point::new(0, 0), Point::new(0, 20), true, None);
        moving.timeout.started = true;
        let mut player = setup_player(UpJumping {
            moving,
            kind: UpJumpingKind::JumpKey,
            spam_delay: SPAM_DELAY,
            auto_mob_wait_completion: false,
        });
        player.context.velocity = (0.0, 2.0); // Y velocity above threshold
        let resources = Resources::new(None, None);

        update_up_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(
            player.state,
            Player::UpJumping(UpJumping {
                moving: Moving {
                    completed: true,
                    ..
                },
                ..
            })
        );
    }

    #[test]
    fn update_up_jumping_state_updated_before_spam_delay_no_keys_sent() {
        let mut moving = Moving::new(Point::new(0, 0), Point::new(0, 20), true, None);
        moving.timeout.started = true;
        moving.timeout.total = SPAM_DELAY - 2; // before threshold
        let mut player = setup_player(UpJumping {
            moving,
            kind: UpJumpingKind::JumpKey,
            spam_delay: SPAM_DELAY,
            auto_mob_wait_completion: false,
        });
        let mut keys = MockInput::new();
        keys.expect_send_key().never();
        keys.expect_send_key_down().never();
        keys.expect_send_key_up().never();
        let resources = Resources::new(Some(keys), None);

        update_up_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::UpJumping(_));
    }

    #[test]
    fn update_up_jumping_state_updated_spam_jump_key_after_delay() {
        let mut moving = Moving::new(Point::new(0, 0), Point::new(0, 20), true, None);
        moving.timeout.started = true;
        moving.timeout.total = SPAM_DELAY; // exactly at threshold
        let mut player = setup_player(UpJumping {
            moving,
            kind: UpJumpingKind::JumpKey,
            spam_delay: SPAM_DELAY,
            auto_mob_wait_completion: false,
        });
        let mut keys = MockInput::new();
        // On spam, JumpKey kind sends Jump again
        keys.expect_send_key()
            .withf(|k| *k == KeyKind::Space)
            .once();
        let resources = Resources::new(Some(keys), None);

        update_up_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::UpJumping(_));
    }

    #[test]
    fn update_up_jumping_state_updated_spam_specific_key_after_delay() {
        let mut moving = Moving::new(Point::new(0, 0), Point::new(0, 20), true, None);
        moving.timeout.started = true;
        moving.timeout.total = SPAM_DELAY;
        let mut player = setup_player(UpJumping {
            moving,
            kind: UpJumpingKind::SpecificKey,
            spam_delay: SPAM_DELAY,
            auto_mob_wait_completion: false,
        });
        player.context.config.up_jump_key = Some(KeyKind::C);
        let mut keys = MockInput::new();
        keys.expect_send_key().withf(|k| *k == KeyKind::C).once();
        let resources = Resources::new(Some(keys), None);

        update_up_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::UpJumping(_));
    }

    #[test]
    fn update_up_jumping_state_updated_mage_spam_jump_after_delay() {
        let mut moving = Moving::new(Point::new(0, 0), Point::new(0, 25), true, None);
        moving.timeout.started = true;
        moving.timeout.total = SPAM_DELAY;
        let mut player = setup_player(UpJumping {
            moving,
            kind: UpJumpingKind::Mage(Mage {
                state: MageState::UpJumping,
                teleport_with_jump_threshold: TELEPORT_WITH_JUMP_THRESHOLD,
            }),
            spam_delay: SPAM_DELAY,
            auto_mob_wait_completion: false,
        });
        player.context.config.jump_key = KeyKind::Space;
        player.context.config.teleport_key = Some(KeyKind::Shift);
        let mut keys = MockInput::new();
        keys.expect_send_key()
            .withf(|k| *k == KeyKind::Space)
            .once();
        let resources = Resources::new(Some(keys), None);

        update_up_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::UpJumping(_));
    }

    #[test]
    fn update_up_jumping_state_updated_completed_and_releases_up() {
        let mut moving = Moving::new(Point::new(0, 0), Point::new(0, 20), true, None);
        moving.completed = true;
        moving.timeout.started = true;
        let mut player = setup_player(UpJumping {
            moving,
            kind: UpJumpingKind::JumpKey,
            spam_delay: SPAM_DELAY,
            auto_mob_wait_completion: false,
        });
        let mut keys = MockInput::new();
        keys.expect_send_key_up()
            .withf(|k| *k == KeyKind::Up)
            .once();
        let resources = Resources::new(Some(keys), None);

        update_up_jumping_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::UpJumping(_));
    }
}
