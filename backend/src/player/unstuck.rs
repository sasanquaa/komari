use opencv::core::Point;

use super::timeout::{Lifecycle, Timeout, next_timeout_lifecycle};
use crate::{
    bridge::KeyKind,
    ecs::Resources,
    minimap::Minimap,
    player::{MOVE_TIMEOUT, Player, PlayerAction, PlayerEntity, next_action},
};

#[derive(Debug, Clone, Copy)]
enum UnstuckingKind {
    Esc,
    Movement { timeout: Timeout, random: bool },
}

#[derive(Debug, Clone, Copy)]
pub struct Unstucking {
    kind: UnstuckingKind,
}

impl Unstucking {
    pub fn new_esc() -> Self {
        Self {
            kind: UnstuckingKind::Esc,
        }
    }

    pub fn new_movement(timeout: Timeout, random: bool) -> Self {
        Self {
            kind: UnstuckingKind::Movement { timeout, random },
        }
    }

    fn movement(mut self, timeout: Timeout, random: bool) -> Unstucking {
        self.kind = UnstuckingKind::Movement { timeout, random };
        self
    }
}

/// A threshold to consider spamming falling action
///
/// This is when the player is inside the top edge of minimap. At least for higher level maps, this
/// seems rare but one possible map is The Forest Of Earth in Arcana.
const Y_IGNORE_THRESHOLD: i32 = 18;

/// Updates the [`Player::Unstucking`] contextual state
///
/// This state can only be transitioned to when [`PlayerState::unstuck_counter`] reached the fixed
/// threshold, when the player moved into the edges of the minimap or rotator detected an UI
/// element blocking the player.
/// If [`PlayerState::unstuck_consecutive_counter`] has not reached the threshold and the player
/// moved into the left/right/top edges of the minimap, it will try to move
/// out as appropriate. It will also try to press ESC key to exit any dialog.
///
/// Each initial transition to [`Player::Unstucking`] increases
/// the [`PlayerState::unstuck_consecutive_counter`] by one. If the threshold is reached, this
/// state will just jump in random direction.
pub fn update_unstucking_state(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
) {
    let Player::Unstucking(unstucking) = player.state else {
        panic!("state is not unstucking");
    };
    let Minimap::Idle(idle) = minimap_state else {
        player.state = Player::Detecting;
        return;
    };

    match unstucking.kind {
        UnstuckingKind::Esc => {
            if resources.detector().detect_esc_settings() {
                resources.input.send_key(KeyKind::Esc);
            }

            match next_action(&player.context) {
                Some(PlayerAction::Unstuck) => {
                    player.context.clear_action_completed();
                    player.state = Player::Detecting;
                }
                Some(_) | None => {
                    player.state = Player::Detecting;
                }
            }
        }
        UnstuckingKind::Movement { timeout, random } => {
            let context = &mut player.context;
            let pos = context
                .last_known_pos
                .map(|pos| Point::new(pos.x, idle.bbox.height - pos.y));
            let random = random || pos.is_none();

            match next_timeout_lifecycle(timeout, MOVE_TIMEOUT) {
                Lifecycle::Started(timeout) => {
                    let to_right = match (random, pos) {
                        (true, _) => resources.rng.random_bool(0.5),
                        (_, Some(Point { y, .. })) if y <= Y_IGNORE_THRESHOLD => {
                            player.state = Player::Unstucking(unstucking.movement(timeout, random));
                            return;
                        }
                        (_, Some(Point { x, .. })) => x <= idle.bbox.width / 2,
                        (_, None) => unreachable!(),
                    };
                    if to_right {
                        resources.input.send_key_down(KeyKind::Right);
                    } else {
                        resources.input.send_key_up(KeyKind::Left);
                    }

                    player.state = Player::Unstucking(unstucking.movement(timeout, random));
                }
                Lifecycle::Ended => {
                    resources.input.send_key_up(KeyKind::Right);
                    resources.input.send_key_up(KeyKind::Left);
                    player.state = Player::Detecting;
                }
                Lifecycle::Updated(timeout) => {
                    let send_space = match (random, pos) {
                        (true, _) => true,
                        (_, Some(pos)) if pos.y > Y_IGNORE_THRESHOLD => true,
                        _ => false,
                    };
                    if send_space {
                        resources.input.send_key(context.config.jump_key);
                    }

                    player.state = Player::Unstucking(unstucking.movement(timeout, random));
                }
            }
        }
    }
}
