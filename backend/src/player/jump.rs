use super::{
    Player,
    moving::{MOVE_TIMEOUT, Moving},
    state::LastMovement,
    timeout::{ChangeAxis, MovingLifecycle, next_moving_lifecycle_with_axis},
};
use crate::{ecs::Resources, player::PlayerEntity};

const TIMEOUT: u32 = MOVE_TIMEOUT + 3;

pub fn update_jumping_state(resources: &Resources, player: &mut PlayerEntity, moving: Moving) {
    match next_moving_lifecycle_with_axis(
        moving,
        player.context.last_known_pos.expect("in positional state"),
        TIMEOUT,
        ChangeAxis::Vertical,
    ) {
        MovingLifecycle::Started(moving) => {
            resources.input.send_key(player.context.config.jump_key);
            player.context.last_movement = Some(LastMovement::Jumping);
            player.state = Player::Jumping(moving);
        }
        MovingLifecycle::Ended(moving) => {
            player.state = Player::Moving(moving.dest, moving.exact, moving.intermediates);
        }
        MovingLifecycle::Updated(moving) => {
            player.state = Player::Jumping(moving);
        }
    }
}
