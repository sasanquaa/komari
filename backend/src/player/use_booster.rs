use super::{Player, timeout::Timeout};
use crate::{
    bridge::KeyKind,
    ecs::Resources,
    player::{
        Booster, PlayerEntity, next_action,
        timeout::{Lifecycle, next_timeout_lifecycle},
    },
};

/// States of using booster.
#[derive(Debug, Clone, Copy)]
enum State {
    /// Using the booster by pressing corresponding booster key.
    Using(Timeout),
    /// Confirming the popup dialog.
    Confirming(Timeout),
    /// Terminal state.
    Completing {
        timeout: Timeout,
        completed: bool,
        failed: bool,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct UsingBooster {
    state: State,
    kind: Booster,
}

impl UsingBooster {
    pub fn new(kind: Booster) -> Self {
        Self {
            state: State::Using(Timeout::default()),
            kind,
        }
    }
}

/// Updates [`Player::UsingBooster`] contextual state.
pub fn update_using_booster_state(resources: &Resources, player: &mut PlayerEntity) {
    let Player::UsingBooster(mut using) = player.state else {
        panic!("state is not using booster")
    };
    let key = match using.kind {
        Booster::Generic => player.context.config.generic_booster_key,
        Booster::Hexa => player.context.config.hexa_booster_key,
    };

    match using.state {
        State::Using(_) => update_using(resources, &mut using, key),
        State::Confirming(_) => update_confirming(resources, &mut using),
        State::Completing { .. } => update_completing(resources, &mut using),
    }

    let player_next_state = if matches!(
        using.state,
        State::Completing {
            completed: true,
            ..
        }
    ) {
        Player::Idle
    } else {
        Player::UsingBooster(using)
    };
    let is_terminal = matches!(player_next_state, Player::Idle);
    if is_terminal {
        if matches!(using.state, State::Completing { failed: true, .. }) {
            player.context.track_booster_fail_count(using.kind);
        } else {
            player.context.clear_booster_fail_count(using.kind);
        }
    }

    match next_action(&player.context) {
        Some(_) => {
            if is_terminal {
                player.context.clear_action_completed();
            }
        }
        None => player.state = Player::Idle, // Force cancel if it is not initiated from an action
    }
}

fn update_using(resources: &Resources, using: &mut UsingBooster, key: KeyKind) {
    const PRESS_KEY_AT: u32 = 30;

    let State::Using(timeout) = using.state else {
        panic!("using booster state is not using")
    };

    match next_timeout_lifecycle(timeout, 60) {
        Lifecycle::Started(timeout) => {
            using.state = State::Using(timeout);
        }
        Lifecycle::Ended => {
            using.state = if resources.detector().detect_admin_visible() {
                State::Confirming(Timeout::default())
            } else {
                State::Completing {
                    timeout: Timeout::default(),
                    completed: false,
                    failed: true,
                }
            };
        }
        Lifecycle::Updated(timeout) => {
            if timeout.current == PRESS_KEY_AT {
                resources.input.send_key(key);
            }
            using.state = State::Using(timeout);
        }
    }
}

fn update_confirming(resources: &Resources, using: &mut UsingBooster) {
    let State::Confirming(timeout) = using.state else {
        panic!("using booster state is not confirming")
    };

    match next_timeout_lifecycle(timeout, 30) {
        Lifecycle::Started(timeout) => {
            resources.input.send_key(KeyKind::Left);
            using.state = State::Confirming(timeout);
        }
        Lifecycle::Ended => {
            resources.input.send_key(KeyKind::Enter);
            using.state = State::Completing {
                timeout: Timeout::default(),
                completed: false,
                failed: false,
            };
        }
        Lifecycle::Updated(timeout) => {
            if timeout.current == 15 {
                resources.input.send_key(KeyKind::Left);
            }
            using.state = State::Confirming(timeout);
        }
    }
}

fn update_completing(resources: &Resources, using: &mut UsingBooster) {
    let State::Completing {
        timeout,
        completed,
        failed,
    } = using.state
    else {
        panic!("using booster state is not completing")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) | Lifecycle::Updated(timeout) => {
            using.state = State::Completing {
                timeout,
                completed,
                failed,
            };
        }
        Lifecycle::Ended => {
            if resources.detector().detect_esc_settings() {
                resources.input.send_key(KeyKind::Esc);
            }
            using.state = State::Completing {
                timeout,
                completed: true,
                failed,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use mockall::predicate::eq;

    use super::*;
    use crate::{
        bridge::{KeyKind, MockInput},
        detect::MockDetector,
        ecs::Resources,
        player::{Booster, timeout::Timeout},
    };

    #[test]
    fn update_using_presses_key_at_tick() {
        let mut keys = MockInput::default();
        keys.expect_send_key().with(eq(KeyKind::F1)).once(); // Will press booster key at tick 30

        let resources = Resources::new(Some(keys), None);
        let mut using = UsingBooster::new(Booster::Generic);
        using.state = State::Using(Timeout {
            current: 29, // one before PRESS_KEY_AT
            started: true,
            ..Default::default()
        });

        update_using(&resources, &mut using, KeyKind::F1);

        assert_matches!(using.state, State::Using(_));
    }

    #[test]
    fn update_using_detects_admin_and_moves_to_confirming() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_admin_visible()
            .once()
            .returning(|| true);
        let resources = Resources::new(None, Some(detector));

        let mut using = UsingBooster::new(Booster::Generic);
        using.state = State::Using(Timeout {
            current: 60,
            started: true,
            ..Default::default()
        });

        update_using(&resources, &mut using, KeyKind::F1);

        assert_matches!(using.state, State::Confirming(_));
    }

    #[test]
    fn update_using_fails_if_admin_not_visible() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_admin_visible()
            .once()
            .returning(|| false);
        let resources = Resources::new(None, Some(detector));

        let mut using = UsingBooster::new(Booster::Generic);
        using.state = State::Using(Timeout {
            current: 60,
            started: true,
            ..Default::default()
        });

        update_using(&resources, &mut using, KeyKind::F1);

        assert_matches!(
            using.state,
            State::Completing {
                failed: true,
                completed: false,
                ..
            }
        );
    }

    #[test]
    fn update_confirming_starts_and_presses_left() {
        let mut keys = MockInput::default();
        keys.expect_send_key().with(eq(KeyKind::Left)).once();
        let resources = Resources::new(Some(keys), None);

        let mut using = UsingBooster::new(Booster::Generic);
        using.state = State::Confirming(Timeout::default());

        update_confirming(&resources, &mut using);
        assert_matches!(using.state, State::Confirming(_));
    }

    #[test]
    fn update_confirming_updates_and_presses_left_at_tick_15() {
        let mut keys = MockInput::default();
        keys.expect_send_key().with(eq(KeyKind::Left)).once();
        let resources = Resources::new(Some(keys), None);

        let mut using = UsingBooster::new(Booster::Generic);
        using.state = State::Confirming(Timeout {
            current: 14,
            started: true,
            ..Default::default()
        });

        update_confirming(&resources, &mut using);
        assert_matches!(using.state, State::Confirming(_));
    }

    #[test]
    fn update_confirming_ends_and_presses_enter() {
        let mut keys = MockInput::default();
        keys.expect_send_key().with(eq(KeyKind::Enter)).once();
        let resources = Resources::new(Some(keys), None);

        let mut using = UsingBooster::new(Booster::Generic);
        using.state = State::Confirming(Timeout {
            current: 30,
            started: true,
            ..Default::default()
        });

        update_confirming(&resources, &mut using);

        assert_matches!(
            using.state,
            State::Completing {
                completed: false,
                failed: false,
                ..
            }
        );
    }

    #[test]
    fn update_completing_detects_esc_and_presses_esc_on_end() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_esc_settings()
            .once()
            .returning(|| true);
        let mut keys = MockInput::default();
        keys.expect_send_key().with(eq(KeyKind::Esc)).once();
        let resources = Resources::new(Some(keys), Some(detector));

        let mut using = UsingBooster::new(Booster::Generic);
        using.state = State::Completing {
            timeout: Timeout {
                current: 20, // trigger Lifecycle::Ended
                started: true,
                ..Default::default()
            },
            completed: false,
            failed: false,
        };

        update_completing(&resources, &mut using);

        assert_matches!(
            using.state,
            State::Completing {
                completed: true,
                ..
            }
        );
    }

    #[test]
    fn update_completing_updates_without_pressing_esc() {
        let detector = MockDetector::default(); // no call expected
        let keys = MockInput::default();
        let resources = Resources::new(Some(keys), Some(detector));

        let mut using = UsingBooster::new(Booster::Generic);
        using.state = State::Completing {
            timeout: Timeout {
                current: 10,
                started: true,
                ..Default::default()
            },
            completed: false,
            failed: false,
        };

        update_completing(&resources, &mut using);
        assert_matches!(
            using.state,
            State::Completing {
                completed: false,
                ..
            }
        );
    }
}
