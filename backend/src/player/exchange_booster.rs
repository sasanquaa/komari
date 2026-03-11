use opencv::core::Rect;
use rand_distr::num_traits::clamp;

use super::{Player, timeout::Timeout};
use crate::{
    bridge::{KeyKind, MouseKind},
    ecs::Resources,
    player::{
        Booster, PlayerEntity, next_action,
        timeout::{Lifecycle, next_timeout_lifecycle},
    },
};

#[derive(Debug, Clone)]
enum ExchangeAmount {
    All,
    Specific(ExchangeAmountSpecific),
}

#[derive(Debug, Clone)]
struct ExchangeAmountSpecific {
    index: usize,
    keys: Vec<KeyKind>,
}

impl ExchangeAmountSpecific {
    fn increment_index(mut self) -> ExchangeAmountSpecific {
        self.index += 1;
        self
    }
}

/// States of exchanging HEXA booster.
#[derive(Debug, Clone, Copy)]
enum State {
    /// Opening the `HEXA Matrix` menu.
    OpenHexaMenu(Timeout),
    /// Opening the `Erda conversion` menu.
    OpenExchangingMenu(Timeout, Rect),
    /// Opening the `HEXA Booster` menu inside `Erda conversion`.
    OpenBoosterMenu(Timeout, Rect),
    /// Typing the amount or clicking `MAX` button.
    Exchanging(Timeout, Rect),
    /// Confirming by clicking the `Convert` button.
    Confirming(Timeout, Rect),
    /// Terminal state.
    Completing(Timeout, bool),
}

#[derive(Debug, Clone)]
pub struct ExchangingBooster {
    state: State,
    amount: ExchangeAmount,
    success: bool,
}

impl ExchangingBooster {
    // TODO: These args should probably be represented by an enum?
    pub fn new(amount: u32, all: bool) -> Self {
        let amount = if all {
            ExchangeAmount::All
        } else {
            let amount = clamp(amount, 1, 20);
            let str = amount.to_string();

            let mut keys = vec![KeyKind::Backspace, KeyKind::Backspace];
            let keys_from_chars = str.chars().map(|char| match char {
                '0' => KeyKind::Zero,
                '1' => KeyKind::One,
                '2' => KeyKind::Two,
                '3' => KeyKind::Three,
                '4' => KeyKind::Four,
                '5' => KeyKind::Five,
                '6' => KeyKind::Six,
                '7' => KeyKind::Seven,
                '8' => KeyKind::Eight,
                '9' => KeyKind::Nine,
                _ => unreachable!(),
            });
            for key in keys_from_chars {
                keys.push(key);
            }

            ExchangeAmount::Specific(ExchangeAmountSpecific { index: 0, keys })
        };

        Self {
            state: State::OpenHexaMenu(Timeout::default()),
            amount,
            success: false,
        }
    }
}

/// Updates [`Player::ExchangingBooster`] contextual state.
pub fn update_exchanging_booster_state(resources: &Resources, player: &mut PlayerEntity) {
    let Player::ExchangingBooster(mut exchanging) = player.state.clone() else {
        panic!("state is not exchanging booster")
    };

    match exchanging.state {
        State::OpenHexaMenu(_) => update_open_hexa_menu(resources, &mut exchanging),
        State::OpenExchangingMenu(_, _) => update_open_exchanging_menu(resources, &mut exchanging),
        State::OpenBoosterMenu(_, _) => update_open_booster_menu(resources, &mut exchanging),
        State::Exchanging(_, _) => update_exchanging(resources, &mut exchanging),
        State::Confirming(_, _) => update_confirming(resources, &mut exchanging),
        State::Completing(_, _) => update_completing(resources, &mut exchanging),
    }

    let did_success = exchanging.success;
    let player_next_state = if matches!(exchanging.state, State::Completing(_, true)) {
        Player::Idle
    } else {
        Player::ExchangingBooster(exchanging)
    };
    let is_terminal = matches!(player_next_state, Player::Idle);

    match next_action(&player.context) {
        Some(_) => {
            if !is_terminal {
                return;
            }

            player.context.clear_action_completed();
            if did_success {
                player.context.clear_booster_fail_count(Booster::Hexa);
            }
        }

        None => player.state = Player::Idle,
    }
}

fn update_open_hexa_menu(resources: &Resources, exchanging: &mut ExchangingBooster) {
    let State::OpenHexaMenu(timeout) = exchanging.state else {
        panic!("exchanging booster state is not opening hexa menu")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) => {
            let (x, y) = match resources
                .detector()
                .detect_hexa_quick_menu()
                .ok()
                .map(bbox_click_point)
            {
                Some(val) => val,
                None => {
                    exchanging.state = State::Completing(Timeout::default(), true);
                    return;
                }
            };

            resources.input.send_mouse(x, y, MouseKind::Click);
            exchanging.state = State::OpenHexaMenu(timeout);
        }
        Lifecycle::Ended => {
            let bbox = match resources.detector().detect_hexa_erda_conversion_button() {
                Ok(val) => val,
                Err(_) => {
                    exchanging.state = State::Completing(Timeout::default(), false);
                    return;
                }
            };

            exchanging.state = State::OpenExchangingMenu(Timeout::default(), bbox);
        }
        Lifecycle::Updated(timeout) => exchanging.state = State::OpenHexaMenu(timeout),
    }
}

fn update_open_exchanging_menu(resources: &Resources, exchanging: &mut ExchangingBooster) {
    let State::OpenExchangingMenu(timeout, bbox) = exchanging.state else {
        panic!("exchanging booster state is not opening exchanging menu")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) => {
            let (x, y) = bbox_click_point(bbox);
            resources.input.send_mouse(x, y, MouseKind::Click);
            exchanging.state = State::OpenExchangingMenu(timeout, bbox);
        }
        Lifecycle::Ended => {
            let bbox = match resources.detector().detect_hexa_booster_button() {
                Ok(val) => val,
                Err(_) => {
                    exchanging.state = State::Completing(Timeout::default(), false);
                    return;
                }
            };

            exchanging.state = State::OpenBoosterMenu(Timeout::default(), bbox);
        }
        Lifecycle::Updated(timeout) => {
            exchanging.state = State::OpenExchangingMenu(timeout, bbox);
        }
    }
}

fn update_open_booster_menu(resources: &Resources, exchanging: &mut ExchangingBooster) {
    let State::OpenBoosterMenu(timeout, bbox) = exchanging.state else {
        panic!("exchanging booster state is not opening booster menu")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) => {
            let (x, y) = bbox_click_point(bbox);
            resources.input.send_mouse(x, y, MouseKind::Click);
            exchanging.state = State::OpenBoosterMenu(timeout, bbox);
        }
        Lifecycle::Ended => {
            let bbox = match resources.detector().detect_hexa_max_button() {
                Ok(val) => val,
                Err(_) => {
                    exchanging.state = State::Completing(Timeout::default(), false);
                    return;
                }
            };

            exchanging.state = State::Exchanging(Timeout::default(), bbox);
        }
        Lifecycle::Updated(timeout) => {
            exchanging.state = State::OpenBoosterMenu(timeout, bbox);
        }
    }
}

fn update_exchanging(resources: &Resources, exchanging: &mut ExchangingBooster) {
    const TYPE_INTERVAL: u32 = 10;

    let State::Exchanging(timeout, bbox) = exchanging.state else {
        panic!("exchanging booster state is not exchanging")
    };
    let amount = exchanging.amount.clone();
    let is_specific_amount = matches!(amount, ExchangeAmount::Specific(_));
    let max_timeout = if is_specific_amount { 60 } else { 20 };

    match next_timeout_lifecycle(timeout, max_timeout) {
        Lifecycle::Started(timeout) => {
            let (mut x, y) = bbox_click_point(bbox);
            if is_specific_amount {
                x += 100;
            }
            resources.input.send_mouse(x, y, MouseKind::Click);
            exchanging.state = State::Exchanging(timeout, bbox);
        }
        Lifecycle::Ended => {
            let bbox = match resources.detector().detect_hexa_convert_button() {
                Ok(val) => val,
                Err(_) => {
                    exchanging.state = State::Completing(Timeout::default(), false);
                    return;
                }
            };

            exchanging.state = State::Confirming(Timeout::default(), bbox);
        }
        Lifecycle::Updated(timeout) => {
            if let ExchangeAmount::Specific(inner) = amount
                && timeout.current.is_multiple_of(TYPE_INTERVAL)
                && inner.index < inner.keys.len()
            {
                let key = inner.keys[inner.index];
                let next = ExchangeAmount::Specific(inner.increment_index());

                exchanging.amount = next;
                resources.input.send_key(key);
            }

            exchanging.state = State::Exchanging(timeout, bbox);
        }
    }
}
fn update_confirming(resources: &Resources, exchanging: &mut ExchangingBooster) {
    let State::Confirming(timeout, bbox) = exchanging.state else {
        panic!("exchanging booster state is not confirming")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) => {
            let (x, y) = bbox_click_point(bbox);
            resources.input.send_mouse(x, y, MouseKind::Click);
            exchanging.success = true;
            exchanging.state = State::Confirming(timeout, bbox);
        }
        Lifecycle::Ended => exchanging.state = State::Completing(Timeout::default(), false),
        Lifecycle::Updated(timeout) => exchanging.state = State::Confirming(timeout, bbox),
    }
}

fn update_completing(resources: &Resources, exchanging: &mut ExchangingBooster) {
    let State::Completing(timeout, completed) = exchanging.state else {
        panic!("exchanging booster state is not completing")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) | Lifecycle::Updated(timeout) => {
            exchanging.state = State::Completing(timeout, completed);
        }
        Lifecycle::Ended => {
            if resources.detector().detect_esc_settings() {
                resources.input.send_key(KeyKind::Esc);
            }
            exchanging.state = State::Completing(timeout, true);
        }
    }
}

#[inline]
fn bbox_click_point(bbox: Rect) -> (i32, i32) {
    let x = bbox.x + bbox.width / 2;
    let y = bbox.y + bbox.height / 2;
    (x, y)
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use anyhow::anyhow;
    use mockall::{Sequence, predicate::eq};
    use opencv::core::Rect;

    use super::*;
    use crate::{
        bridge::{KeyKind, MockInput, MouseKind},
        detect::MockDetector,
        ecs::Resources,
        player::timeout::Timeout,
    };

    fn rect(x: i32, y: i32) -> Rect {
        Rect {
            x,
            y,
            width: 10,
            height: 10,
        }
    }

    #[test]
    fn bbox_click_point_returns_center() {
        let bbox = rect(10, 20);
        let (x, y) = bbox_click_point(bbox);
        assert_eq!((x, y), (15, 25));
    }

    #[test]
    fn update_open_hexa_menu_starts_and_clicks_menu() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_hexa_quick_menu()
            .returning(|| Ok(rect(10, 10)));
        let mut input = MockInput::default();
        input
            .expect_send_mouse()
            .with(eq(15), eq(15), eq(MouseKind::Click))
            .once();

        let resources = Resources::new(Some(input), Some(detector));
        let mut exchanging = ExchangingBooster::new(1, false);
        exchanging.state = State::OpenHexaMenu(Timeout::default());

        update_open_hexa_menu(&resources, &mut exchanging);
        assert_matches!(exchanging.state, State::OpenHexaMenu(_));
    }

    #[test]
    fn update_open_hexa_menu_ends_and_opens_exchanging_menu() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_hexa_erda_conversion_button()
            .once()
            .returning(|| Ok(rect(30, 40)));
        let resources = Resources::new(None, Some(detector));

        let mut exchanging = ExchangingBooster::new(1, false);
        exchanging.state = State::OpenHexaMenu(Timeout {
            current: 20,
            started: true,
            ..Default::default()
        });

        update_open_hexa_menu(&resources, &mut exchanging);
        assert_matches!(exchanging.state, State::OpenExchangingMenu(_, _));
    }

    #[test]
    fn update_open_hexa_menu_fails_when_no_erda_button() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_hexa_erda_conversion_button()
            .once()
            .returning(|| Err(anyhow!("error")));
        let resources = Resources::new(None, Some(detector));

        let mut exchanging = ExchangingBooster::new(1, false);
        exchanging.state = State::OpenHexaMenu(Timeout {
            current: 20,
            started: true,
            ..Default::default()
        });

        update_open_hexa_menu(&resources, &mut exchanging);
        assert_matches!(exchanging.state, State::Completing(_, false));
    }

    #[test]
    fn update_open_exchanging_menu_starts_and_clicks() {
        let mut input = MockInput::default();
        input
            .expect_send_mouse()
            .with(eq(15), eq(25), eq(MouseKind::Click))
            .once();
        let resources = Resources::new(Some(input), None);

        let mut exchanging = ExchangingBooster::new(1, false);
        exchanging.state = State::OpenExchangingMenu(Timeout::default(), rect(10, 20));

        update_open_exchanging_menu(&resources, &mut exchanging);
        assert_matches!(exchanging.state, State::OpenExchangingMenu(_, _));
    }

    #[test]
    fn update_open_exchanging_menu_ends_and_opens_booster_menu() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_hexa_booster_button()
            .once()
            .returning(|| Ok(rect(40, 50)));
        let resources = Resources::new(None, Some(detector));

        let mut exchanging = ExchangingBooster::new(1, false);
        exchanging.state = State::OpenExchangingMenu(
            Timeout {
                current: 20,
                started: true,
                ..Default::default()
            },
            rect(10, 20),
        );

        update_open_exchanging_menu(&resources, &mut exchanging);
        assert_matches!(exchanging.state, State::OpenBoosterMenu(_, _));
    }

    #[test]
    fn update_exchanging_types_full_input_sequence() {
        let mut sequence = Sequence::new();
        let expected_keys = vec![KeyKind::Backspace, KeyKind::Backspace, KeyKind::Five];
        let mut input = MockInput::default();
        for key in &expected_keys {
            input
                .expect_send_key()
                .with(eq(*key))
                .once()
                .in_sequence(&mut sequence);
        }

        let resources = Resources::new(Some(input), None);
        let mut exchanging = ExchangingBooster::new(5, false);
        for i in 1..=expected_keys.len() {
            let timeout = Timeout {
                current: (i as u32) * 10 - 1,
                started: true,
                ..Default::default()
            };
            exchanging.state = State::Exchanging(timeout, rect(10, 10));
            update_exchanging(&resources, &mut exchanging);
        }
        let index = match exchanging.amount {
            ExchangeAmount::All => unreachable!(),
            ExchangeAmount::Specific(specific) => specific.index,
        };

        assert_eq!(
            index,
            expected_keys.len(),
            "All input keys should have been typed"
        );
        assert_matches!(exchanging.state, State::Exchanging(_, _));
    }

    #[test]
    fn update_exchanging_no_amount_clicks_max() {
        let mut input = MockInput::default();
        input
            .expect_send_mouse()
            .with(eq(35), eq(15), eq(MouseKind::Click))
            .once();
        let resources = Resources::new(Some(input), None);

        let mut exchanging = ExchangingBooster::new(1, true); // all = true → no amount
        exchanging.state = State::Exchanging(Timeout::default(), rect(30, 10));

        update_exchanging(&resources, &mut exchanging);
        assert_matches!(exchanging.state, State::Exchanging(_, _));
    }

    #[test]
    fn update_exchanging_ends_and_opens_confirming() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_hexa_convert_button()
            .once()
            .returning(|| Ok(rect(100, 200)));
        let resources = Resources::new(None, Some(detector));

        let mut exchanging = ExchangingBooster::new(1, false);
        exchanging.state = State::Exchanging(
            Timeout {
                current: 60,
                started: true,
                ..Default::default()
            },
            rect(10, 10),
        );

        update_exchanging(&resources, &mut exchanging);
        assert_matches!(exchanging.state, State::Confirming(_, _));
    }

    #[test]
    fn update_confirming_starts_and_clicks() {
        let mut input = MockInput::default();
        input
            .expect_send_mouse()
            .with(eq(15), eq(25), eq(MouseKind::Click))
            .once();
        let resources = Resources::new(Some(input), None);

        let mut exchanging = ExchangingBooster::new(1, false);
        exchanging.state = State::Confirming(Timeout::default(), rect(10, 20));

        update_confirming(&resources, &mut exchanging);
        assert_matches!(exchanging.state, State::Confirming(_, _));
    }

    #[test]
    fn update_confirming_ends_and_completes() {
        let resources = Resources::new(None, None);
        let mut exchanging = ExchangingBooster::new(1, false);
        exchanging.state = State::Confirming(
            Timeout {
                current: 20,
                started: true,
                ..Default::default()
            },
            rect(10, 20),
        );

        update_confirming(&resources, &mut exchanging);
        assert_matches!(exchanging.state, State::Completing(_, false));
    }

    #[test]
    fn update_completing_ends_and_sends_esc() {
        let mut detector = MockDetector::default();
        detector.expect_detect_esc_settings().returning(|| true);
        let mut input = MockInput::default();
        input.expect_send_key().with(eq(KeyKind::Esc)).once();

        let resources = Resources::new(Some(input), Some(detector));

        let mut exchanging = ExchangingBooster::new(1, false);
        exchanging.state = State::Completing(
            Timeout {
                current: 20,
                started: true,
                ..Default::default()
            },
            false,
        );

        update_completing(&resources, &mut exchanging);
        assert_matches!(exchanging.state, State::Completing(_, true));
    }

    #[test]
    fn update_completing_updates_without_esc() {
        let detector = MockDetector::default();
        let input = MockInput::default();
        let resources = Resources::new(Some(input), Some(detector));

        let mut exchanging = ExchangingBooster::new(1, false);
        exchanging.state = State::Completing(
            Timeout {
                current: 10,
                started: true,
                ..Default::default()
            },
            false,
        );

        update_completing(&resources, &mut exchanging);
        assert_matches!(exchanging.state, State::Completing(_, false));
    }
}
