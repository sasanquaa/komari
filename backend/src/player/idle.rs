use log::debug;
use opencv::core::Point;

use super::{
    AutoMob, Key, Move, Player, PlayerAction,
    actions::{next_action, update_from_ping_pong_action},
    double_jump::DoubleJumping,
    familiars_swap::FamiliarsSwapping,
    moving::{Moving, find_intermediate_points},
    panic::Panicking,
    use_key::UseKey,
};
use crate::{
    ActionKeyDirection, ActionKeyWith, Position,
    bridge::KeyKind,
    ecs::Resources,
    minimap::Minimap,
    player::{
        PlayerEntity, SolvingShape, exchange_booster::ExchangingBooster,
        solve_violetta::SolvingVioletta, unstuck::Unstucking, use_booster::UsingBooster,
    },
    rng::Rng,
};

/// Updates [`Player::Idle`] contextual state.
///
/// This state does not do much on its own except when auto mobbing. It acts as entry
/// to other state when there is an action and helps clearing keys.
pub fn update_idle_state(resources: &Resources, player: &mut PlayerEntity, minimap_state: Minimap) {
    player.context.last_destinations = None;
    player.context.last_movement = None;
    player.context.stalling_timeout_state = None;
    player.state = Player::Idle; // Sets initial next state first
    resources.input.send_key_up(KeyKind::Up);
    resources.input.send_key_up(KeyKind::Down);
    resources.input.send_key_up(KeyKind::Left);
    resources.input.send_key_up(KeyKind::Right);

    update_from_action(resources, player, minimap_state);
}

fn update_from_action(resources: &Resources, player: &mut PlayerEntity, minimap_state: Minimap) {
    let context = &mut player.context;
    let action = next_action(context);

    match action {
        Some(PlayerAction::AutoMob(AutoMob { position, .. })) => {
            let point = Point::new(position.x, position.y);
            let intermediates = if context.config.auto_mob_platforms_pathing {
                match minimap_state {
                    Minimap::Idle(idle) => find_intermediate_points(
                        &idle.platforms,
                        context.last_known_pos.unwrap(),
                        point,
                        position.allow_adjusting,
                        context.config.auto_mob_platforms_pathing_up_jump_only,
                        false,
                    ),
                    _ => unreachable!(),
                }
            } else {
                None
            };
            let next = match intermediates {
                Some(mut intermediates) => {
                    let (point, exact) = intermediates.next().unwrap();
                    Player::Moving(point, exact, Some(intermediates))
                }
                None => Player::Moving(point, position.allow_adjusting, None),
            };

            context.auto_mob_clear_pathing_task();
            context.last_destinations = intermediates
                .map(|intermediates| {
                    intermediates
                        .inner()
                        .into_iter()
                        .map(|(point, _, _)| point)
                        .collect::<Vec<_>>()
                })
                .or(Some(vec![point]));
            player.state = next;
        }

        Some(PlayerAction::Move(Move { position, .. })) => {
            let x = get_x_destination(&resources.rng, position);
            let point = Point::new(x, position.y);

            debug!(target: "backend/player", "handling move: {point:?}");
            player.state = Player::Moving(point, position.allow_adjusting, None);
        }

        Some(PlayerAction::Key(Key {
            position: Some(position),
            ..
        })) => {
            let x = get_x_destination(&resources.rng, position);
            let point = Point::new(x, position.y);

            debug!(target: "backend/player", "handling move: {point:?}");
            player.state = Player::Moving(point, position.allow_adjusting, None);
        }

        Some(PlayerAction::Key(
            key @ Key {
                position: None,
                with: ActionKeyWith::DoubleJump,
                direction,
                ..
            },
        )) => {
            let last_pos = context.last_known_pos.unwrap();
            let last_direction = context.last_known_direction;

            player.state =
                if matches!(direction, ActionKeyDirection::Any) || direction == last_direction {
                    Player::DoubleJumping(DoubleJumping::new(
                        Moving::new(last_pos, last_pos, false, None),
                        true,
                        true,
                    ))
                } else {
                    Player::UseKey(UseKey::from_key(key))
                };
        }

        Some(PlayerAction::Key(
            key @ Key {
                position: None,
                with: ActionKeyWith::Any | ActionKeyWith::Stationary,
                ..
            },
        )) => {
            player.state = Player::UseKey(UseKey::from_key(key));
        }

        Some(PlayerAction::SolveRune) => {
            let idle = match minimap_state {
                Minimap::Idle(idle) => idle,
                _ => {
                    player.context.clear_action_completed();
                    return;
                }
            };
            let rune = match idle.rune() {
                Some(rune) => rune,
                None => {
                    player.context.clear_action_completed();
                    return;
                }
            };

            context.last_destinations = Some(vec![rune]);
            if !context.config.rune_platforms_pathing {
                player.state = Player::Moving(rune, false, None);
                return;
            }
            if !context.is_stationary {
                return;
            }

            let intermediates = find_intermediate_points(
                &idle.platforms,
                context.last_known_pos.unwrap(),
                rune,
                true,
                context.config.rune_platforms_pathing_up_jump_only,
                true,
            );
            match intermediates {
                Some(mut intermediates) => {
                    context.last_destinations = Some(
                        intermediates
                            .inner()
                            .into_iter()
                            .map(|(point, _, _)| point)
                            .collect(),
                    );
                    let (point, exact) = intermediates.next().unwrap();
                    player.state = Player::Moving(point, exact, Some(intermediates));
                }
                None => {
                    player.state = Player::Moving(rune, false, None);
                }
            }
        }

        Some(PlayerAction::PingPong(ping_pong)) => {
            let last_pos = context.last_known_pos.unwrap();
            update_from_ping_pong_action(resources, player, minimap_state, ping_pong, last_pos)
        }

        Some(PlayerAction::FamiliarsSwap(swapping)) => {
            player.state = Player::FamiliarsSwapping(FamiliarsSwapping::new(
                swapping.swappable_slots,
                swapping.swappable_rarities,
            ));
        }

        Some(PlayerAction::Panic(panic)) => {
            player.state = Player::Panicking(Panicking::new(&resources.rng, panic.to));
        }

        Some(PlayerAction::UseBooster(using)) => {
            player.state = Player::UsingBooster(UsingBooster::new(using.kind));
        }

        Some(PlayerAction::ExchangeBooster(exchanging)) => {
            player.state = Player::ExchangingBooster(ExchangingBooster::new(
                exchanging.amount,
                exchanging.all,
            ));
        }

        Some(PlayerAction::Unstuck) => {
            player.state = Player::Unstucking(Unstucking::new_esc());
        }

        Some(PlayerAction::SolveShape) => {
            player.state = Player::SolvingShape(SolvingShape::default());
        }

        Some(PlayerAction::SolveVioletta) => {
            player.state = Player::SolvingVioletta(SolvingVioletta::default());
        }

        None => (),
    }
}

fn get_x_destination(rng: &Rng, position: Position) -> i32 {
    let x_min = position.x.saturating_sub(position.x_random_range).max(0);
    let x_max = position.x.saturating_add(position.x_random_range + 1);
    rng.random_range(x_min..x_max)
}
