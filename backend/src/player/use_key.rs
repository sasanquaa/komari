use std::assert_matches::assert_matches;

use super::{
    AutoMob, PingPongDirection, PlayerContext, Timeout,
    actions::{Key, PingPong, PlayerAction, update_from_ping_pong_action},
    double_jump::DoubleJumping,
    timeout::{Lifecycle, next_timeout_lifecycle},
};
use crate::{
    ActionKeyDirection, ActionKeyWith, Position, WaitAfterBuffered,
    bridge::{InputKeyDownOptions, KeyKind, LinkKeyKind},
    ecs::Resources,
    minimap::Minimap,
    player::{
        LastMovement, MOVE_TIMEOUT, Moving, Player, PlayerEntity, next_action,
        state::{BufferedStalling, BufferedStallingCallback},
    },
    run::MS_PER_TICK,
};

/// The total number of ticks for changing direction before timing out.
const CHANGE_DIRECTION_TIMEOUT: u32 = 3;

/// The tick to which the actual key will be pressed for [`LinkKeyKind::Along`].
const LINK_ALONG_PRESS_TICK: u32 = 2;

#[derive(Clone, Copy, Debug)]
enum ActionInfo {
    AutoMobbing { should_terminate: bool },
}

/// The different states of using key.
#[derive(Clone, Copy, Debug)]
enum State {
    /// Checks whether [`ActionKeyWith`] and [`ActionKeyDirection`] are satisfied and stalls
    /// for [`UseKey::wait_before_use_ticks`].
    Precondition,
    /// Changes direction to match [`ActionKeyDirection`].
    ///
    /// Returns to [`State::Precondition`] upon timeout.
    ChangingDirection(Timeout),
    /// Ensures player double jumped or is stationary.
    ///
    /// Returns to [`State::Precondition`] if player is stationary or
    /// transfers to [`Player::DoubleJumping`].
    EnsuringUseWith,
    /// Uses the actual key with optional [`LinkKeyKind`], [`UseKey::key_hold_ticks`] and stalls
    /// for [`UseKey::wait_after_use_ticks`].
    Using(Using),
    /// Ensures all [`UseKey::count`] times executed.
    Postcondition,
}

#[derive(Clone, Copy, Debug, Default)]
struct Using {
    link_timeout: Timeout,
    link_completed: bool,
    hold_timeout: Timeout,
    hold_completed: bool,
}

#[derive(Clone, Copy, Debug)]
enum PendingTransition {
    None,
    WaitBefore,
    WaitAfter,
    DoubleJump,
}

#[derive(Clone, Copy, Debug)]
pub struct UseKey {
    key: KeyKind,
    key_hold_ticks: u32,
    key_hold_buffered_to_wait_after: bool,
    link_key: LinkKeyKind,
    count: u32,
    current_count: u32,
    direction: ActionKeyDirection,
    with: ActionKeyWith,
    wait_before_use_ticks: u32,
    wait_after_use_ticks: u32,
    wait_after_buffered: WaitAfterBuffered,
    pending_transition: PendingTransition,
    action_info: Option<ActionInfo>,
    state: State,
}

impl UseKey {
    pub fn from_key(key: Key) -> Self {
        let Key {
            key,
            key_hold_ticks,
            key_hold_buffered_to_wait_after,
            link_key,
            count,
            direction,
            with,
            wait_before_use_ticks,
            wait_before_use_ticks_random_range,
            wait_after_use_ticks,
            wait_after_use_ticks_random_range,
            wait_after_buffered,
            ..
        } = key;
        let wait_before =
            random_wait_ticks(wait_before_use_ticks, wait_before_use_ticks_random_range);
        let wait_after = random_wait_ticks(wait_after_use_ticks, wait_after_use_ticks_random_range);

        Self {
            key,
            key_hold_ticks,
            key_hold_buffered_to_wait_after,
            link_key,
            count,
            current_count: 0,
            direction,
            with,
            wait_before_use_ticks: wait_before,
            wait_after_use_ticks: wait_after,
            wait_after_buffered,
            pending_transition: PendingTransition::None,
            action_info: None,
            state: State::Precondition,
        }
    }

    pub fn from_auto_mob(
        mob: AutoMob,
        direction: ActionKeyDirection,
        should_terminate: bool,
    ) -> Self {
        let wait_before =
            random_wait_ticks(mob.wait_before_ticks, mob.wait_before_ticks_random_range);
        let wait_after = random_wait_ticks(mob.wait_after_ticks, mob.wait_after_ticks_random_range);

        Self {
            key: mob.key,
            key_hold_ticks: mob.key_hold_ticks,
            key_hold_buffered_to_wait_after: false,
            link_key: mob.link_key,
            count: mob.count,
            current_count: 0,
            direction,
            with: mob.with,
            wait_before_use_ticks: wait_before,
            wait_after_use_ticks: wait_after,
            wait_after_buffered: WaitAfterBuffered::None,
            pending_transition: PendingTransition::None,
            action_info: Some(ActionInfo::AutoMobbing { should_terminate }),
            state: State::Precondition,
        }
    }

    pub fn from_ping_pong(ping_pong: PingPong) -> Self {
        let wait_before = random_wait_ticks(
            ping_pong.wait_before_ticks,
            ping_pong.wait_before_ticks_random_range,
        );
        let wait_after = random_wait_ticks(
            ping_pong.wait_after_ticks,
            ping_pong.wait_after_ticks_random_range,
        );
        let direction = if matches!(ping_pong.direction, PingPongDirection::Left) {
            ActionKeyDirection::Left
        } else {
            ActionKeyDirection::Right
        };

        Self {
            key: ping_pong.key,
            key_hold_ticks: ping_pong.key_hold_ticks,
            key_hold_buffered_to_wait_after: false,
            link_key: ping_pong.link_key,
            count: ping_pong.count,
            current_count: 0,
            direction,
            with: ping_pong.with,
            wait_before_use_ticks: wait_before,
            wait_after_use_ticks: wait_after,
            wait_after_buffered: WaitAfterBuffered::None,
            pending_transition: PendingTransition::None,
            action_info: None,
            state: State::Precondition,
        }
    }

    fn is_last_key_use(&self) -> bool {
        self.current_count >= self.count - 1
    }

    fn has_wait_after_buffer(&self) -> bool {
        !matches!(self.wait_after_buffered, WaitAfterBuffered::None)
    }

    fn should_buffer_key_holding(&self) -> bool {
        self.key_hold_ticks > 0
            && matches!(self.link_key, LinkKeyKind::None)
            && self.is_last_key_use()
            && self.has_wait_after_buffer()
            && self.key_hold_buffered_to_wait_after
    }
}

/// Updates the [`Player::UseKey`] contextual state.
///
/// Like [`Player::SolvingRune`], this state can only be transitioned via a [`PlayerAction`]. It
/// can be transitioned during any of the movement state. Or if there is no position, it will
/// be transitioned to immediately by [`Player::Idle`].
///
/// There are multiple stages to using a key as described by [`State`].
pub fn update_use_key_state(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
) {
    let Player::UseKey(mut use_key) = player.state else {
        panic!("state is not using key")
    };

    match use_key.state {
        State::Precondition => {
            update_precondition(resources, &mut player.context, &mut use_key);
            if matches!(use_key.pending_transition, PendingTransition::WaitBefore) {
                use_key.pending_transition = PendingTransition::None;
                use_key.state = State::Using(Using::default());
                player.context.stalling_timeout_state = Some(Player::UseKey(use_key));
                player.state = Player::Stalling(Timeout::default(), use_key.wait_before_use_ticks);
                return;
            }
        }
        State::ChangingDirection(_) => {
            update_changing_direction(resources, &mut player.context, &mut use_key);
        }
        #[allow(unused_assignments)]
        State::EnsuringUseWith => {
            update_ensuring_use_with(&player.context, &mut use_key);
            if matches!(use_key.pending_transition, PendingTransition::DoubleJump) {
                use_key.pending_transition = PendingTransition::None;
                player.state = Player::DoubleJumping(DoubleJumping::new(
                    Moving::new(
                        player.context.last_known_pos.expect("in positional state"),
                        player.context.last_known_pos.expect("in positional state"),
                        false,
                        None,
                    ),
                    true,
                    true,
                ));
                return;
            }
        }
        State::Using(_) => {
            update_using(resources, &player.context, &mut use_key);

            let has_transition = matches!(use_key.pending_transition, PendingTransition::WaitAfter);
            let should_buffer =
                has_transition && use_key.has_wait_after_buffer() && use_key.is_last_key_use();

            if should_buffer {
                let should_buffer_holding = use_key.should_buffer_key_holding();
                let buffer_ticks = if should_buffer_holding {
                    use_key.wait_after_use_ticks + use_key.key_hold_ticks
                } else {
                    use_key.wait_after_use_ticks
                };
                let update_callback = if should_buffer_holding {
                    Some(BufferedStallingCallback::new(
                        move |resources: &Resources| {
                            resources.input.send_key_down_with_options(
                                use_key.key,
                                InputKeyDownOptions::default().repeatable(),
                            );
                        },
                    ))
                } else {
                    None
                };
                let end_callback = if should_buffer_holding {
                    Some(BufferedStallingCallback::new(
                        move |resources: &Resources| {
                            resources.input.send_key_up(use_key.key);
                        },
                    ))
                } else {
                    None
                };
                let can_interrupt = matches!(
                    use_key.wait_after_buffered,
                    WaitAfterBuffered::Interruptible
                );

                use_key.current_count = use_key.count;
                player.context.stalling_buffered = if can_interrupt {
                    BufferedStalling::Interruptible(Timeout::default(), buffer_ticks)
                } else {
                    BufferedStalling::Uninterruptible(Timeout::default(), buffer_ticks)
                };
                player.context.stalling_timeout_buffered_update_callback = update_callback;
                player.context.stalling_timeout_buffered_end_callback = end_callback;
            } else if has_transition {
                use_key.pending_transition = PendingTransition::None;
                use_key.state = State::Postcondition;
                player.context.stalling_timeout_state = Some(Player::UseKey(use_key));
                player.state = Player::Stalling(Timeout::default(), use_key.wait_after_use_ticks);
                return;
            }
        }
        State::Postcondition => {
            use_key.current_count += 1;
            if use_key.current_count < use_key.count {
                use_key.state = State::Precondition;
            }
        }
    }

    let player_next_state = if use_key.current_count >= use_key.count {
        Player::Idle
    } else {
        Player::UseKey(use_key)
    };
    let is_terminal = matches!(player_next_state, Player::Idle);

    match next_action(&player.context) {
        Some(PlayerAction::AutoMob(AutoMob {
            position: Position { y, .. },
            ..
        })) => {
            assert!(!use_key.key_hold_buffered_to_wait_after);
            assert!(use_key.wait_after_buffered == WaitAfterBuffered::None);

            let should_terminate = matches!(
                use_key.action_info,
                Some(ActionInfo::AutoMobbing {
                    should_terminate: true
                })
            );
            if !is_terminal || !should_terminate {
                player.state = player_next_state;
                return;
            }

            player
                .context
                .auto_mob_track_ignore_xs(minimap_state, false);
            if player.context.auto_mob_reachable_y_require_update(y) {
                player.state = Player::Stalling(Timeout::default(), MOVE_TIMEOUT);
                return;
            }

            assert_matches!(player_next_state, Player::Idle);
            player.context.clear_action_completed();
            player.state = player_next_state;
        }

        Some(PlayerAction::PingPong(ping_pong)) => {
            assert!(!use_key.key_hold_buffered_to_wait_after);
            assert!(use_key.wait_after_buffered == WaitAfterBuffered::None);

            if !is_terminal {
                player.state = player_next_state;
                return;
            }

            player.context.clear_unstucking(true);
            update_from_ping_pong_action(
                resources,
                player,
                minimap_state,
                ping_pong,
                player.context.last_known_pos.expect("in positional state"),
            )
        }

        Some(action @ (PlayerAction::Move(_) | PlayerAction::Key(_))) => {
            if is_terminal {
                if !action.is_key_action_without_position() {
                    player.context.clear_unstucking(false);
                }
                player.context.clear_action_completed();
            }

            player.state = player_next_state;
        }

        None => player.state = player_next_state,

        _ => unreachable!(),
    }
}

fn update_precondition(resources: &Resources, context: &mut PlayerContext, use_key: &mut UseKey) {
    if context.stalling_buffered.stalling() {
        context.clear_stalling_buffer_states_if_possible(resources);
        return;
    }

    if !ensure_direction(context, use_key.direction) {
        use_key.state = State::ChangingDirection(Timeout::default());
        return;
    }

    if !ensure_use_with(context, use_key.with) {
        use_key.state = State::EnsuringUseWith;
        return;
    }

    if use_key.wait_before_use_ticks == 0 {
        use_key.state = State::Using(Using::default());
        return;
    }

    use_key.pending_transition = PendingTransition::WaitBefore;
}

#[inline]
fn ensure_direction(context: &PlayerContext, direction: ActionKeyDirection) -> bool {
    match direction {
        ActionKeyDirection::Any => true,
        ActionKeyDirection::Left | ActionKeyDirection::Right => {
            direction == context.last_known_direction
        }
    }
}

#[inline]
fn ensure_use_with(context: &PlayerContext, with: ActionKeyWith) -> bool {
    match with {
        ActionKeyWith::Any => true,
        ActionKeyWith::Stationary => context.is_stationary,
        ActionKeyWith::DoubleJump => {
            matches!(context.last_movement, Some(LastMovement::DoubleJumping))
        }
    }
}

fn update_using(resources: &Resources, context: &PlayerContext, use_key: &mut UseKey) {
    let State::Using(using) = use_key.state else {
        panic!("use key state is not using");
    };

    match use_key.link_key {
        LinkKeyKind::After(_) => {
            if !using.hold_completed {
                update_holding_key(resources, use_key);
                if use_key.key_hold_ticks > 0 {
                    return;
                }
            }

            if !using.link_completed {
                return update_linking_key(
                    resources,
                    use_key,
                    context.config.link_key_timing_millis,
                );
            }
        }
        LinkKeyKind::AtTheSame(key) => {
            resources.input.send_key(key);
            if !using.hold_completed {
                update_holding_key(resources, use_key);
                if use_key.key_hold_ticks > 0 {
                    return;
                }
            }
        }
        LinkKeyKind::Along(_) => {
            if !using.link_completed {
                return update_linking_key(
                    resources,
                    use_key,
                    context.config.link_key_timing_millis,
                );
            }
        }
        LinkKeyKind::Before(_) | LinkKeyKind::None => {
            if matches!(use_key.link_key, LinkKeyKind::Before(_)) && !using.link_completed {
                return update_linking_key(
                    resources,
                    use_key,
                    context.config.link_key_timing_millis,
                );
            }

            if !using.hold_completed {
                update_holding_key(resources, use_key);
                if use_key.key_hold_ticks > 0 {
                    return;
                }
            }
        }
    }

    if use_key.wait_after_use_ticks == 0 && !use_key.should_buffer_key_holding() {
        use_key.state = State::Postcondition;
        return;
    }

    use_key.pending_transition = PendingTransition::WaitAfter;
}

fn update_ensuring_use_with(context: &PlayerContext, use_key: &mut UseKey) {
    match use_key.with {
        ActionKeyWith::Any => unreachable!(),
        ActionKeyWith::Stationary => {
            use_key.state = if context.is_stationary {
                State::Precondition
            } else {
                State::EnsuringUseWith
            };
        }
        ActionKeyWith::DoubleJump => {
            use_key.pending_transition = PendingTransition::DoubleJump;
        }
    }
}

fn update_changing_direction(
    resources: &Resources,
    context: &mut PlayerContext,
    use_key: &mut UseKey,
) {
    let State::ChangingDirection(timeout) = use_key.state else {
        panic!("using key state is not changing direction");
    };
    let key = match use_key.direction {
        ActionKeyDirection::Left => KeyKind::Left,
        ActionKeyDirection::Right => KeyKind::Right,
        ActionKeyDirection::Any => unreachable!(),
    };

    match next_timeout_lifecycle(timeout, CHANGE_DIRECTION_TIMEOUT) {
        Lifecycle::Started(timeout) => {
            if !resources.input.is_key_cleared(key) {
                use_key.state = State::ChangingDirection(timeout.started(false));
                return;
            }
            resources.input.send_key(key);
            use_key.state = State::ChangingDirection(timeout);
        }
        Lifecycle::Ended => {
            context.last_known_direction = use_key.direction;
            use_key.state = State::Precondition;
        }
        Lifecycle::Updated(timeout) => {
            use_key.state = State::ChangingDirection(timeout);
        }
    }
}

#[inline]
fn update_holding_key(resources: &Resources, use_key: &mut UseKey) {
    let State::Using(using) = use_key.state else {
        panic!("use key state is not using");
    };

    if use_key.key_hold_ticks == 0 {
        resources.input.send_key(use_key.key);
        use_key.state = State::Using(Using {
            hold_completed: true,
            ..using
        });
        return;
    }

    match next_timeout_lifecycle(using.hold_timeout, use_key.key_hold_ticks) {
        Lifecycle::Started(timeout) => {
            resources.input.send_key_down_with_options(
                use_key.key,
                InputKeyDownOptions::default().repeatable(),
            );
            use_key.state = State::Using(Using {
                hold_timeout: timeout,
                hold_completed: use_key.should_buffer_key_holding(),
                ..using
            });
        }
        Lifecycle::Updated(timeout) => {
            resources.input.send_key_down_with_options(
                use_key.key,
                InputKeyDownOptions::default().repeatable(),
            );
            use_key.state = State::Using(Using {
                hold_timeout: timeout,
                ..using
            });
        }
        Lifecycle::Ended => {
            resources.input.send_key_up(use_key.key);
            use_key.state = State::Using(Using {
                hold_completed: true,
                ..using
            });
        }
    }
}

#[inline]
fn update_linking_key(resources: &Resources, use_key: &mut UseKey, link_key_timing_millis: u64) {
    let State::Using(using) = use_key.state else {
        panic!("use key state is not using");
    };
    let link_key = use_key.link_key;
    let min_timeout = if matches!(link_key, LinkKeyKind::Along(_)) {
        2
    } else {
        1
    };
    let link_key_timeout = ((link_key_timing_millis / MS_PER_TICK) as u32).max(min_timeout);

    match next_timeout_lifecycle(using.link_timeout, link_key_timeout) {
        Lifecycle::Started(timeout) => {
            match link_key {
                LinkKeyKind::Before(key) => {
                    resources.input.send_key(key);
                }
                LinkKeyKind::Along(key) => {
                    resources.input.send_key_down(key);
                }
                LinkKeyKind::AtTheSame(_) | LinkKeyKind::After(_) => (),
                LinkKeyKind::None => panic!("there is no link key"),
            }
            use_key.state = State::Using(Using {
                link_timeout: timeout,
                ..using
            });
        }
        Lifecycle::Ended => {
            match link_key {
                LinkKeyKind::After(key) => {
                    resources.input.send_key(key);
                }
                LinkKeyKind::Along(key) => {
                    resources.input.send_key_up(key);
                }
                LinkKeyKind::AtTheSame(_) | LinkKeyKind::Before(_) => (),
                LinkKeyKind::None => panic!("there is no link key"),
            }
            use_key.state = State::Using(Using {
                link_completed: true,
                ..using
            });
        }
        Lifecycle::Updated(timeout) => {
            if matches!(link_key, LinkKeyKind::Along(_)) && timeout.total == LINK_ALONG_PRESS_TICK {
                resources.input.send_key(use_key.key);
            }
            use_key.state = State::Using(Using {
                link_timeout: timeout,
                ..using
            });
        }
    }
}

#[inline]
fn random_wait_ticks(wait_base_ticks: u32, wait_random_range: u32) -> u32 {
    // TODO: Replace rand with Rng
    let wait_min = wait_base_ticks.saturating_sub(wait_random_range);
    let wait_max = wait_base_ticks.saturating_add(wait_random_range + 1);
    rand::random_range(wait_min..wait_max)
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use mockall::{Sequence, predicate::eq};
    use opencv::core::Point;

    use super::LinkKeyKind;
    use crate::{
        ActionKeyDirection, ActionKeyWith, WaitAfterBuffered,
        bridge::{InputKeyDownOptions, KeyKind, MockInput},
        ecs::Resources,
        minimap::Minimap,
        player::{
            Player, PlayerContext, PlayerEntity, Timeout,
            double_jump::DoubleJumping,
            state::BufferedStalling,
            use_key::{PendingTransition, State, UseKey, Using, update_use_key_state},
        },
    };

    fn make_player(use_key: UseKey) -> PlayerEntity {
        PlayerEntity {
            state: Player::UseKey(use_key),
            context: PlayerContext::default(),
        }
    }

    #[test]
    fn update_use_key_state_ensuring_use_with_stationary() {
        let resources = Resources::new(None, None);
        let mut player = make_player(UseKey {
            key: KeyKind::A,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::None,
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
            action_info: None,
            state: State::Precondition,
            wait_after_buffered: WaitAfterBuffered::None,
            pending_transition: PendingTransition::None,
        });

        // Start EnsuringUseWith
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::EnsuringUseWith,
                ..
            })
        );

        // Complete EnsuringUseWith when stationary
        player.context.is_stationary = true;
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::Precondition,
                ..
            })
        );
    }

    #[test]
    fn update_use_key_state_ensuring_use_with_double_jump() {
        let resources = Resources::new(None, None);
        let mut player = make_player(UseKey {
            key: KeyKind::A,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::None,
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::DoubleJump,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
            action_info: None,
            state: State::Precondition,
            wait_after_buffered: WaitAfterBuffered::None,
            pending_transition: PendingTransition::None,
        });
        player.context.last_known_pos = Some(Point::default());

        // Start EnsuringUseWith
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::EnsuringUseWith,
                ..
            })
        );

        // Transitions to double jump
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::DoubleJumping(DoubleJumping { forced: true, .. })
        );
    }

    #[test]
    fn update_use_key_state_changing_direction() {
        let mut keys = MockInput::new();
        keys.expect_is_key_cleared()
            .withf(|k| matches!(k, KeyKind::Left))
            .returning(|_| true);
        keys.expect_send_key()
            .withf(|k| matches!(k, KeyKind::Left))
            .once();
        let resources = Resources::new(Some(keys), None);
        let mut use_key = UseKey {
            key: KeyKind::A,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::None,
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Left,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
            action_info: None,
            state: State::Precondition,
            wait_after_buffered: WaitAfterBuffered::None,
            pending_transition: PendingTransition::None,
        };
        let mut player = make_player(use_key);

        // Transition into ChangingDirection
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::ChangingDirection(Timeout { started: false, .. }),
                ..
            })
        );

        // Sends down on started
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::ChangingDirection(Timeout { started: true, .. }),
                ..
            })
        );

        // Simulate completion of ChangingDirection
        use_key.state = State::ChangingDirection(Timeout {
            started: true,
            total: 3,
            current: 3,
        });
        player = make_player(use_key);
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.context.last_known_direction,
            ActionKeyDirection::Left
        );
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::Precondition,
                ..
            })
        );
    }

    #[test]
    fn update_use_key_state_repeats_until_count_reached() {
        let mut keys = MockInput::new();
        keys.expect_send_key()
            .times(3)
            .withf(|k| matches!(k, KeyKind::A));
        let resources = Resources::new(Some(keys), None);
        let use_key = UseKey {
            key: KeyKind::A,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::None,
            count: 3,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
            action_info: None,
            state: State::Precondition,
            wait_after_buffered: WaitAfterBuffered::None,
            pending_transition: PendingTransition::None,
        };
        let mut player = make_player(use_key);

        for i in 0..3 {
            update_use_key_state(&resources, &mut player, Minimap::Detecting);
            assert_matches!(
                player.state,
                Player::UseKey(UseKey {
                    state: State::Using(_),
                    ..
                })
            );

            update_use_key_state(&resources, &mut player, Minimap::Detecting);
            assert_matches!(
                player.state,
                Player::UseKey(UseKey {
                    state: State::Postcondition,
                    ..
                })
            );

            update_use_key_state(&resources, &mut player, Minimap::Detecting);
            if i == 2 {
                assert_matches!(player.state, Player::Idle);
            } else {
                assert_matches!(
                    player.state,
                    Player::UseKey(UseKey {
                        state: State::Precondition,
                        ..
                    })
                );
            }
        }
    }

    #[test]
    fn update_use_key_state_waits_before() {
        let resources = Resources::new(None, None);
        let use_key = UseKey {
            key: KeyKind::A,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::None,
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 5,
            wait_after_use_ticks: 0,
            action_info: None,
            state: State::Precondition,
            wait_after_buffered: WaitAfterBuffered::None,
            pending_transition: PendingTransition::None,
        };
        let mut player = make_player(use_key);

        update_use_key_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::Stalling(_, 5));
        assert_matches!(
            player.context.stalling_timeout_state,
            Some(Player::UseKey(UseKey {
                state: State::Using(_),
                pending_transition: PendingTransition::None,
                ..
            }))
        );
    }

    #[test]
    fn update_use_key_state_waits_after() {
        let mut keys = MockInput::new();
        keys.expect_send_key()
            .withf(|k| matches!(k, KeyKind::A))
            .once();
        let resources = Resources::new(Some(keys), None);
        let use_key = UseKey {
            key: KeyKind::A,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::None,
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 7,
            action_info: None,
            wait_after_buffered: WaitAfterBuffered::None,
            state: State::Using(Using::default()),
            pending_transition: PendingTransition::None,
        };
        let mut player = make_player(use_key);

        update_use_key_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::Stalling(_, 7));
        assert_matches!(
            player.context.stalling_timeout_state,
            Some(Player::UseKey(UseKey {
                state: State::Postcondition,
                pending_transition: PendingTransition::None,
                ..
            }))
        );
    }

    #[test]
    fn update_use_key_state_link_key_along() {
        let mut sequence = Sequence::new();
        let mut keys = MockInput::new();
        keys.expect_send_key_down()
            .withf(|k| matches!(k, KeyKind::Alt))
            .once()
            .in_sequence(&mut sequence);
        keys.expect_send_key()
            .withf(|k| matches!(k, KeyKind::A))
            .once()
            .in_sequence(&mut sequence);
        keys.expect_send_key_up()
            .withf(|k| matches!(k, KeyKind::Alt))
            .once()
            .in_sequence(&mut sequence);
        let resources = Resources::new(Some(keys), None);
        let mut use_key = UseKey {
            key: KeyKind::A,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::Along(KeyKind::Alt),
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
            action_info: None,
            state: State::Using(Using::default()),
            wait_after_buffered: WaitAfterBuffered::None,
            pending_transition: PendingTransition::None,
        };
        let mut player = make_player(use_key);

        // Hold Alt
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::Using(Using {
                    link_completed: false,
                    ..
                }),
                ..
            })
        );

        // Press A
        use_key.state = State::Using(Using {
            link_timeout: Timeout {
                current: 1,
                total: 1,
                started: true,
            },
            ..Default::default()
        });
        player = make_player(use_key);
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::Using(Using {
                    link_completed: false,
                    ..
                }),
                ..
            })
        );

        // Release Alt
        use_key.state = State::Using(Using {
            link_timeout: Timeout {
                current: 2,
                total: 2,
                started: true,
            },
            ..Default::default()
        });
        player = make_player(use_key);
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::Using(Using {
                    link_completed: true,
                    ..
                }),
                ..
            })
        );
    }

    #[test]
    fn update_use_key_state_link_key_before() {
        let mut sequence = Sequence::new();
        let mut keys = MockInput::new();
        keys.expect_send_key()
            .withf(|k| matches!(k, KeyKind::Alt))
            .once()
            .in_sequence(&mut sequence);
        keys.expect_send_key()
            .withf(|k| matches!(k, KeyKind::A))
            .once()
            .in_sequence(&mut sequence);
        let resources = Resources::new(Some(keys), None);

        let mut use_key = UseKey {
            key: KeyKind::A,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::Before(KeyKind::Alt),
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
            action_info: None,
            state: State::Using(Using::default()),
            wait_after_buffered: WaitAfterBuffered::None,
            pending_transition: PendingTransition::None,
        };
        let mut player = make_player(use_key);

        // Press Alt
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::Using(Using {
                    link_completed: false,
                    hold_completed: false,
                    ..
                }),
                ..
            })
        );

        // Press A
        use_key.state = State::Using(Using {
            link_completed: true,
            ..Default::default()
        });
        player = make_player(use_key);
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
    }

    #[test]
    fn update_use_key_state_link_key_after() {
        let mut sequence = Sequence::new();
        let mut keys = MockInput::new();
        keys.expect_send_key()
            .withf(|k| matches!(k, KeyKind::A))
            .once()
            .in_sequence(&mut sequence);
        keys.expect_send_key()
            .withf(|k| matches!(k, KeyKind::Alt))
            .once()
            .in_sequence(&mut sequence);
        let resources = Resources::new(Some(keys), None);

        let mut use_key = UseKey {
            key: KeyKind::A,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::After(KeyKind::Alt),
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
            action_info: None,
            state: State::Using(Using::default()),
            wait_after_buffered: WaitAfterBuffered::None,
            pending_transition: PendingTransition::None,
        };
        let mut player = make_player(use_key);

        // Press A
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::Using(Using {
                    link_completed: false,
                    hold_completed: true,
                    ..
                }),
                ..
            })
        );

        // Press Alt
        use_key.state = State::Using(Using {
            link_timeout: Timeout {
                current: 1, // Min is 1
                started: true,
                ..Default::default()
            },
            hold_completed: true,
            ..Default::default()
        });
        player = make_player(use_key);
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
    }

    #[test]
    fn update_use_key_state_link_key_at_the_same() {
        let mut sequence = Sequence::new();
        let mut keys = MockInput::new();
        keys.expect_send_key()
            .withf(|k| matches!(k, KeyKind::Alt))
            .once()
            .in_sequence(&mut sequence);
        keys.expect_send_key()
            .withf(|k| matches!(k, KeyKind::A))
            .once()
            .in_sequence(&mut sequence);
        let resources = Resources::new(Some(keys), None);

        let use_key = UseKey {
            key: KeyKind::A,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::AtTheSame(KeyKind::Alt),
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
            action_info: None,
            state: State::Using(Using::default()),
            wait_after_buffered: WaitAfterBuffered::None,
            pending_transition: PendingTransition::None,
        };
        let mut player = make_player(use_key);

        // Press Alt then A
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::Postcondition,
                ..
            })
        );
    }

    #[test]
    fn update_use_key_state_hold_key() {
        let mut sequence = Sequence::new();
        let mut keys = MockInput::new();
        keys.expect_send_key_down_with_options()
            .with(
                eq(KeyKind::A),
                eq(InputKeyDownOptions::default().repeatable()),
            )
            .times(4)
            .in_sequence(&mut sequence);
        keys.expect_send_key_up()
            .with(eq(KeyKind::A))
            .times(1)
            .in_sequence(&mut sequence);

        let resources = Resources::new(Some(keys), None);
        let use_key = UseKey {
            key: KeyKind::A,
            key_hold_ticks: 3,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::None,
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
            wait_after_buffered: WaitAfterBuffered::None,
            pending_transition: PendingTransition::None,
            action_info: None,
            state: State::Using(Using::default()),
        };

        let mut player = make_player(use_key);

        for _ in 0..4 {
            update_use_key_state(&resources, &mut player, Minimap::Detecting);
        }
        update_use_key_state(&resources, &mut player, Minimap::Detecting);
    }

    #[test]
    fn update_use_key_state_buffer_hold_key() {
        let mut sequence = Sequence::new();
        let mut keys = MockInput::new();

        keys.expect_send_key_down_with_options()
            .with(
                eq(KeyKind::A),
                eq(InputKeyDownOptions::default().repeatable()),
            )
            .times(1)
            .in_sequence(&mut sequence);

        let resources = Resources::new(Some(keys), None);
        let use_key = UseKey {
            key: KeyKind::A,
            key_hold_ticks: 2,
            key_hold_buffered_to_wait_after: true,
            link_key: LinkKeyKind::None,
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 4,
            wait_after_buffered: WaitAfterBuffered::Interruptible,
            pending_transition: PendingTransition::None,
            action_info: None,
            state: State::Using(Using::default()),
        };

        let mut player = make_player(use_key);

        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.state,
            Player::UseKey(UseKey {
                state: State::Using(Using {
                    hold_completed: true,
                    ..
                }),
                ..
            })
        );

        update_use_key_state(&resources, &mut player, Minimap::Detecting);
        assert_matches!(
            player.context.stalling_buffered,
            BufferedStalling::Interruptible(_, 6)
        );
        assert!(
            player
                .context
                .stalling_timeout_buffered_update_callback
                .is_some()
        );
        assert!(
            player
                .context
                .stalling_timeout_buffered_end_callback
                .is_some()
        );
    }

    #[test]
    fn update_use_key_state_only_last_use_buffers_wait_after() {
        let mut sequence = Sequence::new();
        let mut keys = MockInput::new();

        // First usage
        keys.expect_send_key_down_with_options()
            .with(
                eq(KeyKind::A),
                eq(InputKeyDownOptions::default().repeatable()),
            )
            .times(2)
            .in_sequence(&mut sequence);
        keys.expect_send_key_up()
            .with(eq(KeyKind::A))
            .times(1)
            .in_sequence(&mut sequence);

        // Second usage
        keys.expect_send_key_down_with_options()
            .with(
                eq(KeyKind::A),
                eq(InputKeyDownOptions::default().repeatable()),
            )
            .times(1)
            .in_sequence(&mut sequence);

        let resources = Resources::new(Some(keys), None);
        let use_key = UseKey {
            key: KeyKind::A,
            count: 2,
            current_count: 0,
            key_hold_ticks: 1,
            key_hold_buffered_to_wait_after: true,

            wait_before_use_ticks: 0,
            wait_after_use_ticks: 5,
            wait_after_buffered: WaitAfterBuffered::Uninterruptible,

            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            link_key: LinkKeyKind::None,

            pending_transition: PendingTransition::None,
            action_info: None,
            state: State::Using(Using::default()),
        };

        let mut player = make_player(use_key);

        update_use_key_state(&resources, &mut player, Minimap::Detecting); // hold
        update_use_key_state(&resources, &mut player, Minimap::Detecting); // hold 
        update_use_key_state(&resources, &mut player, Minimap::Detecting); // release
        update_use_key_state(&resources, &mut player, Minimap::Detecting); // postcondition
        assert!(!player.context.stalling_buffered.stalling());
        assert!(
            player
                .context
                .stalling_timeout_buffered_update_callback
                .is_none()
        );
        assert!(
            player
                .context
                .stalling_timeout_buffered_end_callback
                .is_none()
        );
        assert_matches!(
            player.context.stalling_timeout_state,
            Some(Player::UseKey(UseKey {
                state: State::Postcondition,
                ..
            }))
        );

        let mut player = make_player(UseKey {
            current_count: 1,
            state: State::Using(Using::default()),
            ..use_key
        });
        update_use_key_state(&resources, &mut player, Minimap::Detecting); // hold
        update_use_key_state(&resources, &mut player, Minimap::Detecting); // buffer
        assert_matches!(
            player.context.stalling_buffered,
            BufferedStalling::Uninterruptible(_, _)
        );
        assert!(
            player
                .context
                .stalling_timeout_buffered_update_callback
                .is_some()
        );
        assert!(
            player
                .context
                .stalling_timeout_buffered_end_callback
                .is_some()
        );
    }
}
