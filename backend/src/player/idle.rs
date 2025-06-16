use log::debug;
use opencv::core::Point;
use platforms::windows::KeyKind;

use super::{
    Player, PlayerAction, PlayerActionAutoMob, PlayerActionKey, PlayerActionMove, PlayerState,
    actions::{PlayerActionPingPong, on_action_state_mut, on_ping_pong_double_jump_action},
    double_jump::DoubleJumping,
    familiars_swap::FamiliarsSwapping,
    moving::{Moving, find_intermediate_points},
    panic::Panicking,
    use_key::UseKey,
};
use crate::{
    ActionKeyDirection, ActionKeyWith, Position, context::Context, minimap::Minimap, rng::Rng,
};

/// Updates [`Player::Idle`] contextual state.
///
/// This state does not do much on its own except when auto mobbing. It acts as entry
/// to other state when there is an action and helps clearing keys.
pub fn update_idle_context(context: &Context, state: &mut PlayerState) -> Player {
    state.last_destinations = None;
    state.last_movement = None;
    state.stalling_timeout_state = None;
    let _ = context.keys.send_up(KeyKind::Up);
    let _ = context.keys.send_up(KeyKind::Down);
    let _ = context.keys.send_up(KeyKind::Left);
    let _ = context.keys.send_up(KeyKind::Right);

    on_action_state_mut(
        state,
        |state, action| on_player_action(context, state, action),
        || Player::Idle,
    )
}

fn on_player_action(
    context: &Context,
    state: &mut PlayerState,
    action: PlayerAction,
) -> Option<(Player, bool)> {
    let cur_pos = state.last_known_pos.unwrap();
    match action {
        PlayerAction::AutoMob(PlayerActionAutoMob { position, .. }) => {
            let point = Point::new(position.x, position.y);
            let intermediates = if state.config.auto_mob_platforms_pathing {
                match context.minimap {
                    Minimap::Idle(idle) => find_intermediate_points(
                        &idle.platforms,
                        state.last_known_pos.unwrap(),
                        point,
                        position.allow_adjusting,
                        state.config.auto_mob_platforms_pathing_up_jump_only,
                        false,
                    ),
                    _ => unreachable!(),
                }
            } else {
                None
            };
            let next = intermediates
                .map(|mut intermediates| {
                    let (point, exact) = intermediates.next().unwrap();
                    Player::Moving(point, exact, Some(intermediates))
                })
                .unwrap_or(Player::Moving(point, position.allow_adjusting, None));

            state.last_destinations = intermediates
                .map(|intermediates| {
                    intermediates
                        .inner()
                        .into_iter()
                        .map(|(point, _, _)| point)
                        .collect::<Vec<_>>()
                })
                .or(Some(vec![point]));
            Some((next, false))
        }
        PlayerAction::Move(PlayerActionMove { position, .. }) => {
            let x = get_x_destination(&context.rng, position);
            debug!(target: "player", "handling move: {} {}", x, position.y);
            Some((
                Player::Moving(Point::new(x, position.y), position.allow_adjusting, None),
                false,
            ))
        }
        PlayerAction::Key(PlayerActionKey {
            position: Some(position),
            ..
        }) => {
            let x = get_x_destination(&context.rng, position);
            debug!(target: "player", "handling move: {} {}", x, position.y);
            Some((
                Player::Moving(Point::new(x, position.y), position.allow_adjusting, None),
                false,
            ))
        }
        PlayerAction::Key(PlayerActionKey {
            position: None,
            with: ActionKeyWith::DoubleJump,
            direction,
            ..
        }) => {
            if matches!(direction, ActionKeyDirection::Any)
                || direction == state.last_known_direction
            {
                Some((
                    Player::DoubleJumping(DoubleJumping::new(
                        Moving::new(cur_pos, cur_pos, false, None),
                        true,
                        true,
                    )),
                    false,
                ))
            } else {
                Some((Player::UseKey(UseKey::from_action(action)), false))
            }
        }
        PlayerAction::Key(PlayerActionKey {
            position: None,
            with: ActionKeyWith::Any | ActionKeyWith::Stationary,
            ..
        }) => Some((Player::UseKey(UseKey::from_action(action)), false)),
        PlayerAction::SolveRune => {
            if let Minimap::Idle(idle) = context.minimap
                && let Some(rune) = idle.rune.value().copied()
            {
                if state.config.rune_platforms_pathing {
                    if !state.is_stationary {
                        return Some((Player::Idle, false));
                    }
                    let intermediates = find_intermediate_points(
                        &idle.platforms,
                        cur_pos,
                        rune,
                        true,
                        state.config.rune_platforms_pathing_up_jump_only,
                        true,
                    );
                    if let Some(mut intermediates) = intermediates {
                        state.last_destinations = Some(
                            intermediates
                                .inner()
                                .into_iter()
                                .map(|(point, _, _)| point)
                                .collect(),
                        );
                        let (point, exact) = intermediates.next().unwrap();
                        return Some((Player::Moving(point, exact, Some(intermediates)), false));
                    }
                }
                state.last_destinations = Some(vec![rune]);
                return Some((Player::Moving(rune, true, None), false));
            }
            Some((Player::Idle, true))
        }
        PlayerAction::PingPong(PlayerActionPingPong {
            bound, direction, ..
        }) => Some(on_ping_pong_double_jump_action(
            context, cur_pos, bound, direction,
        )),
        PlayerAction::FamiliarsSwapping(swapping) => Some((
            Player::FamiliarsSwapping(FamiliarsSwapping::new(
                swapping.swappable_slots,
                swapping.swappable_rarities,
            )),
            false,
        )),
        PlayerAction::Panic(panic) => Some((Player::Panicking(Panicking::new(panic.to)), false)),
    }
}

fn get_x_destination(rng: &Rng, position: Position) -> i32 {
    let x_min = position.x.saturating_sub(position.x_random_range).max(0);
    let x_max = position.x.saturating_add(position.x_random_range + 1);
    rng.random_range(x_min..x_max)
}
