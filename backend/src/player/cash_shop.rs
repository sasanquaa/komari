use log::info;

use super::{
    Player,
    timeout::{Lifecycle, Timeout, next_timeout_lifecycle},
};
use crate::{bridge::KeyKind, ecs::Resources, player::PlayerEntity};

#[derive(Clone, Copy, Debug)]
enum State {
    Entering,
    Entered(Timeout),
    Exitting,
    Exitted,
    Stalling(Timeout),
    Completed,
}

#[derive(Clone, Copy, Debug)]
pub struct CashShop {
    state: State,
}

impl CashShop {
    pub fn new() -> Self {
        Self {
            state: State::Entering,
        }
    }
}

pub fn update_cash_shop_state(
    resources: &Resources,
    player: &mut PlayerEntity,
    mut cash_shop: CashShop,
    failed_to_detect_player: bool,
) {
    let cash_shop_key = match player.context.config.cash_shop_key {
        Some(val) => val,
        None => {
            info!(target: "backend/player","aborted entering cash shop because cash shop key is not set");
            player.context.clear_action_completed();
            player.state = Player::Idle;
            return;
        }
    };

    match cash_shop.state {
        State::Entering => update_entering(resources, &mut cash_shop, cash_shop_key),
        State::Entered(timeout) => update_entered(&mut cash_shop, timeout),
        State::Exitting => update_exitting(resources, &mut cash_shop),
        State::Exitted => update_exitted(&mut cash_shop, failed_to_detect_player),
        State::Stalling(timeout) => update_stalling(&mut cash_shop, timeout),
        State::Completed => unreachable!(),
    }

    player.state = if matches!(cash_shop.state, State::Completed) {
        Player::Idle
    } else {
        Player::CashShopThenExit(cash_shop)
    };
}

fn update_exitted(cash_shop: &mut CashShop, failed_to_detect_player: bool) {
    cash_shop.state = if failed_to_detect_player {
        State::Exitted
    } else {
        State::Stalling(Timeout::default())
    };
}

fn update_entering(resources: &Resources, cash_shop: &mut CashShop, key: KeyKind) {
    resources.input.send_key(key);
    cash_shop.state = if resources.detector().detect_player_in_cash_shop() {
        State::Entered(Timeout::default())
    } else {
        State::Entering
    };
}

fn update_entered(cash_shop: &mut CashShop, timeout: Timeout) {
    // Exit after 10 secs
    match next_timeout_lifecycle(timeout, 305) {
        Lifecycle::Ended => cash_shop.state = State::Exitting,
        Lifecycle::Started(timeout) | Lifecycle::Updated(timeout) => {
            cash_shop.state = State::Entered(timeout)
        }
    }
}

fn update_exitting(resources: &Resources, cash_shop: &mut CashShop) {
    resources.input.send_key(KeyKind::Esc);
    resources.input.send_key(KeyKind::Enter);
    cash_shop.state = if resources.detector().detect_player_in_cash_shop() {
        State::Exitting
    } else {
        State::Exitted
    };
}

fn update_stalling(cash_shop: &mut CashShop, timeout: Timeout) {
    // Return after 3 secs
    match next_timeout_lifecycle(timeout, 90) {
        Lifecycle::Ended => cash_shop.state = State::Completed,
        Lifecycle::Started(timeout) | Lifecycle::Updated(timeout) => {
            cash_shop.state = State::Stalling(timeout);
        }
    }
}
