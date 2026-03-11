use std::fmt;

use opencv::core::{Point, Rect};
use strum::Display;

use super::{Player, PlayerContext, use_key::UseKey};
use crate::{
    array::Array,
    bridge::{KeyKind, LinkKeyKind},
    ecs::Resources,
    minimap::Minimap,
    models::{
        Action, ActionKey, ActionKeyDirection, ActionKeyWith, ActionMove, FamiliarRarity, Position,
        SwappableFamiliars, WaitAfterBuffered,
    },
    player::PlayerEntity,
    run::MS_PER_TICK,
};

/// The minimum x distance required to transition to [`Player::UseKey`] in auto mob action.
pub const AUTO_MOB_USE_KEY_X_THRESHOLD: i32 = 16;

/// The minimum y distance required to transition to [`Player::UseKey`] in auto mob action.
pub const AUTO_MOB_USE_KEY_Y_THRESHOLD: i32 = 8;

/// Represents the fixed key action.
///
/// Converted from [`ActionKey`] without fields used by [`Rotator`]
#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub key: KeyKind,
    pub key_hold_ticks: u32,
    pub key_hold_buffered_to_wait_after: bool,
    pub link_key: LinkKeyKind,
    pub count: u32,
    pub position: Option<Position>,
    pub direction: ActionKeyDirection,
    pub with: ActionKeyWith,
    pub wait_before_use_ticks: u32,
    pub wait_before_use_ticks_random_range: u32,
    pub wait_after_use_ticks: u32,
    pub wait_after_use_ticks_random_range: u32,
    pub wait_after_buffered: WaitAfterBuffered,
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.position {
            Some(position) => {
                write!(f, "Key({} / {}, {})", self.key, position.x, position.y)
            }
            None => {
                write!(f, "Key({})", self.key)
            }
        }
    }
}

impl From<ActionKey> for Key {
    fn from(
        ActionKey {
            key,
            key_hold_millis,
            key_hold_buffered_to_wait_after,
            link_key,
            count,
            position,
            direction,
            with,
            wait_before_use_millis,
            wait_before_use_millis_random_range,
            wait_after_use_millis,
            wait_after_use_millis_random_range,
            wait_after_buffered,
            ..
        }: ActionKey,
    ) -> Self {
        let count = count.max(1);
        let key_hold_ticks = (key_hold_millis / MS_PER_TICK) as u32;
        let wait_before_use_ticks = (wait_before_use_millis / MS_PER_TICK) as u32;
        let wait_before_use_ticks_random_range =
            (wait_before_use_millis_random_range / MS_PER_TICK) as u32;
        let wait_after_use_ticks = (wait_after_use_millis / MS_PER_TICK) as u32;
        let wait_after_use_ticks_random_range =
            (wait_after_use_millis_random_range / MS_PER_TICK) as u32;

        Self {
            key: key.into(),
            key_hold_ticks,
            key_hold_buffered_to_wait_after,
            link_key: link_key.into(),
            count,
            position,
            direction,
            with,
            wait_before_use_ticks,
            wait_before_use_ticks_random_range,
            wait_after_use_ticks,
            wait_after_use_ticks_random_range,
            wait_after_buffered,
        }
    }
}

/// Represents the fixed move action.
///
/// Converted from [`ActionMove`] without fields used by [`Rotator`].
#[derive(Clone, Copy, Debug)]
pub struct Move {
    pub position: Position,
    pub wait_after_move_ticks: u32,
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Move({}, {})", self.position.x, self.position.y)
    }
}

impl From<ActionMove> for Move {
    fn from(
        ActionMove {
            position,
            wait_after_move_millis,
            ..
        }: ActionMove,
    ) -> Self {
        Self {
            position,
            wait_after_move_ticks: (wait_after_move_millis / MS_PER_TICK) as u32,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Default))]
pub struct AutoMob {
    pub key: KeyKind,
    pub key_hold_ticks: u32,
    pub link_key: LinkKeyKind,
    pub count: u32,
    pub with: ActionKeyWith,
    pub wait_before_ticks: u32,
    pub wait_before_ticks_random_range: u32,
    pub wait_after_ticks: u32,
    pub wait_after_ticks_random_range: u32,
    pub position: Position,
    pub is_pathing: bool,
}

impl fmt::Display for AutoMob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AutoMob({}, {})", self.position.x, self.position.y)
    }
}

/// Represents a ping pong action.
///
/// This is a type of action that moves in one direction and spams a fixed key. Once the player hits
/// either edges determined by [`Self::bound`] or close enough, the action is completed.
/// The [`Rotator`] then rotates the next action in the reverse direction.
///
/// This action forces the player to always stay inside the bound.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Default))]
pub struct PingPong {
    pub key: KeyKind,
    pub key_hold_ticks: u32,
    pub link_key: LinkKeyKind,
    pub count: u32,
    pub with: ActionKeyWith,
    pub wait_before_ticks: u32,
    pub wait_before_ticks_random_range: u32,
    pub wait_after_ticks: u32,
    pub wait_after_ticks_random_range: u32,
    /// Bound of ping pong action.
    ///
    /// This bound is in player relative coordinate.
    pub bound: Rect,
    pub direction: PingPongDirection,
}

impl fmt::Display for PingPong {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.direction {
            PingPongDirection::Left => write!(f, "PingPong(Left)"),
            PingPongDirection::Right => write!(f, "PingPong(Right)"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Default))]
pub enum PingPongDirection {
    #[cfg_attr(test, default)]
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
pub struct FamiliarsSwap {
    pub swappable_slots: SwappableFamiliars,
    pub swappable_rarities: Array<FamiliarRarity, 2>,
}

#[derive(Clone, Copy, Debug)]
pub struct Panic {
    pub to: PanicTo,
}

#[derive(Clone, Copy, Debug)]
pub enum PanicTo {
    Town,
    Channel,
}

#[derive(Clone, Copy, Debug)]
pub struct UseBooster {
    pub kind: Booster,
}

#[derive(Clone, Copy, Debug)]
pub enum Booster {
    Generic,
    Hexa,
}

#[derive(Clone, Copy, Debug)]
pub struct ExchangeBooster {
    pub amount: u32,
    pub all: bool,
}

/// Represents an action the [`Rotator`] can use.
#[derive(Clone, Debug, Display)]
pub enum PlayerAction {
    /// Fixed key action provided by the user.
    #[strum(to_string = "{0}")]
    Key(Key),
    /// Fixed move action provided by the user.
    #[strum(to_string = "{0}")]
    Move(Move),
    /// Solves rune action.
    SolveRune,
    /// Solves the lie detector's transparent shape.
    SolveShape,
    /// Solves the lie detector's violetta.
    SolveVioletta,
    /// Auto-mobbing action.
    #[strum(to_string = "{0}")]
    AutoMob(AutoMob),
    /// Ping pong action.
    #[strum(to_string = "{0}")]
    PingPong(PingPong),
    /// Swaps familiars action.
    FamiliarsSwap(FamiliarsSwap),
    /// Panics to town or another channel action.
    Panic(Panic),
    /// Use Generic or HEXA booster action.
    UseBooster(UseBooster),
    /// Exchange HEXA booster action.
    ExchangeBooster(ExchangeBooster),
    /// Unstucking by pressing ESC.
    Unstuck,
}

impl PlayerAction {
    pub(super) fn is_key_action_without_position(&self) -> bool {
        matches!(self, PlayerAction::Key(Key { position: None, .. }))
    }
}

impl From<Action> for PlayerAction {
    fn from(action: Action) -> Self {
        match action {
            Action::Move(action) => PlayerAction::Move(action.into()),
            Action::Key(action) => PlayerAction::Key(action.into()),
        }
    }
}

#[inline]
pub(super) fn next_action(context: &PlayerContext) -> Option<PlayerAction> {
    context
        .priority_action
        .clone()
        .or(context.normal_action.clone())
}

#[inline]
pub(super) fn update_from_ping_pong_action(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
    ping_pong: PingPong,
    cur_pos: Point,
) {
    let direction = ping_pong.direction;
    let bound = ping_pong.bound;
    let hit_x_bound_edge = match direction {
        PingPongDirection::Left => cur_pos.x - bound.x <= 0,
        PingPongDirection::Right => cur_pos.x - bound.x - bound.width >= 0,
    };
    if hit_x_bound_edge {
        player.context.clear_action_completed();
        player.context.clear_unstucking(false);
        player.state = Player::Idle;
        return;
    }

    release_arrow_keys(resources);
    let minimap_width = match minimap_state {
        Minimap::Idle(idle) => idle.bbox.width,
        _ => unreachable!(),
    };
    let y = cur_pos.y; // y doesn't matter in ping pong
    let moving = match direction {
        PingPongDirection::Left => Player::Moving(Point::new(0, y), false, None),
        PingPongDirection::Right => Player::Moving(Point::new(minimap_width, y), false, None),
    };
    player.state = moving;
}

/// Checks proximity in [`PlayerAction::AutoMob`] for transitioning to [`Player::UseKey`].
///
/// If `state` is [`Some`], this function will attempt to use key when auto mob is currently
/// pathing.
///
/// This is common logics shared with other contextual states when there is auto mob action.
#[inline]
pub(super) fn update_from_auto_mob_action(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
    mob: AutoMob,
    x_distance: i32,
    x_direction: i32,
    y_distance: i32,
) {
    let should_terminate =
        x_distance <= AUTO_MOB_USE_KEY_X_THRESHOLD && y_distance <= AUTO_MOB_USE_KEY_Y_THRESHOLD;
    if should_terminate && player.context.stalling_buffered.stalling() {
        player.context.clear_action_completed();
        player.state = Player::Idle;
        return;
    }

    let direction = match x_direction {
        direction if direction > 0 => ActionKeyDirection::Right,
        direction if direction < 0 => ActionKeyDirection::Left,
        _ => ActionKeyDirection::Any,
    };
    let should_check_pathing = matches!(
        player.state,
        Player::DoubleJumping(_) | Player::Adjusting(_)
    );

    if should_check_pathing
        && player
            .context
            .auto_mob_pathing_should_use_key(resources, minimap_state)
    {
        release_arrow_keys(resources);
        player.state = Player::UseKey(UseKey::from_auto_mob(mob, direction, should_terminate));
        return;
    }

    if should_terminate {
        release_arrow_keys(resources);
        player.context.last_known_direction = ActionKeyDirection::Any;
        player.state = Player::UseKey(UseKey::from_auto_mob(mob, direction, should_terminate));
    }
}

fn release_arrow_keys(resources: &Resources) {
    resources.input.send_key_up(KeyKind::Down);
    resources.input.send_key_up(KeyKind::Up);
    resources.input.send_key_up(KeyKind::Left);
    resources.input.send_key_up(KeyKind::Right);
}
