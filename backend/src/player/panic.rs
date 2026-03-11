use log::info;

use super::{Player, actions::PanicTo, timeout::Timeout};
use crate::{
    bridge::KeyKind,
    ecs::Resources,
    minimap::Minimap,
    player::{
        PlayerEntity, next_action,
        timeout::{Lifecycle, next_timeout_lifecycle},
    },
    rng::Rng,
};

const MAX_RETRY: u32 = 3;

/// States of panicking mode.
#[derive(Debug, Clone, Copy)]
enum State {
    /// Cycling through channels.
    ChangingChannel(Timeout, u32),
    /// Going to town.
    GoingToTown(Timeout, u32),
    Completing(Timeout, bool),
}

#[derive(Debug, Clone, Copy)]
struct PanicToChannel {
    cycle_to_right: bool,
    cycle_count: u32,
    current_cycle_count: u32,
}

impl PanicToChannel {
    fn new(rng: &Rng) -> Self {
        Self {
            cycle_to_right: rng.random_bool(0.5),
            cycle_count: rng.random_range(1..=5),
            current_cycle_count: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Panicking {
    state: State,
    pub to: PanicTo,
    to_channel: Option<PanicToChannel>,
}

impl Panicking {
    pub fn new(rng: &Rng, to: PanicTo) -> Self {
        Self {
            state: match to {
                PanicTo::Channel => State::ChangingChannel(Timeout::default(), 0),
                PanicTo::Town => State::GoingToTown(Timeout::default(), 0),
            },
            to,
            to_channel: if matches!(to, PanicTo::Channel) {
                Some(PanicToChannel::new(rng))
            } else {
                None
            },
        }
    }
}

/// Updates [`Player::Panicking`] contextual state.
pub fn update_panicking_state(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
    mut panicking: Panicking,
) {
    match panicking.state {
        State::ChangingChannel(_, _) => {
            let key = match player.context.config.change_channel_key {
                Some(val) => val,
                None => {
                    info!(target:"backend/player","aborted panicking because change channel key is not set");
                    player.context.clear_action_completed();
                    player.state = Player::Idle;
                    return;
                }
            };

            update_changing_channel(resources, &mut panicking, minimap_state, key)
        }
        State::GoingToTown(_, _) => {
            let key = match player.context.config.to_town_key {
                Some(val) => val,
                None => {
                    info!(target:"backend/player","aborted panicking because to town key is not set");
                    player.context.clear_action_completed();
                    player.state = Player::Idle;
                    return;
                }
            };

            update_going_to_town(resources, &mut panicking, key)
        }
        State::Completing(_, _) => update_completing(resources, &mut panicking, minimap_state),
    }

    let player_next_state = if matches!(panicking.state, State::Completing(_, true)) {
        Player::Idle
    } else {
        Player::Panicking(panicking)
    };

    match next_action(&player.context) {
        Some(_) => {
            if matches!(player_next_state, Player::Idle) {
                player.context.clear_action_completed();
            }

            player.state = player_next_state;
        }
        None => {
            // Allow continuing for town even if the bot has already halted
            // Force cancel if it is not initiated from an action for other panic kind
            player.state = if matches!(panicking.to, PanicTo::Town) {
                player_next_state
            } else {
                Player::Idle
            };
        }
    }
}

fn update_changing_channel(
    resources: &Resources,
    panicking: &mut Panicking,
    minimap_state: Minimap,
    key: KeyKind,
) {
    const INITIAL_DELAY: u32 = 150;
    const PRESS_KEY_INTERVAL: u32 = 15;
    const WAIT_UPDATE_MARGIN: u32 = 10;

    let State::ChangingChannel(timeout, retry_count) = panicking.state else {
        panic!("panicking state is not changing channel")
    };

    let to_channel = panicking
        .to_channel
        .as_mut()
        .expect("channel panic must have PanicToChannel");

    let start_delay = if retry_count == 0 { INITIAL_DELAY } else { 0 };
    let cycle_timeout = to_channel.cycle_count * PRESS_KEY_INTERVAL;
    let total_timeout = start_delay + cycle_timeout + PRESS_KEY_INTERVAL + WAIT_UPDATE_MARGIN;

    match next_timeout_lifecycle(timeout, total_timeout) {
        Lifecycle::Started(timeout) => {
            if !resources.detector().detect_change_channel_menu_opened() {
                resources.input.send_key(key);
            }

            panicking.state = State::ChangingChannel(timeout, retry_count);
        }

        Lifecycle::Ended => {
            if !matches!(minimap_state, Minimap::Idle(_)) {
                panicking.state = State::Completing(Timeout::default(), false);
                return;
            }

            panicking.state = if retry_count < MAX_RETRY {
                State::ChangingChannel(Timeout::default(), retry_count + 1)
            } else {
                State::Completing(Timeout::default(), true)
            };
        }

        Lifecycle::Updated(timeout) => {
            if resources.detector().detect_change_channel_menu_opened() {
                let direction_key = if to_channel.cycle_to_right {
                    KeyKind::Right
                } else {
                    KeyKind::Left
                };

                let tick = timeout.current;
                let cycle_relative_tick = tick.saturating_sub(start_delay);
                if cycle_relative_tick > 0
                    && cycle_relative_tick.is_multiple_of(PRESS_KEY_INTERVAL)
                    && to_channel.current_cycle_count < to_channel.cycle_count
                {
                    resources.input.send_key(direction_key);
                    to_channel.current_cycle_count += 1;
                }

                let enter_relative_tick = cycle_relative_tick.saturating_sub(cycle_timeout);
                if enter_relative_tick > 0 && enter_relative_tick.is_multiple_of(PRESS_KEY_INTERVAL)
                {
                    resources.input.send_key(KeyKind::Enter);
                }
            }

            panicking.state = State::ChangingChannel(timeout, retry_count);
        }
    }
}

fn update_going_to_town(resources: &Resources, panicking: &mut Panicking, key: KeyKind) {
    let State::GoingToTown(timeout, retry_count) = panicking.state else {
        panic!("panicking state is not going to town")
    };

    match next_timeout_lifecycle(timeout, 90) {
        Lifecycle::Started(timeout) => {
            resources.input.send_key(key);
            panicking.state = State::GoingToTown(timeout, retry_count);
        }

        Lifecycle::Ended => {
            let has_confirm_button = resources.detector().detect_popup_confirm_button().is_ok();
            if has_confirm_button {
                resources.input.send_key(KeyKind::Enter);
            }

            panicking.state = if !has_confirm_button && retry_count < MAX_RETRY {
                State::GoingToTown(Timeout::default(), retry_count + 1)
            } else {
                State::Completing(Timeout::default(), true)
            };
        }
        Lifecycle::Updated(timeout) => {
            panicking.state = State::GoingToTown(timeout, retry_count);
        }
    }
}

fn update_completing(resources: &Resources, panicking: &mut Panicking, minimap_state: Minimap) {
    let State::Completing(timeout, completed) = panicking.state else {
        panic!("panicking state is not completing")
    };

    if matches!(panicking.to, PanicTo::Town) {
        panicking.state = State::Completing(timeout, true);
        return;
    }

    match next_timeout_lifecycle(timeout, 245) {
        Lifecycle::Ended => match minimap_state {
            Minimap::Idle(idle) => {
                if idle.has_any_other_player() {
                    panicking.to_channel = Some(PanicToChannel::new(&resources.rng));
                    panicking.state = State::ChangingChannel(Timeout::default(), 0);
                } else {
                    panicking.state = State::Completing(timeout, true);
                }
            }
            Minimap::Detecting => {
                panicking.state = State::Completing(Timeout::default(), false);
            }
        },
        Lifecycle::Started(timeout) | Lifecycle::Updated(timeout) => {
            panicking.state = State::Completing(timeout, completed);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use anyhow::{Ok, anyhow};
    use mockall::predicate::eq;
    use opencv::core::Rect;

    use super::*;
    use crate::{
        bridge::MockInput,
        detect::MockDetector,
        minimap::{Minimap, MinimapIdle},
    };

    #[test]
    fn update_changing_channel_and_send_key_keys() {
        let mut panicking = Panicking::new(&Rng::default(), PanicTo::Channel);
        let cycle_count = panicking.to_channel.unwrap().cycle_count;

        let mut keys = MockInput::default();
        let mut detector = MockDetector::default();
        detector
            .expect_detect_change_channel_menu_opened()
            .return_const(true);
        keys.expect_send_key().times(1 + cycle_count as usize);

        let resources = Resources::new(Some(keys), Some(detector));

        for i in 1..=cycle_count {
            panicking.state = State::ChangingChannel(
                Timeout {
                    current: 149 + i * 15,
                    started: true,
                    ..Default::default()
                },
                0,
            );

            update_changing_channel(&resources, &mut panicking, Minimap::Detecting, KeyKind::F1);

            assert_matches!(panicking.state, State::ChangingChannel(_, _));
            assert_eq!(panicking.to_channel.unwrap().current_cycle_count, i);
        }

        panicking.state = State::ChangingChannel(
            Timeout {
                current: 149 + 15 * (cycle_count + 1),
                started: true,
                ..Default::default()
            },
            0,
        );

        update_changing_channel(&resources, &mut panicking, Minimap::Detecting, KeyKind::F1);

        assert_matches!(panicking.state, State::ChangingChannel(_, _));
        assert_eq!(
            panicking.to_channel.unwrap().current_cycle_count,
            cycle_count
        );
    }

    #[test]
    fn update_changing_channel_and_send_keys_retry() {
        let mut panicking = Panicking::new(&Rng::default(), PanicTo::Channel);
        let cycle_count = panicking.to_channel.unwrap().cycle_count;

        let mut keys = MockInput::default();
        let mut detector = MockDetector::default();
        detector
            .expect_detect_change_channel_menu_opened()
            .return_const(true);
        keys.expect_send_key().times(1 + cycle_count as usize);

        let resources = Resources::new(Some(keys), Some(detector));

        for i in 1..=cycle_count {
            panicking.state = State::ChangingChannel(
                Timeout {
                    current: i * 15 - 1,
                    started: true,
                    ..Default::default()
                },
                1,
            );

            update_changing_channel(&resources, &mut panicking, Minimap::Detecting, KeyKind::F1);

            assert_matches!(panicking.state, State::ChangingChannel(_, _));
            assert_eq!(panicking.to_channel.unwrap().current_cycle_count, i);
        }

        panicking.state = State::ChangingChannel(
            Timeout {
                current: 15 * (cycle_count + 1) - 1,
                started: true,
                ..Default::default()
            },
            1,
        );

        update_changing_channel(&resources, &mut panicking, Minimap::Detecting, KeyKind::F1);

        assert_matches!(panicking.state, State::ChangingChannel(_, _));
        assert_eq!(
            panicking.to_channel.unwrap().current_cycle_count,
            cycle_count
        );
    }

    #[test]
    fn update_changing_channel_complete_if_minimap_not_idle() {
        let resources = Resources::new(None, None);
        let mut panicking = Panicking::new(&Rng::default(), PanicTo::Channel);
        panicking.state = State::ChangingChannel(
            Timeout {
                current: 160 + (panicking.to_channel.unwrap().cycle_count + 1) * 15,
                started: true,
                ..Default::default()
            },
            0,
        );

        update_changing_channel(&resources, &mut panicking, Minimap::Detecting, KeyKind::F1);

        assert_matches!(panicking.state, State::Completing(_, false));
    }

    #[test]
    fn update_changing_channel_complete_if_minimap_not_idle_retry() {
        let resources = Resources::new(None, None);
        let mut panicking = Panicking::new(&Rng::default(), PanicTo::Channel);
        panicking.state = State::ChangingChannel(
            Timeout {
                current: 10 + (panicking.to_channel.unwrap().cycle_count + 1) * 15,
                started: true,
                ..Default::default()
            },
            1,
        );

        update_changing_channel(&resources, &mut panicking, Minimap::Detecting, KeyKind::F1);

        assert_matches!(panicking.state, State::Completing(_, false));
    }

    #[test]
    fn update_going_to_town_started_send_key() {
        let mut keys = MockInput::default();
        keys.expect_send_key().once().with(eq(KeyKind::F2));
        let resources = Resources::new(Some(keys), None);
        let mut panicking = Panicking::new(&Rng::default(), PanicTo::Town);
        panicking.state = State::GoingToTown(Timeout::default(), 0);

        update_going_to_town(&resources, &mut panicking, KeyKind::F2);

        assert_matches!(panicking.state, State::GoingToTown(_, _));
    }

    #[test]
    fn update_going_to_town_ended_send_key_and_complete_if_esc_confirm_opened() {
        let mut keys = MockInput::default();
        keys.expect_send_key().once().with(eq(KeyKind::Enter));
        let mut detector = MockDetector::default();
        detector
            .expect_detect_popup_confirm_button()
            .returning(|| Ok(Rect::default()));
        let resources = Resources::new(Some(keys), Some(detector));
        let mut panicking = Panicking::new(&Rng::default(), PanicTo::Town);
        panicking.state = State::GoingToTown(
            Timeout {
                started: true,
                current: 90,
                ..Default::default()
            },
            0,
        );

        update_going_to_town(&resources, &mut panicking, KeyKind::F2);

        assert_matches!(panicking.state, State::Completing(_, true));
    }

    #[test]
    fn update_going_to_town_ended_retry() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_popup_confirm_button()
            .returning(|| Err(anyhow!("button not found")));
        let resources = Resources::new(None, Some(detector));
        let mut panicking = Panicking::new(&Rng::default(), PanicTo::Town);
        panicking.state = State::GoingToTown(
            Timeout {
                started: true,
                current: 90,
                ..Default::default()
            },
            0,
        );

        update_going_to_town(&resources, &mut panicking, KeyKind::F2);

        assert_matches!(
            panicking.state,
            State::GoingToTown(
                Timeout {
                    started: false,
                    current: 0,
                    ..
                },
                1
            )
        );
    }

    #[test]
    fn update_completing_for_town_immediately_complete() {
        let resources = Resources::new(None, None);
        let mut panicking = Panicking::new(&Rng::default(), PanicTo::Town);
        panicking.state = State::Completing(Timeout::default(), false);

        update_completing(&resources, &mut panicking, Minimap::Detecting);

        assert_matches!(panicking.state, State::Completing(_, true));
    }

    #[test]
    fn update_completing_for_channel_switch_to_idle_if_no_players() {
        let resources = Resources::new(None, None);
        let mut panicking = Panicking::new(&Rng::default(), PanicTo::Channel);
        panicking.state = State::Completing(
            Timeout {
                current: 245,
                started: true,
                ..Default::default()
            },
            false,
        );

        update_completing(
            &resources,
            &mut panicking,
            Minimap::Idle(MinimapIdle::default()),
        );

        assert_matches!(panicking.state, State::Completing(_, true));
    }
}
