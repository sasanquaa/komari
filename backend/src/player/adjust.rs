use std::cmp::Ordering;

use super::{
    Key, PlayerAction,
    moving::Moving,
    timeout::{Lifecycle, next_timeout_lifecycle},
    use_key::UseKey,
};
use crate::{
    ActionKeyDirection, ActionKeyWith,
    bridge::KeyKind,
    ecs::Resources,
    minimap::Minimap,
    player::{
        Player, PlayerEntity,
        actions::update_from_auto_mob_action,
        double_jump::DoubleJumping,
        moving::MOVE_TIMEOUT,
        next_action,
        state::LastMovement,
        timeout::{ChangeAxis, MovingLifecycle, Timeout, next_moving_lifecycle_with_axis},
    },
};

/// Minimum x distance from the destination required to perform small movement.
pub const ADJUSTING_SHORT_THRESHOLD: i32 = 1;

/// Minimum x distance from the destination required to walk.
pub const ADJUSTING_MEDIUM_THRESHOLD: i32 = 3;

const ADJUSTING_SHORT_TIMEOUT: u32 = MOVE_TIMEOUT + 3;

#[derive(Clone, Copy, Debug)]
pub struct Adjusting {
    pub moving: Moving,
    adjust_timeout: Timeout,
}

impl Adjusting {
    pub fn new(moving: Moving) -> Self {
        Self {
            moving,
            adjust_timeout: Timeout::default(),
        }
    }

    fn moving(self, moving: Moving) -> Adjusting {
        Adjusting { moving, ..self }
    }

    fn update_adjusting(&mut self, resources: &Resources, keys: Option<(KeyKind, KeyKind)>) {
        self.adjust_timeout =
            match next_timeout_lifecycle(self.adjust_timeout, ADJUSTING_SHORT_TIMEOUT) {
                Lifecycle::Started(timeout) => {
                    if let Some((up_key, down_key)) = keys {
                        resources.input.send_key_up(up_key);
                        resources.input.send_key(down_key);
                    }
                    timeout
                }
                Lifecycle::Ended => Timeout::default(),
                Lifecycle::Updated(timeout) => timeout,
            };
    }
}

/// Updates the [`Player::Adjusting`] contextual state.
///
/// This state just walks towards the destination. If [`Moving::exact`] is true,
/// then it will perform small movement to ensure the `x` is as close as possible.
pub fn update_adjusting_state(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
) {
    let Player::Adjusting(adjusting) = player.state else {
        panic!("state is not adjusting")
    };
    let context = &mut player.context;
    let cur_pos = context.last_known_pos.expect("in positional state");

    let moving = adjusting.moving;
    let is_intermediate = moving.is_destination_intermediate();

    match next_moving_lifecycle_with_axis(moving, cur_pos, MOVE_TIMEOUT, ChangeAxis::Both) {
        MovingLifecycle::Started(moving) => {
            context.last_movement = Some(LastMovement::Adjusting);
            player.state = Player::Adjusting(adjusting.moving(moving));
        }
        MovingLifecycle::Ended(moving) => {
            resources.input.send_key_up(KeyKind::Right);
            resources.input.send_key_up(KeyKind::Left);
            player.state = Player::Moving(moving.dest, moving.exact, moving.intermediates);
        }
        MovingLifecycle::Updated(mut moving) => {
            let mut adjusting = adjusting;
            let threshold = context.double_jump_threshold(is_intermediate);
            let (x_distance, x_direction) = moving.x_distance_direction_from(true, moving.pos);

            if !context.config.disable_double_jumping && x_distance >= threshold {
                player.state = Player::Moving(moving.dest, moving.exact, moving.intermediates);
                return;
            }

            // Movement logics
            if !moving.completed {
                let adjusting_started = adjusting.adjust_timeout.started;
                if adjusting_started {
                    // Do not allow timing out if adjusting is in-progress
                    moving.timeout.current = moving.timeout.current.saturating_sub(1);
                }

                let should_adjust_medium =
                    !adjusting_started && x_distance >= ADJUSTING_MEDIUM_THRESHOLD;
                let should_adjust_short =
                    adjusting_started || (moving.exact && x_distance >= ADJUSTING_SHORT_THRESHOLD);
                let direction = match x_direction.cmp(&0) {
                    Ordering::Greater => {
                        Some((KeyKind::Right, KeyKind::Left, ActionKeyDirection::Right))
                    }
                    Ordering::Less => {
                        Some((KeyKind::Left, KeyKind::Right, ActionKeyDirection::Left))
                    }
                    _ => None,
                };

                match (should_adjust_medium, should_adjust_short, direction) {
                    (true, _, Some((down_key, up_key, dir))) => {
                        resources.input.send_key_up(up_key);
                        resources.input.send_key_down(down_key);
                        context.last_known_direction = dir;
                    }
                    (false, true, Some((down_key, up_key, dir))) => {
                        adjusting.update_adjusting(resources, Some((up_key, down_key)));
                        context.last_known_direction = dir;
                    }
                    _ => {
                        if adjusting_started {
                            adjusting.update_adjusting(resources, None);
                        } else {
                            resources.input.send_key_up(KeyKind::Left);
                            resources.input.send_key_up(KeyKind::Right);
                            moving = moving.completed(true);
                        }
                    }
                }
            }

            // Computes and sets initial next state first
            let next_moving = if !moving.completed {
                moving
            } else if moving.exact && x_distance >= ADJUSTING_SHORT_THRESHOLD {
                // Exact adjusting incomplete
                moving.completed(false).timeout_current(0)
            } else {
                moving.timeout_current(MOVE_TIMEOUT)
            };
            player.state = Player::Adjusting(adjusting.moving(next_moving));

            update_from_action(resources, player, minimap_state, moving);
        }
    }
}

fn update_from_action(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
    moving: Moving,
) {
    const USE_KEY_Y_THRESHOLD: i32 = 2;

    let cur_pos = moving.pos;
    let context = &player.context;
    let (x_distance, x_direction) = moving.x_distance_direction_from(false, cur_pos);
    let (y_distance, _) = moving.y_distance_direction_from(false, cur_pos);

    match next_action(context) {
        Some(PlayerAction::Key(
            key @ Key {
                with: ActionKeyWith::DoubleJump,
                direction,
                ..
            },
        )) => {
            if !moving.completed || y_distance > 0 {
                return;
            }

            player.state = if matches!(direction, ActionKeyDirection::Any)
                || direction == context.last_known_direction
            {
                Player::DoubleJumping(DoubleJumping::new(
                    moving.timeout(Timeout::default()).completed(false),
                    true,
                    false,
                ))
            } else {
                Player::UseKey(UseKey::from_key(key))
            };
        }
        Some(PlayerAction::Key(
            key @ Key {
                with: ActionKeyWith::Any,
                ..
            },
        )) => {
            if moving.completed && y_distance <= USE_KEY_Y_THRESHOLD {
                player.state = Player::UseKey(UseKey::from_key(key));
            }
        }
        Some(PlayerAction::AutoMob(mob)) => update_from_auto_mob_action(
            resources,
            player,
            minimap_state,
            mob,
            x_distance,
            x_direction,
            y_distance,
        ),
        None
        | Some(
            PlayerAction::Key(Key {
                with: ActionKeyWith::Stationary,
                ..
            })
            | PlayerAction::SolveRune
            | PlayerAction::Move(_),
        ) => (),
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
        bridge::MockInput,
        player::{Player, PlayerContext},
    };

    fn mock_player_entity(pos: Point) -> PlayerEntity {
        let mut context = PlayerContext::default();
        context.last_known_pos = Some(pos);

        PlayerEntity {
            state: Player::Idle,
            context,
        }
    }

    #[test]
    fn update_adjusting_state_started() {
        let pos = Point { x: 0, y: 0 };
        let dest = Point { x: 10, y: 0 };
        let mut player = mock_player_entity(pos);
        player.context.is_stationary = true;
        player.state = Player::Adjusting(Adjusting::new(Moving::new(pos, dest, false, None)));

        let resources = Resources::new(None, None);

        update_adjusting_state(&resources, &mut player, Minimap::Detecting);

        // Should remain in Adjusting state (started branch moves it into Adjusting with moving started)
        assert_matches!(player.state, Player::Adjusting(_));
        assert_matches!(player.context.last_movement, Some(LastMovement::Adjusting));
    }

    #[test]
    fn update_adjusting_state_updated_performs_medium_adjustment_right() {
        let mut keys = MockInput::default();
        // Expect right to be pressed down and left to be released
        keys.expect_send_key_up().with(eq(KeyKind::Left)).once();
        keys.expect_send_key_down().with(eq(KeyKind::Right)).once();

        let resources = Resources::new(Some(keys), None);

        let pos = Point { x: 0, y: 0 };
        let dest = Point { x: 5, y: 0 }; // x_distance = 5 (>= medium threshold = 3)
        let mut player = mock_player_entity(pos);
        player.state = Player::Adjusting(Adjusting::new(
            Moving::new(pos, dest, false, None).timeout_started(true),
        ));

        update_adjusting_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::Adjusting(_));
        assert_eq!(
            player.context.last_known_direction,
            ActionKeyDirection::Right
        );
    }

    #[test]
    fn update_adjusting_state_updated_performs_medium_adjustment_left() {
        let mut keys = MockInput::default();
        keys.expect_send_key_up().with(eq(KeyKind::Right)).once();
        keys.expect_send_key_down().with(eq(KeyKind::Left)).once();

        let resources = Resources::new(Some(keys), None);

        let pos = Point { x: 10, y: 0 };
        let dest = Point { x: 0, y: 0 }; // x_distance = 10
        let mut player = mock_player_entity(pos);
        player.state = Player::Adjusting(Adjusting::new(
            Moving::new(pos, dest, false, None).timeout_started(true),
        ));

        update_adjusting_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::Adjusting(_));
        assert_eq!(
            player.context.last_known_direction,
            ActionKeyDirection::Left
        );
    }

    #[test]
    fn update_adjusting_state_updated_completes_when_no_direction_and_no_adjustment() {
        let mut keys = MockInput::default();
        keys.expect_send_key_up().with(eq(KeyKind::Left)).once();
        keys.expect_send_key_up().with(eq(KeyKind::Right)).once();

        let resources = Resources::new(Some(keys), None);

        let pos = Point { x: 5, y: 0 };
        let dest = Point { x: 5, y: 0 }; // same position, no direction
        let mut player = mock_player_entity(pos);
        player.state = Player::Adjusting(Adjusting::new(
            Moving::new(pos, dest, false, None).timeout_started(true),
        ));

        update_adjusting_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(
            player.state,
            Player::Adjusting(Adjusting {
                moving: Moving {
                    completed: true,
                    ..
                },
                ..
            })
        );
    }

    #[test]
    fn update_adjusting_state_updated_short_adjustment_started() {
        let mut keys = MockInput::default();
        keys.expect_send_key_up().with(eq(KeyKind::Left)).once();
        keys.expect_send_key().with(eq(KeyKind::Right)).once();

        let resources = Resources::new(Some(keys), None);

        let pos = Point { x: 0, y: 0 };
        let dest = Point { x: 1, y: 0 }; // exact = true, x_distance = 1
        let mut player = mock_player_entity(pos);
        player.state = Player::Adjusting(Adjusting::new(
            Moving::new(pos, dest, true, None).timeout_started(true),
        ));

        update_adjusting_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(
            player.state,
            Player::Adjusting(Adjusting {
                adjust_timeout: Timeout { started: true, .. },
                ..
            })
        );
        assert_eq!(
            player.context.last_known_direction,
            ActionKeyDirection::Right
        );
    }

    #[test]
    fn update_adjusting_state_updated_timeout_freezes_when_adjusting_started() {
        let resources = Resources::new(None, None);
        let pos = Point { x: 0, y: 0 };
        let dest = Point { x: 1, y: 0 };
        let mut player = mock_player_entity(pos);

        let moving = Moving::new(pos, dest, true, None)
            .timeout_current(3)
            .timeout_started(true);
        let mut adjusting = Adjusting::new(moving);
        adjusting.adjust_timeout = Timeout {
            current: 1,
            started: true,
            ..Default::default()
        };
        player.state = Player::Adjusting(adjusting);

        update_adjusting_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(
            player.state,
            Player::Adjusting(Adjusting {
                moving: Moving {
                    timeout: Timeout { current: 3, .. },
                    ..
                },
                adjust_timeout: Timeout { current: 2, .. }
            })
        );
    }

    #[test]
    fn update_adjusting_state_updated_complted_exact_not_close_enough_keeps_adjusting() {
        let resources = Resources::new(None, None);
        let pos = Point { x: 0, y: 0 };
        let dest = Point { x: 1, y: 0 };
        let mut player = mock_player_entity(pos);

        let moving = Moving::new(pos, dest, true, None)
            .completed(true)
            .timeout_current(4)
            .timeout_started(true);
        player.state = Player::Adjusting(Adjusting::new(moving));

        update_adjusting_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(
            player.state,
            Player::Adjusting(Adjusting {
                moving: Moving {
                    completed: false,
                    timeout: Timeout {
                        current: 0,
                        started: true,
                        ..
                    },
                    ..
                },
                ..
            })
        );
    }

    // TODO: add tests for on_action
}
