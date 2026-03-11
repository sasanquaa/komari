use std::ops::Range;

use log::{debug, info};
use opencv::core::Point;

use super::{
    GRAPPLING_MAX_THRESHOLD, JUMP_THRESHOLD, Player, PlayerContext,
    actions::{Key, Move, PlayerAction},
    double_jump::{DOUBLE_JUMP_THRESHOLD, DoubleJumping},
    state::LastMovement,
    timeout::Timeout,
    up_jump::UpJumping,
};
use crate::{
    ActionKeyDirection, ActionKeyWith, MAX_PLATFORMS_COUNT,
    array::Array,
    bridge::KeyKind,
    ecs::Resources,
    minimap::Minimap,
    pathing::{MovementHint, PlatformWithNeighbors, find_points_with},
    player::{
        Falling, PlayerEntity,
        adjust::{ADJUSTING_MEDIUM_THRESHOLD, ADJUSTING_SHORT_THRESHOLD, Adjusting},
        grapple::{GRAPPLING_THRESHOLD, Grappling},
        next_action,
        solve_rune::SolvingRune,
        unstuck::Unstucking,
        use_key::UseKey,
    },
};

/// Maximum amount of ticks a change in x or y direction must be detected.
pub const MOVE_TIMEOUT: u32 = 5;

/// Jumpable y distances.
const JUMPABLE_RANGE: Range<i32> = 4..JUMP_THRESHOLD;
const UP_JUMP_THRESHOLD: i32 = JUMP_THRESHOLD;

/// Intermediate points to move by.
///
/// The last point is the destination.
#[derive(Clone, Copy, Debug)]
pub struct MovingIntermediates {
    current: usize,
    inner: Array<(Point, MovementHint, bool), 16>,
}

impl MovingIntermediates {
    #[inline]
    pub fn inner(&self) -> Array<(Point, MovementHint, bool), 16> {
        self.inner
    }

    #[inline]
    pub fn has_next(&self) -> bool {
        self.current < self.inner.len()
    }

    #[inline]
    pub fn next(&mut self) -> Option<(Point, bool)> {
        if self.current >= self.inner.len() {
            return None;
        }
        let next = self.inner[self.current];
        self.current += 1;
        Some((next.0, next.2))
    }
}

/// A contextual state that stores moving-related data.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Default))]
pub struct Moving {
    /// The player's previous position.
    ///
    /// It will be updated to current position after calling [`next_moving_lifecycle_with_axis`].
    /// Before calling this function, it will always be the previous position in relative to
    /// [`PlayerState::last_known_pos`].
    pub pos: Point,
    /// The destination the player is moving to.
    ///
    /// When [`Self::intermediates`] is [`Some`], this could be an intermediate destination.
    pub dest: Point,
    /// Whether to allow adjusting to precise destination.
    pub exact: bool,
    /// Whether the movement has completed.
    ///
    /// For example, in up jump with fixed key like Corsair, it is considered complete
    /// when the key is pressed.
    pub completed: bool,
    /// Current timeout ticks for checking if the player position's changed.
    pub timeout: Timeout,
    /// Intermediate points to move to before reaching the destination.
    ///
    /// When [`Some`], the last point is the destination.
    pub intermediates: Option<MovingIntermediates>,
}

/// Convenient implementations
impl Moving {
    #[inline]
    pub fn new(
        pos: Point,
        dest: Point,
        exact: bool,
        intermediates: Option<MovingIntermediates>,
    ) -> Self {
        Self {
            pos,
            dest,
            exact,
            completed: false,
            timeout: Timeout::default(),
            intermediates,
        }
    }

    #[inline]
    pub fn completed(mut self, completed: bool) -> Moving {
        self.completed = completed;
        self
    }

    #[inline]
    pub fn timeout(mut self, timeout: Timeout) -> Moving {
        self.timeout = timeout;
        self
    }

    #[inline]
    pub fn timeout_current(mut self, current: u32) -> Moving {
        self.timeout.current = current;
        self
    }

    #[inline]
    pub fn timeout_started(mut self, started: bool) -> Moving {
        self.timeout = self.timeout.started(started);
        self
    }

    #[inline]
    fn intermediate_hint(&self) -> Option<MovementHint> {
        self.intermediates
            .map(|intermediates| intermediates.inner[intermediates.current.saturating_sub(1)].1)
    }

    /// Computes the x distance and direction between [`Self::dest`] and `cur_pos`.
    ///
    /// If `current_destination` is false, it will use the last destination if
    /// [`Self::intermediates`] is [`Some`].
    ///
    /// Returns the distance and direction values pair computed from `dest - cur_pos`.
    #[inline]
    pub fn x_distance_direction_from(
        &self,
        current_destination: bool,
        cur_pos: Point,
    ) -> (i32, i32) {
        self.distance_direction_from(true, current_destination, cur_pos)
    }

    /// Computes the y distance and direction between [`Self::dest`] and `cur_pos`.
    ///
    /// If `current_destination` is false, it will use the last destination if
    /// [`Self::intermediates`] is [`Some`].
    ///
    /// Returns the distance and direction values pair computed from `dest - cur_pos`.
    #[inline]
    pub fn y_distance_direction_from(
        &self,
        current_destination: bool,
        cur_pos: Point,
    ) -> (i32, i32) {
        self.distance_direction_from(false, current_destination, cur_pos)
    }

    #[inline]
    fn distance_direction_from(
        &self,
        compute_x: bool,
        current_destination: bool,
        cur_pos: Point,
    ) -> (i32, i32) {
        let dest = if current_destination {
            self.dest
        } else {
            self.last_destination()
        };
        let direction = if compute_x {
            dest.x - cur_pos.x
        } else {
            dest.y - cur_pos.y
        };
        let distance = direction.abs();
        (distance, direction)
    }

    #[inline]
    fn last_destination(&self) -> Point {
        if self.is_destination_intermediate() {
            let points = self.intermediates.unwrap().inner;
            points[points.len() - 1].0
        } else {
            self.dest
        }
    }

    #[inline]
    pub fn is_destination_intermediate(&self) -> bool {
        self.intermediates
            .is_some_and(|intermediates| intermediates.has_next())
    }

    /// Determines whether auto mobbing intermediate destination can be skipped.
    #[inline]
    pub fn auto_mob_can_skip_current_destination(&self, context: &PlayerContext) -> bool {
        if !context.has_auto_mob_action_only() {
            return false;
        }

        let Some(intermediates) = self.intermediates else {
            return false;
        };
        if !intermediates.has_next() {
            return false;
        }

        let pos = context.last_known_pos.expect("in positional context");
        let (x_distance, _) = self.x_distance_direction_from(true, pos);
        let (y_distance, y_direction) = self.y_distance_direction_from(true, pos);

        let did_fall_down =
            matches!(context.last_movement, Some(LastMovement::Falling)) && y_direction >= 0;
        let did_up_jump =
            matches!(context.last_movement, Some(LastMovement::UpJumping)) && y_direction <= 0;
        let y_within_jump = y_distance < JUMP_THRESHOLD;

        let can_skip_y = did_fall_down || did_up_jump || y_within_jump;
        let can_skip_x = x_distance < DOUBLE_JUMP_THRESHOLD;

        can_skip_x && can_skip_y
    }
}

/// Updates the [`Player::Moving`] contextual state.
///
/// This state does not perform any movement but acts as coordinator
/// for other movement states. It keeps track of [`PlayerState::unstuck_counter`], avoids
/// state looping and advancing `intermediates` when the current destination is reached.
///
/// It will first transition to [`Player::DoubleJumping`] and [`Player::Adjusting`] for
/// matching `x` of `dest`. Then, [`Player::Grappling`], [`Player::UpJumping`], [`Player::Jumping`]
/// or [`Player::Falling`] for matching `y` of `dest`. (e.g. horizontal then vertical)
///
/// In auto mob or intermediate destination, most of the movement thresholds are relaxed for
/// more fluid movement.
pub fn update_moving_state(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
) {
    let Player::Moving(dest, exact, intermediates) = player.state else {
        panic!("state is not moving")
    };
    let context = &mut player.context;

    player.state = Player::Idle; // Sets initial next state first
    if context.track_unstucking() {
        player.state = Player::Unstucking(Unstucking::new_movement(
            Timeout::default(),
            context.track_unstucking_transitioned(),
        ));
        return;
    }

    let cur_pos = context.last_known_pos.unwrap();
    let moving = Moving::new(cur_pos, dest, exact, intermediates);
    let is_intermediate = moving.is_destination_intermediate();
    let skip_destination = moving.auto_mob_can_skip_current_destination(context);

    let (x_distance, _) = moving.x_distance_direction_from(true, cur_pos);
    let (y_distance, y_direction) = moving.y_distance_direction_from(true, cur_pos);

    let disable_double_jumping = context.config.disable_double_jumping;
    let disable_adjusting = context.config.disable_adjusting;

    // Check to double jump
    if !skip_destination
        && !disable_double_jumping
        && x_distance >= context.double_jump_threshold(is_intermediate)
    {
        let require_stationary = context.has_ping_pong_action_only()
            && !matches!(
                context.last_movement,
                Some(LastMovement::Grappling | LastMovement::UpJumping)
            );
        return abort_action_on_state_repeat(
            player,
            Player::DoubleJumping(DoubleJumping::new(moving, false, require_stationary)),
            minimap_state,
        );
    }

    // Check to adjust and allow disabling adjusting only if `exact` is false
    if !skip_destination
        && ((!disable_adjusting && x_distance >= ADJUSTING_MEDIUM_THRESHOLD)
            || (exact && x_distance >= ADJUSTING_SHORT_THRESHOLD))
    {
        return abort_action_on_state_repeat(
            player,
            Player::Adjusting(Adjusting::new(moving)),
            minimap_state,
        );
    }

    // Check to grapple
    let has_teleport_key = context.config.teleport_key.is_some();
    if !skip_destination
        && y_direction > 0
        && ((!has_teleport_key && y_distance >= GRAPPLING_THRESHOLD)
            || (has_teleport_key && y_distance >= GRAPPLING_MAX_THRESHOLD))
        && !context.should_disable_grappling()
    {
        return abort_action_on_state_repeat(
            player,
            Player::Grappling(Grappling::new(moving)),
            minimap_state,
        );
    }

    // Check to up jump
    if !skip_destination && y_direction > 0 && y_distance >= UP_JUMP_THRESHOLD {
        // In auto mob with platforms pathing and up jump only, immediately aborts the action
        // if there are no intermediate points and the distance is too big to up jump.
        if context.has_auto_mob_action_only()
            && context.config.auto_mob_platforms_pathing
            && context.config.auto_mob_platforms_pathing_up_jump_only
            && intermediates.is_none()
            && y_distance >= GRAPPLING_THRESHOLD
        {
            debug!(target:"backend/player","auto mob aborted because distance for up jump only is too big");
            context.clear_action_completed();
            player.state = Player::Idle;
            return;
        }

        let next_state = Player::UpJumping(UpJumping::new(moving, resources, context));
        return abort_action_on_state_repeat(player, next_state, minimap_state);
    }

    // Check to jump
    if !skip_destination && y_direction > 0 && JUMPABLE_RANGE.contains(&y_distance) {
        return abort_action_on_state_repeat(player, Player::Jumping(moving), minimap_state);
    }

    // Check to fall
    if !skip_destination
        && y_direction < 0
        && y_distance >= context.falling_threshold(is_intermediate)
    {
        return abort_action_on_state_repeat(
            player,
            Player::Falling(Falling::new(moving, cur_pos, false)),
            minimap_state,
        );
    }

    debug!(target: "backend/player", "reached {dest:?} with actual position {cur_pos:?}");
    if let Some(mut intermediates) = intermediates
        && let Some((dest, exact)) = intermediates.next()
    {
        context.clear_unstucking(false);
        context.clear_last_movement();
        if matches!(moving.intermediate_hint(), Some(MovementHint::WalkAndJump)) {
            context.stalling_timeout_state = Some(Player::Jumping(Moving::new(
                cur_pos,
                dest,
                exact,
                Some(intermediates),
            )));

            let key = if dest.x - cur_pos.x >= 0 {
                KeyKind::Right
            } else {
                KeyKind::Left
            };
            resources.input.send_key_down(key);
            player.state = Player::Stalling(Timeout::default(), 3);
            return;
        }

        player.state = Player::Moving(dest, exact, Some(intermediates));
        return;
    }

    update_from_action(player, moving);
}

/// Aborts the action when state starts looping.
///
/// Note: Initially, this is only intended for auto mobbing until rune pathing is added...
#[inline]
fn abort_action_on_state_repeat(
    player: &mut PlayerEntity,
    player_next_state: Player,
    minimap_state: Minimap,
) {
    if player.context.track_last_movement_repeated() {
        info!(target:"backend/player","abort action due to repeated state");
        player.context.auto_mob_track_ignore_xs(minimap_state, true);
        player.context.clear_action_completed();
        player.state = Player::Idle;
        return;
    }

    player.state = player_next_state;
}

fn update_from_action(player: &mut PlayerEntity, moving: Moving) {
    let action = next_action(&player.context);
    let last_direction = player.context.last_known_direction;

    match action {
        Some(PlayerAction::Move(Move {
            wait_after_move_ticks,
            ..
        })) => {
            if wait_after_move_ticks > 0 {
                player.state = Player::Stalling(Timeout::default(), wait_after_move_ticks);
                return;
            }
            player.state = Player::Idle;
            player.context.clear_unstucking(false);
            player.context.clear_action_completed();
        }

        Some(PlayerAction::Key(
            key @ Key {
                with: ActionKeyWith::DoubleJump,
                direction,
                ..
            },
        )) => {
            player.state =
                if (matches!(direction, ActionKeyDirection::Any) || direction == last_direction) {
                    Player::DoubleJumping(DoubleJumping::new(moving, true, false))
                } else {
                    Player::UseKey(UseKey::from_key(key))
                };
        }

        Some(PlayerAction::Key(
            key @ Key {
                with: ActionKeyWith::Any | ActionKeyWith::Stationary,
                ..
            },
        )) => {
            player.state = Player::UseKey(UseKey::from_key(key));
        }

        Some(PlayerAction::AutoMob(mob)) => {
            player.state =
                Player::UseKey(UseKey::from_auto_mob(mob, ActionKeyDirection::Any, true));
        }

        Some(PlayerAction::SolveRune) => {
            player.state = Player::SolvingRune(SolvingRune::default());
        }

        Some(PlayerAction::PingPong(_)) => {
            player.context.clear_unstucking(false);
            player.context.clear_action_completed();
            player.state = Player::Idle;
        }

        Some(
            PlayerAction::SolveShape
            | PlayerAction::SolveVioletta
            | PlayerAction::Unstuck
            | PlayerAction::Panic(_)
            | PlayerAction::FamiliarsSwap(_)
            | PlayerAction::UseBooster(_)
            | PlayerAction::ExchangeBooster(_),
        ) => {
            panic!("unhandled action {action:?}")
        }

        None => (),
    }
}

#[inline]
pub fn find_intermediate_points(
    platforms: &Array<PlatformWithNeighbors, MAX_PLATFORMS_COUNT>,
    cur_pos: Point,
    dest: Point,
    exact: bool,
    up_jump_only: bool,
    enable_hint: bool,
) -> Option<MovingIntermediates> {
    let vertical_threshold = if up_jump_only {
        GRAPPLING_THRESHOLD
    } else {
        GRAPPLING_MAX_THRESHOLD
    };
    let vec = find_points_with(
        platforms,
        cur_pos,
        dest,
        enable_hint,
        DOUBLE_JUMP_THRESHOLD,
        JUMP_THRESHOLD,
        vertical_threshold,
    )?;
    let len = vec.len();
    let array = Array::from_iter(
        vec.into_iter()
            .enumerate()
            .map(|(i, (point, hint))| (point, hint, if i == len - 1 { exact } else { false })),
    );
    Some(MovingIntermediates {
        current: 0,
        inner: array,
    })
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use opencv::core::Point;

    use super::*;
    use crate::ecs::Resources;

    fn setup_player(pos: Point, state: Player) -> PlayerEntity {
        let mut player = PlayerEntity {
            state,
            context: PlayerContext::default(),
        };
        player.context.last_known_pos = Some(pos);
        player
    }

    #[test]
    fn update_moving_to_double_jump() {
        let resources = Resources::new(None, None);
        let dest = Point::new(100, 0); // Large x-distance triggers double jump
        let mut player = setup_player(Point::new(0, 0), Player::Moving(dest, false, None));

        update_moving_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::DoubleJumping(_));
    }

    #[test]
    fn update_moving_to_adjusting() {
        let resources = Resources::new(None, None);
        let dest = Point::new(20, 0); // Less than double jump x-distance
        let mut player = setup_player(Point::new(0, 0), Player::Moving(dest, false, None));

        update_moving_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::Adjusting(_));
    }

    #[test]
    fn update_moving_to_grappling() {
        let resources = Resources::new(None, None);
        let mut player = setup_player(
            Point::new(0, 0),
            Player::Moving(Point::new(0, GRAPPLING_THRESHOLD + 10), true, None),
        );
        player.context.config.grappling_key = Some(KeyKind::A);

        update_moving_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::Grappling(_));
    }

    #[test]
    fn update_moving_to_upjump() {
        let resources = Resources::new(None, None);
        let dest = Point::new(0, 20); // y-distance below grappling
        let mut player = setup_player(Point::new(0, 0), Player::Moving(dest, true, None));

        update_moving_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::UpJumping(_));
    }

    #[test]
    fn update_moving_to_jumping() {
        let resources = Resources::new(None, None);
        let cur_pos = Point::new(100, 100);
        let dest = Point::new(100, 106);
        let mut player = setup_player(cur_pos, Player::Moving(dest, false, None));

        update_moving_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::Jumping(_));
    }

    #[test]
    fn update_moving_to_falling() {
        let resources = Resources::new(None, None);
        let cur_pos = Point::new(100, 100);
        let dest = Point::new(100, 50);
        let mut player = setup_player(cur_pos, Player::Moving(dest, false, None));

        update_moving_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::Falling(_));
    }

    #[test]
    fn update_moving_to_idle_when_destination_reached() {
        let resources = Resources::new(None, None);
        let pos = Point::new(100, 200);
        let mut player = setup_player(pos, Player::Moving(pos, true, None));

        update_moving_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::Idle);
    }

    #[test]
    fn update_moving_with_intermediate_points_triggers_next_move() {
        let resources = Resources::new(None, None);
        let pos = Point::new(50, 0);
        let intermediates = MovingIntermediates {
            current: 1,
            inner: Array::from_iter([
                (pos, MovementHint::Infer, false),
                (Point::new(100, 0), MovementHint::Infer, true),
            ]),
        };
        let mut player = setup_player(pos, Player::Moving(pos, true, Some(intermediates)));

        update_moving_state(&resources, &mut player, Minimap::Detecting);

        assert_matches!(player.state, Player::Moving(Point { x: 100, y: 0 }, _, _));
    }
}
