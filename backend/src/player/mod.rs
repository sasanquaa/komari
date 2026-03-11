use actions::next_action;
use adjust::{Adjusting, update_adjusting_state};
use cash_shop::{CashShop, update_cash_shop_state};
use double_jump::{DoubleJumping, update_double_jumping_state};
use fall::update_falling_state;
use familiars_swap::{FamiliarsSwapping, update_familiars_swapping_state};
use grapple::update_grappling_state;
use idle::update_idle_state;
use jump::update_jumping_state;
use moving::{MOVE_TIMEOUT, Moving, MovingIntermediates, update_moving_state};
use opencv::core::Point;
use panic::update_panicking_state;
use solve_rune::{SolvingRune, update_solving_rune_state};
use stall::update_stalling_state;
use state::LastMovement;
use strum::Display;
use timeout::Timeout;
use unstuck::update_unstucking_state;
use up_jump::{UpJumping, update_up_jumping_state};
use use_key::{UseKey, update_use_key_state};

use crate::{
    bridge::KeyKind,
    buff::BuffEntities,
    ecs::Resources,
    minimap::{Minimap, MinimapEntity},
    models::ActionKeyDirection,
    player::{
        exchange_booster::{ExchangingBooster, update_exchanging_booster_state},
        fall::Falling,
        grapple::Grappling,
        solve_shape::{SolvingShape, update_solving_shape_state},
        solve_violetta::{SolvingVioletta, update_solving_violetta_state},
        unstuck::Unstucking,
        use_booster::{UsingBooster, update_using_booster_state},
    },
};

mod actions;
mod adjust;
mod cash_shop;
mod double_jump;
mod exchange_booster;
mod fall;
mod familiars_swap;
mod grapple;
mod idle;
mod jump;
mod moving;
mod panic;
mod solve_rune;
mod solve_shape;
mod solve_violetta;
mod stall;
mod state;
mod timeout;
mod unstuck;
mod up_jump;
mod use_booster;
mod use_key;

pub use actions::*;
pub use {
    double_jump::DOUBLE_JUMP_THRESHOLD, grapple::GRAPPLING_MAX_THRESHOLD,
    grapple::GRAPPLING_THRESHOLD, panic::Panicking, state::PlayerContext, state::Quadrant,
};

/// Minimum y distance from the destination required to perform a jump.
pub const JUMP_THRESHOLD: i32 = 7;

#[derive(Debug)]
pub struct PlayerEntity {
    pub state: Player,
    pub context: PlayerContext,
}

/// The player contextual states.
#[derive(Clone, Debug, Display)]
#[allow(clippy::large_enum_variant)] // There is only ever a single instance of Player
pub enum Player {
    /// Detects player on the minimap.
    Detecting,
    /// Does nothing state.
    ///
    /// Acts as entry to other state when there is a [`PlayerAction`].
    Idle,
    /// Uses key.
    UseKey(UseKey),
    /// Movement-related coordinator state.
    Moving(Point, bool, Option<MovingIntermediates>),
    /// Performs walk or small adjustment x-wise action.
    Adjusting(Adjusting),
    /// Performs double jump action.
    DoubleJumping(DoubleJumping),
    /// Performs a grappling action.
    Grappling(Grappling),
    /// Performs a normal jump.
    Jumping(Moving),
    /// Performs an up jump action.
    UpJumping(UpJumping),
    Falling(Falling),
    /// Unstucks when inside non-detecting position or because of [`PlayerState::unstuck_counter`].
    Unstucking(Unstucking),
    /// Stalls for time and return to [`Player::Idle`] or [`PlayerState::stalling_timeout_state`].
    Stalling(Timeout, u32),
    /// Tries to solve a rune.
    SolvingRune(SolvingRune),
    /// Tries to solve lie detector's transparent shape.
    SolvingShape(SolvingShape),
    SolvingVioletta(SolvingVioletta),
    /// Enters the cash shop then exit after 10 seconds.
    CashShopThenExit(CashShop),
    #[strum(to_string = "FamiliarsSwapping({0})")]
    FamiliarsSwapping(FamiliarsSwapping),
    Panicking(Panicking),
    UsingBooster(UsingBooster),
    ExchangingBooster(ExchangingBooster),
}

impl Player {
    #[inline]
    pub fn can_override_current_state(&self, cur_pos: Option<Point>) -> bool {
        const OVERRIDABLE_DISTANCE: i32 = DOUBLE_JUMP_THRESHOLD / 2;

        match self {
            Player::Detecting | Player::Idle => true,
            Player::Moving(dest, _, _) => {
                if let Some(pos) = cur_pos {
                    (dest.x - pos.x).abs() >= OVERRIDABLE_DISTANCE
                } else {
                    true
                }
            }
            Player::DoubleJumping(DoubleJumping {
                moving,
                forced: false,
                ..
            })
            | Player::Adjusting(Adjusting { moving, .. }) => {
                let (distance, _) =
                    moving.x_distance_direction_from(true, cur_pos.unwrap_or(moving.pos));
                distance >= OVERRIDABLE_DISTANCE
            }
            Player::Grappling(Grappling { moving, .. })
            | Player::Jumping(moving)
            | Player::UpJumping(UpJumping { moving, .. })
            | Player::Falling(Falling { moving, .. }) => moving.completed,
            Player::SolvingRune(_)
            | Player::CashShopThenExit(_)
            | Player::Unstucking(_)
            | Player::DoubleJumping(DoubleJumping { forced: true, .. })
            | Player::UseKey(_)
            | Player::FamiliarsSwapping(_)
            | Player::Panicking(_)
            | Player::UsingBooster(_)
            | Player::ExchangingBooster(_)
            | Player::SolvingShape(_)
            | Player::SolvingVioletta(_)
            | Player::Stalling(_, _) => false,
        }
    }
}

pub fn run_system(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap: &MinimapEntity,
    buffs: &BuffEntities,
) {
    if player.context.rune_cash_shop {
        resources.input.send_key_up(KeyKind::Up);
        resources.input.send_key_up(KeyKind::Down);
        resources.input.send_key_up(KeyKind::Left);
        resources.input.send_key_up(KeyKind::Right);
        player.context.rune_cash_shop = false;
        player.context.reset_to_idle_next_update = false;
        player.state = Player::CashShopThenExit(CashShop::new());
        return;
    }

    let did_update =
        player
            .context
            .update_state(resources, player.state.clone(), minimap.state, buffs);
    if !did_update && !resources.operation.halting() {
        // When the player detection fails, the possible causes are:
        // - Player moved inside the edges of the minimap
        // - Other UIs overlapping the minimap
        //
        // `update_non_positional_context` is here to continue updating
        // `Player::Unstucking` returned from below when the player
        // is inside the edges of the minimap. And also `Player::CashShopThenExit`.
        if update_non_positional_state(resources, player, minimap.state, true) {
            return;
        }

        let is_stucking = match minimap.state {
            Minimap::Detecting => false,
            Minimap::Idle(idle) => !idle.partially_overlapping,
        };
        if is_stucking {
            let unstucking = Unstucking::new_movement(
                Timeout::default(),
                player.context.track_unstucking_transitioned(),
            );
            player.state = Player::Unstucking(unstucking);
            player.context.last_known_direction = ActionKeyDirection::Any;
            return;
        }

        player.state = Player::Detecting;
        return;
    }

    if player.context.reset_to_idle_next_update {
        player.context.reset_to_idle_next_update = false;
        player.state = Player::Idle;
    }
    if player.context.reset_stalling_buffer_states_next_update {
        player.context.reset_stalling_buffer_states_next_update = false;
        player.context.clear_stalling_buffer_states(resources);
    }

    if !update_non_positional_state(resources, player, minimap.state, false) {
        update_positional_state(resources, player, minimap.state);
    }
}

/// Updates the contextual state that does not require the player current position.
///
/// Returns `true` if state is updated.
#[inline]
fn update_non_positional_state(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
    failed_to_detect_player: bool,
) -> bool {
    match player.state {
        Player::UseKey(_) => update_use_key_state(resources, player, minimap_state),
        Player::FamiliarsSwapping(_) => {
            update_familiars_swapping_state(resources, player);
        }
        Player::Unstucking(_) => {
            update_unstucking_state(resources, player, minimap_state);
        }
        Player::Stalling(timeout, max_timeout) => {
            if failed_to_detect_player {
                return false;
            }

            update_stalling_state(player, timeout, max_timeout);
        }
        Player::SolvingRune(_) => {
            if failed_to_detect_player {
                return false;
            }

            update_solving_rune_state(resources, player);
        }
        Player::SolvingShape(_) => update_solving_shape_state(resources, player),
        Player::SolvingVioletta(_) => update_solving_violetta_state(resources, player),
        Player::CashShopThenExit(cash_shop) => {
            update_cash_shop_state(resources, player, cash_shop, failed_to_detect_player);
        }
        Player::Panicking(panicking) => {
            update_panicking_state(resources, player, minimap_state, panicking);
        }
        Player::UsingBooster(_) => update_using_booster_state(resources, player),
        Player::ExchangingBooster(_) => update_exchanging_booster_state(resources, player),
        Player::Detecting
        | Player::Idle
        | Player::Moving(_, _, _)
        | Player::Adjusting(_)
        | Player::DoubleJumping(_)
        | Player::Grappling(_)
        | Player::Jumping(_)
        | Player::UpJumping(_)
        | Player::Falling(_) => return false,
    }

    true
}

/// Updates the contextual state that requires the player current position.
#[inline]
fn update_positional_state(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
) {
    match player.state {
        Player::Detecting => player.state = Player::Idle,
        Player::Idle => update_idle_state(resources, player, minimap_state),
        Player::Moving(_, _, _) => update_moving_state(resources, player, minimap_state),
        Player::Adjusting(_) => update_adjusting_state(resources, player, minimap_state),
        Player::DoubleJumping(_) => update_double_jumping_state(resources, player, minimap_state),
        Player::Grappling(_) => update_grappling_state(resources, player, minimap_state),
        Player::UpJumping(_) => update_up_jumping_state(resources, player, minimap_state),
        Player::Jumping(moving) => update_jumping_state(resources, player, moving),
        Player::Falling(Falling { .. }) => update_falling_state(resources, player, minimap_state),
        Player::UseKey(_)
        | Player::Unstucking(_)
        | Player::Stalling(_, _)
        | Player::SolvingRune(_)
        | Player::FamiliarsSwapping(_)
        | Player::Panicking(_)
        | Player::UsingBooster(_)
        | Player::ExchangingBooster(_)
        | Player::SolvingShape(_)
        | Player::SolvingVioletta(_)
        | Player::CashShopThenExit(_) => unreachable!(),
    }
}
