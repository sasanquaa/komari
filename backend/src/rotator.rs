use std::{
    assert_matches::debug_assert_matches,
    collections::{HashSet, VecDeque},
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};

use anyhow::Result;
use log::debug;
use opencv::core::{Point, Rect};
use ordered_hash_map::OrderedHashMap;
use rand::seq::IteratorRandom;

use crate::{
    ActionKeyDirection, ActionKeyWith, AutoMobbing, FamiliarRarity, KeyBinding, PanicMode,
    Position, RotationMode, SwappableFamiliars,
    array::Array,
    buff::{Buff, BuffKind},
    context::{Context, MS_PER_TICK},
    database::{Action, ActionCondition, ActionKey, ActionMove, PingPong},
    minimap::Minimap,
    player::{
        GRAPPLING_THRESHOLD, PanicTo, PingPongDirection, Player, PlayerAction, PlayerActionAutoMob,
        PlayerActionFamiliarsSwapping, PlayerActionKey, PlayerActionPanic, PlayerActionPingPong,
        PlayerState,
    },
    skill::{Skill, SkillKind},
    task::{Task, Update, update_detection_task},
};

const COOLDOWN_BETWEEN_QUEUE_MILLIS: u128 = 20_000;
const COOLDOWN_BETWEEN_POTION_QUEUE_MILLIS: u128 = 2_000;

/// [`Condition`] evaluation result.
enum ConditionResult {
    /// The action will be queued.
    Queue,
    /// The action is skipped and evaluated again on next update.
    Skip,
    /// The action is skipped but `last_queued_time` is updated.
    Ignore,
}

type ConditionFn = Box<dyn Fn(&Context, &mut PlayerState, Option<Instant>) -> ConditionResult>;

/// Predicate for when a priority action can be queued.
struct Condition(ConditionFn);

impl std::fmt::Debug for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "dyn Fn(...)")
    }
}

/// A priority action that can override a normal action.
///
/// This includes all non-[`ActionCondition::Any`] actions.
///
/// When a player is in the middle of doing a normal action, this type of action
/// can override most of the player's current state and forced to perform this action.
/// However, it cannot override player states that are considered "terminal". These states
/// include stalling, using key and forced double jumping. It also cannot override linked action.
///
/// When this type of action has [`Self::queue_to_front`] set, it will be queued to the
/// front and override other non-[`Self::queue_to_front`] priority action. The overriden
/// action is simply placed back to the queue in front. It is mostly useful for action such as
/// `press attack after x seconds even in the middle of moving`.
#[derive(Debug)]
struct PriorityAction {
    /// The predicate for when this action should be queued.
    condition: Condition,
    /// The kind the above predicate was derived from.
    condition_kind: Option<ActionCondition>,
    /// The inner action.
    inner: RotatorAction,
    /// Whether to queue this action to the front of [`Rotator::priority_actions_queue`].
    queue_to_front: bool,
    /// Whether this action is being ignored.
    ///
    /// While ignored, [`Self::last_queued_time`] will be updated to [`Instant::now`].
    /// The action is ignored for as long as it is still in the queue or the player
    /// is still executing it.
    ignoring: bool,
    /// The last [`Instant`] when this action was queued
    last_queued_time: Option<Instant>,
}

/// The action that will be passed to the player
///
/// There are [`RotatorAction::Single`] and [`RotatorAction::Linked`] actions.
/// With [`RotatorAction::Linked`] action is a linked list of actions. [`RotatorAction::Linked`]
/// action is executed in order, until completion and cannot be replaced by any other
/// type of actions.
#[derive(Clone, Debug)]
enum RotatorAction {
    Single(PlayerAction),
    Linked(LinkedAction),
}

/// A linked list of actions
#[derive(Clone, Debug)]
struct LinkedAction {
    inner: PlayerAction,
    next: Option<Box<LinkedAction>>,
}

/// The rotator's rotation mode
#[derive(Default, Debug)]
pub enum RotatorMode {
    StartToEnd,
    #[default]
    StartToEndThenReverse,
    AutoMobbing(AutoMobbing),
    PingPong(PingPong),
}

impl From<RotationMode> for RotatorMode {
    fn from(mode: RotationMode) -> Self {
        match mode {
            RotationMode::StartToEnd => RotatorMode::StartToEnd,
            RotationMode::StartToEndThenReverse => RotatorMode::StartToEndThenReverse,
            RotationMode::AutoMobbing(auto_mobbing) => RotatorMode::AutoMobbing(auto_mobbing),
            RotationMode::PingPong(ping_pong) => RotatorMode::PingPong(ping_pong),
        }
    }
}

#[derive(Default, Debug)]
pub struct Rotator {
    // This is literally free postfix increment!
    id_counter: AtomicU32,
    normal_actions: Vec<(u32, RotatorAction)>,
    normal_queuing_linked_action: Option<(u32, Box<LinkedAction>)>,
    normal_index: usize,
    /// Whether [`Self::normal_actions`] is being accessed from the end
    normal_actions_backward: bool,
    normal_actions_reset_on_erda: bool,
    normal_rotate_mode: RotatorMode,
    /// The [`Task`] used when [`Self::normal_rotate_mode`] is [`RotatorMode::AutoMobbing`]
    auto_mob_task: Option<Task<Result<Vec<Point>>>>,
    priority_actions: OrderedHashMap<u32, PriorityAction>,
    /// The currently executing [`RotatorAction::Linked`] action
    priority_queuing_linked_action: Option<(u32, Box<LinkedAction>)>,
    /// A [`VecDeque`] of [`PriorityAction`] ids
    ///
    /// Populates from [`Self::priority_actions`] when its predicate for queuing is true
    priority_actions_queue: VecDeque<u32>,
}

pub struct RotatorBuildArgs<'a> {
    pub mode: RotatorMode,
    pub actions: &'a [Action],
    pub buffs: &'a [(BuffKind, KeyBinding)],
    pub potion_key: KeyBinding,
    pub familiar_essence_key: KeyBinding,
    pub familiar_swappable_slots: SwappableFamiliars,
    pub familiar_swappable_rarities: &'a HashSet<FamiliarRarity>,
    pub familiar_swap_check_millis: u64,
    pub panic_mode: PanicMode,
    pub enable_panic_mode: bool,
    pub enable_rune_solving: bool,
    pub enable_change_channel_on_elite_boss_appear: bool,
    pub enable_familiars_swapping: bool,
    pub enable_reset_normal_actions_on_erda: bool,
}

impl Rotator {
    pub fn build_actions(&mut self, args: RotatorBuildArgs<'_>) {
        let RotatorBuildArgs {
            mode,
            actions,
            buffs,
            potion_key,
            familiar_essence_key,
            familiar_swappable_slots,
            familiar_swappable_rarities,
            familiar_swap_check_millis,
            panic_mode,
            enable_panic_mode,
            enable_rune_solving,
            enable_change_channel_on_elite_boss_appear,
            enable_familiars_swapping,
            enable_reset_normal_actions_on_erda,
        } = args;
        debug!(target: "rotator", "preparing actions {actions:?} {buffs:?}");
        self.reset_queue();
        self.normal_actions.clear();
        self.normal_rotate_mode = mode;
        self.normal_actions_reset_on_erda = enable_reset_normal_actions_on_erda;
        self.priority_actions.clear();

        let mut i = 0;
        while i < actions.len() {
            let action = actions[i];
            let condition = match action {
                Action::Move(ActionMove { condition, .. })
                | Action::Key(ActionKey { condition, .. }) => condition,
            };
            let queue_to_front = match action {
                Action::Move(_) => false,
                Action::Key(ActionKey { queue_to_front, .. }) => queue_to_front.unwrap_or_default(),
            };
            let (action, offset) = rotator_action(action, i, actions);
            debug_assert!(i != 0 || !matches!(condition, ActionCondition::Linked));
            // Should not move i below the match because it could cause
            // infinite loop due to auto mobbing ignoring Any condition
            i += offset;
            match condition {
                ActionCondition::EveryMillis(_) | ActionCondition::ErdaShowerOffCooldown => {
                    self.priority_actions.insert(
                        self.id_counter.fetch_add(1, Ordering::Relaxed),
                        priority_action(action, condition, queue_to_front),
                    );
                }
                ActionCondition::Any => {
                    if matches!(self.normal_rotate_mode, RotatorMode::AutoMobbing(_)) {
                        continue;
                    }
                    self.normal_actions
                        .push((self.id_counter.fetch_add(1, Ordering::Relaxed), action))
                }
                ActionCondition::Linked => unreachable!(),
            }
        }

        self.priority_actions.insert(
            self.id_counter.fetch_add(1, Ordering::Relaxed),
            elite_boss_potion_spam_priority_action(potion_key),
        );
        if buffs
            .iter()
            .any(|(buff, _)| matches!(buff, BuffKind::Familiar))
        {
            self.priority_actions.insert(
                self.id_counter.fetch_add(1, Ordering::Relaxed),
                familiar_essence_replenish_priority_action(familiar_essence_key),
            );
        }
        if enable_rune_solving {
            self.priority_actions.insert(
                self.id_counter.fetch_add(1, Ordering::Relaxed),
                solve_rune_priority_action(),
            );
        }
        if enable_change_channel_on_elite_boss_appear {
            self.priority_actions.insert(
                self.id_counter.fetch_add(1, Ordering::Relaxed),
                elite_boss_change_channel_priority_action(),
            );
        }
        if enable_familiars_swapping {
            self.priority_actions.insert(
                self.id_counter.fetch_add(1, Ordering::Relaxed),
                priority_action(
                    RotatorAction::Single(PlayerAction::FamiliarsSwapping(
                        PlayerActionFamiliarsSwapping {
                            swappable_slots: familiar_swappable_slots,
                            swappable_rarities: Array::from_iter(
                                familiar_swappable_rarities.clone(),
                            ),
                        },
                    )),
                    ActionCondition::EveryMillis(familiar_swap_check_millis),
                    true,
                ),
            );
        }
        if enable_panic_mode {
            self.priority_actions.insert(
                self.id_counter.fetch_add(1, Ordering::Relaxed),
                panic_priority_action(panic_mode),
            );
        }
        for (i, key) in buffs.iter().copied() {
            self.priority_actions.insert(
                self.id_counter.fetch_add(1, Ordering::Relaxed),
                buff_priority_action(i, key),
            );
        }
    }

    #[inline]
    pub fn reset_queue(&mut self) {
        self.normal_actions_backward = false;
        self.reset_normal_actions_queue();
        self.priority_actions_queue.clear();
        self.priority_queuing_linked_action = None;
    }

    #[inline]
    fn reset_normal_actions_queue(&mut self) {
        self.normal_index = 0;
        self.normal_queuing_linked_action = None;
    }

    #[inline]
    pub fn rotate_action(&mut self, context: &Context, player: &mut PlayerState) {
        if context.halting || matches!(context.player, Player::CashShopThenExit(_, _)) {
            return;
        }
        self.rotate_priority_actions(context, player);
        self.rotate_priority_actions_queue(context, player);
        if !player.has_priority_action() && !player.has_normal_action() {
            match self.normal_rotate_mode {
                RotatorMode::StartToEnd => self.rotate_start_to_end(player),
                RotatorMode::StartToEndThenReverse => self.rotate_start_to_end_then_reverse(player),
                RotatorMode::AutoMobbing(auto_mobbing) => {
                    self.rotate_auto_mobbing(context, player, auto_mobbing)
                }
                RotatorMode::PingPong(ping_pong) => {
                    self.rotate_ping_pong(context, player, ping_pong)
                }
            }
        }
    }

    /// Rotates the actions inside the [`Self::priority_actions`]
    ///
    /// This function does not pass the action to the player but only pushes the action to
    /// [`Self::priority_actions_queue`]. It is responsible for checking queuing condition.
    fn rotate_priority_actions(&mut self, context: &Context, player: &mut PlayerState) {
        /// Checks if the provided `id` is a priority linked action in queue or executing.
        #[inline]
        fn is_priority_linked_action_queuing_or_executing(
            rotator: &Rotator,
            player: &PlayerState,
            id: u32,
        ) -> bool {
            if rotator
                .priority_queuing_linked_action
                .as_ref()
                .is_some_and(|(action_id, _)| *action_id == id)
            {
                return true;
            }
            player.priority_action_id().is_some_and(|action_id| {
                action_id == id
                    && rotator
                        .priority_actions
                        .get(&id)
                        .is_some_and(|action| matches!(action.inner, RotatorAction::Linked(_)))
            })
        }

        /// Checks if the player or the queue has
        /// a [`ActionCondition::ErdaShowerOffCooldown`] action.
        #[inline]
        fn has_erda_action_queuing_or_executing(rotator: &Rotator, player: &PlayerState) -> bool {
            if player.priority_action_id().is_some_and(|id| {
                rotator.priority_actions.get(&id).is_some_and(|action| {
                    matches!(
                        action.condition_kind,
                        Some(ActionCondition::ErdaShowerOffCooldown)
                    )
                })
            }) {
                return true;
            }
            rotator.priority_actions_queue.iter().any(|id| {
                matches!(
                    rotator.priority_actions.get(id).unwrap().condition_kind,
                    Some(ActionCondition::ErdaShowerOffCooldown)
                )
            })
        }

        // Keeps ignoring while there is any type of erda condition action inside the queue
        let has_erda_action = has_erda_action_queuing_or_executing(self, player);
        let ids = self.priority_actions.keys().copied().collect::<Vec<_>>(); // why?
        let mut did_queue_erda_action = false;

        for id in ids {
            // Ignores for as long as the action is a linked action that is queuing
            // or executing
            let has_linked_action =
                is_priority_linked_action_queuing_or_executing(self, player, id);
            let action = self.priority_actions.get_mut(&id).unwrap();

            action.ignoring = match action.condition_kind {
                Some(ActionCondition::ErdaShowerOffCooldown) => {
                    has_erda_action || has_linked_action
                }
                Some(ActionCondition::Linked) | Some(ActionCondition::EveryMillis(_)) | None => {
                    player // The player currently executing action
                        .priority_action_id()
                        .is_some_and(|action_id| action_id == id)
                        || self // The action is in queue
                            .priority_actions_queue
                            .iter()
                            .any(|action_id| *action_id == id)
                        || has_linked_action
                }
                Some(ActionCondition::Any) => unreachable!(),
            };
            if action.ignoring {
                action.last_queued_time = Some(Instant::now());
                continue;
            }

            let result = (action.condition.0)(context, player, action.last_queued_time);
            match result {
                ConditionResult::Queue => {
                    if action.queue_to_front {
                        self.priority_actions_queue.push_front(id);
                    } else {
                        self.priority_actions_queue.push_back(id);
                    }
                    action.last_queued_time = Some(Instant::now());
                    if !did_queue_erda_action {
                        did_queue_erda_action = matches!(
                            action.condition_kind,
                            Some(ActionCondition::ErdaShowerOffCooldown)
                        );
                    }
                }
                ConditionResult::Skip => (),
                ConditionResult::Ignore => {
                    action.last_queued_time = Some(Instant::now());
                }
            }
        }

        if did_queue_erda_action && self.normal_actions_reset_on_erda {
            self.reset_normal_actions_queue();
            player.reset_normal_action();
        }
    }

    /// Rotates the actions inside the [`Self::priority_actions_queue`].
    ///
    /// If there is any on-going linked action:
    /// - For normal action, it will wait until the action is completed by the normal rotation.
    /// - For priority action, it will rotate and wait until all the actions are executed.
    ///
    /// After that, it will rotate actions inside [`Self::priority_actions_queue`].
    fn rotate_priority_actions_queue(&mut self, context: &Context, player: &mut PlayerState) {
        /// Checks if the player is queuing or executing a normal [`RotatorAction::Linked`] action.
        ///
        /// This prevents [`Self::rotate_priority_actions_queue`] from overriding the normal
        /// linked action.
        #[inline]
        fn has_normal_linked_action_queuing_or_executing(
            rotator: &Rotator,
            player: &PlayerState,
        ) -> bool {
            if rotator.normal_queuing_linked_action.is_some() {
                return true;
            }
            player.normal_action_id().is_some_and(|id| {
                rotator.normal_actions.iter().any(|(action_id, action)| {
                    *action_id == id && matches!(action, RotatorAction::Linked(_))
                })
            })
        }

        /// Checks if the player is executing a priority [`RotatorAction::Linked`] action.
        ///
        /// This does not check the queuing linked action because this check is to allow the linked
        /// action to be rotated in [`Self::rotate_priority_actions_queue`].
        #[inline]
        fn has_priority_linked_action_executing(rotator: &Rotator, player: &PlayerState) -> bool {
            player.priority_action_id().is_some_and(|id| {
                rotator
                    .priority_actions
                    .get(&id)
                    .is_some_and(|action| matches!(action.inner, RotatorAction::Linked(_)))
            })
        }

        if self.priority_actions_queue.is_empty() && self.priority_queuing_linked_action.is_none() {
            return;
        }
        if !context
            .player
            .can_action_override_current_state(player.last_known_pos)
            || has_normal_linked_action_queuing_or_executing(self, player)
            || has_priority_linked_action_executing(self, player)
        {
            return;
        }
        if self.rotate_queuing_linked_action(player, true) {
            return;
        }
        let id = *self.priority_actions_queue.front().unwrap();
        let Some(action) = self.priority_actions.get(&id) else {
            self.priority_actions_queue.pop_front();
            return;
        };
        let has_queue_to_front = player
            .priority_action_id()
            .and_then(|id| {
                self.priority_actions
                    .get(&id)
                    .map(|action| action.queue_to_front)
            })
            .unwrap_or_default();
        if has_queue_to_front {
            return;
        }
        if player.has_priority_action() && !action.queue_to_front {
            return;
        }

        self.priority_actions_queue.pop_front();
        match action.inner.clone() {
            RotatorAction::Single(inner) => {
                if action.queue_to_front {
                    if let Some(id) = player.replace_priority_action(id, inner) {
                        self.priority_actions_queue.push_front(id);
                    }
                } else {
                    player.set_priority_action(id, inner);
                }
            }
            RotatorAction::Linked(linked) => {
                if action.queue_to_front
                    && let Some(id) = player.take_priority_action()
                {
                    self.priority_actions_queue.push_front(id);
                }
                self.priority_queuing_linked_action = Some((id, Box::new(linked)));
                self.rotate_queuing_linked_action(player, true);
            }
        }
    }

    fn rotate_auto_mobbing(
        &mut self,
        context: &Context,
        player: &mut PlayerState,
        auto_mobbing: AutoMobbing,
    ) {
        debug_assert!(!player.has_normal_action() && !player.has_priority_action());
        let Minimap::Idle(idle) = context.minimap else {
            return;
        };
        let Some(pos) = player.last_known_pos else {
            return;
        };
        let AutoMobbing {
            bound,
            key,
            key_count,
            key_wait_before_millis,
            key_wait_after_millis,
        } = auto_mobbing;
        let bound = if player.config.auto_mob_platforms_bound {
            idle.platforms_bound.unwrap_or(bound.into())
        } else {
            bound.into()
        };
        let Update::Ok(points) =
            update_detection_task(context, 0, &mut self.auto_mob_task, move |detector| {
                detector.detect_mobs(idle.bbox, bound, pos)
            })
        else {
            return;
        };
        let Some(point) = points
            .iter()
            .filter(|point| {
                let y = idle.bbox.height - point.y;
                y <= pos.y || (y - pos.y).abs() <= GRAPPLING_THRESHOLD
            })
            .choose(&mut rand::rng())
            .map(|point| Point::new(point.x, idle.bbox.height - point.y))
            .and_then(|point| {
                debug!(target: "rotator", "auto mob raw position {point:?}");
                player.auto_mob_pick_reachable_y_position(context, point)
            })
            .or_else(|| {
                let point = player.auto_mob_pathing_point(context);
                debug!(target: "rotator", "auto mob use pathing point {point:?}");
                point
            })
        else {
            return;
        };
        player.set_normal_action(
            u32::MAX,
            PlayerAction::AutoMob(PlayerActionAutoMob {
                key,
                count: key_count.max(1),
                wait_before_ticks: (key_wait_before_millis / MS_PER_TICK) as u32,
                wait_after_ticks: (key_wait_after_millis / MS_PER_TICK) as u32,
                position: Position {
                    x: point.x,
                    x_random_range: 0,
                    y: point.y,
                    allow_adjusting: false,
                },
            }),
        );
    }

    fn rotate_ping_pong(
        &mut self,
        context: &Context,
        player: &mut PlayerState,
        ping_pong: PingPong,
    ) {
        debug_assert!(!player.has_normal_action() && !player.has_priority_action());
        let Minimap::Idle(idle) = context.minimap else {
            return;
        };
        let Some(pos) = player.last_known_pos else {
            return;
        };
        let PingPong {
            bound,
            key,
            key_count,
            key_wait_before_millis,
            key_wait_after_millis,
        } = ping_pong;

        let bbox = idle.bbox;
        let dist_left = pos.x - bbox.x;
        let dist_right = (bbox.x + bbox.width) - pos.x;
        let direction = if dist_left > dist_right {
            PingPongDirection::Left
        } else {
            PingPongDirection::Right
        };
        let bound = Rect::new(
            bound.x,
            bbox.height - (bound.y + bound.height),
            bound.width,
            bound.height,
        );

        player.set_normal_action(
            u32::MAX - 1,
            PlayerAction::PingPong(PlayerActionPingPong {
                key,
                count: key_count.max(1),
                wait_before_ticks: (key_wait_before_millis / MS_PER_TICK) as u32,
                wait_after_ticks: (key_wait_after_millis / MS_PER_TICK) as u32,
                bound,
                direction,
            }),
        );
    }

    fn rotate_start_to_end(&mut self, player: &mut PlayerState) {
        debug_assert!(!player.has_normal_action() && !player.has_priority_action());
        if self.normal_actions.is_empty() {
            return;
        }
        if self.rotate_queuing_linked_action(player, false) {
            return;
        }
        debug_assert!(self.normal_index < self.normal_actions.len());
        let (id, action) = self.normal_actions[self.normal_index].clone();
        self.normal_index = (self.normal_index + 1) % self.normal_actions.len();
        match action {
            RotatorAction::Single(action) => {
                player.set_normal_action(id, action);
            }
            RotatorAction::Linked(action) => {
                self.normal_queuing_linked_action = Some((id, Box::new(action)));
                self.rotate_queuing_linked_action(player, false);
            }
        }
    }

    fn rotate_start_to_end_then_reverse(&mut self, player: &mut PlayerState) {
        debug_assert!(!player.has_normal_action() && !player.has_priority_action());
        if self.normal_actions.is_empty() {
            return;
        }
        if self.rotate_queuing_linked_action(player, false) {
            return;
        }
        debug_assert!(self.normal_index < self.normal_actions.len());
        let len = self.normal_actions.len();
        let i = if self.normal_actions_backward {
            (len - self.normal_index).saturating_sub(1)
        } else {
            self.normal_index
        };
        if (self.normal_index + 1) == len {
            self.normal_actions_backward = !self.normal_actions_backward
        }
        let (id, action) = self.normal_actions[i].clone();
        self.normal_index = (self.normal_index + 1) % len;
        match action {
            RotatorAction::Single(action) => {
                player.set_normal_action(id, action);
            }
            RotatorAction::Linked(action) => {
                self.normal_queuing_linked_action = Some((id, Box::new(action)));
                self.rotate_queuing_linked_action(player, false);
            }
        }
    }

    #[inline]
    fn rotate_queuing_linked_action(
        &mut self,
        player: &mut PlayerState,
        is_priority: bool,
    ) -> bool {
        let linked_action = if is_priority {
            &mut self.priority_queuing_linked_action
        } else {
            &mut self.normal_queuing_linked_action
        };
        if linked_action.is_none() {
            return false;
        }
        let (id, action) = linked_action.take().unwrap();
        *linked_action = action.next.map(|action| (id, action));
        if is_priority {
            player.set_priority_action(id, action.inner);
        } else {
            player.set_normal_action(id, action.inner);
        }
        true
    }
}

/// Creates a [`RotatorAction`] with `start_action` as the initial action
///
/// If `start_action` is linked, this function returns [`RotatorAction::Linked`] with [`usize`] as
/// the offset from `start_index` to the next non-linked action.
/// Otherwise, this returns [`RotatorAction::Single`] with [`usize`] offset of 1.
#[inline]
fn rotator_action(
    start_action: Action,
    start_index: usize,
    actions: &[Action],
) -> (RotatorAction, usize) {
    if start_index == actions.len() - 1 {
        // Last action cannot be a linked action
        return (RotatorAction::Single(start_action.into()), 1);
    }
    if start_index + 1 < actions.len() {
        match actions[start_index + 1] {
            Action::Move(ActionMove {
                condition: ActionCondition::Linked,
                ..
            })
            | Action::Key(ActionKey {
                condition: ActionCondition::Linked,
                ..
            }) => (),
            _ => return (RotatorAction::Single(start_action.into()), 1),
        }
    }
    let mut head = LinkedAction {
        inner: start_action.into(),
        next: None,
    };
    let mut current = &mut head;
    let mut offset = 1;
    for action in actions.iter().skip(start_index + 1) {
        match action {
            Action::Move(ActionMove {
                condition: ActionCondition::Linked,
                ..
            })
            | Action::Key(ActionKey {
                condition: ActionCondition::Linked,
                ..
            }) => {
                let action = LinkedAction {
                    inner: (*action).into(),
                    next: None,
                };
                current.next = Some(Box::new(action));
                current = current.next.as_mut().unwrap();
                offset += 1;
            }
            _ => break,
        }
    }
    (RotatorAction::Linked(head), offset)
}

#[inline]
fn priority_action(
    action: RotatorAction,
    condition: ActionCondition,
    queue_to_front: bool,
) -> PriorityAction {
    debug_assert_matches!(
        condition,
        ActionCondition::EveryMillis(_) | ActionCondition::ErdaShowerOffCooldown
    );
    PriorityAction {
        inner: action,
        condition: Condition(Box::new(move |context, _, last_queued_time| {
            if should_queue_fixed_action(context, last_queued_time, condition) {
                ConditionResult::Queue
            } else {
                ConditionResult::Skip
            }
        })),
        condition_kind: Some(condition),
        queue_to_front,
        ignoring: false,
        last_queued_time: None,
    }
}

/// Creates a [`PlayerAction::Key`] priority action that automatically spams a potion key
/// when an elite boss is detected.
///
/// The action will only queue if:
/// - Enough time has passed since the last time this action was queued (debounced).
/// - The current minimap state is [`Minimap::Idle`] and an elite boss is present.
#[inline]
fn elite_boss_potion_spam_priority_action(key: KeyBinding) -> PriorityAction {
    PriorityAction {
        condition: Condition(Box::new(|context, _, last_queued_time| {
            if !at_least_millis_passed_since(last_queued_time, COOLDOWN_BETWEEN_POTION_QUEUE_MILLIS)
            {
                return ConditionResult::Skip;
            }
            if let Minimap::Idle(idle) = context.minimap
                && idle.has_elite_boss
            {
                ConditionResult::Queue
            } else {
                ConditionResult::Skip
            }
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::Key(PlayerActionKey {
            key,
            link_key: None,
            count: 1,
            position: None,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 5,
            wait_before_use_ticks_random_range: 0,
            wait_after_use_ticks: 0,
            wait_after_use_ticks_random_range: 0,
        })),
        queue_to_front: true,
        ignoring: false,
        last_queued_time: None,
    }
}

/// Creates a [`PlayerAction::Key`] priority action to replenish familiar essence
/// when it is detected as depleted.
///
/// The action will only queue if:
/// - Enough time has passed since the last queue attempt.
/// - The familiar buff is currently active.
/// - Familiar essence is detected as depleted.
///
/// If the essence is not depleted, the action will be marked as [`ConditionResult::Ignore`]
/// and temporarily ignored in subsequent queue do to `last_queued_time` being updated.
#[inline]
fn familiar_essence_replenish_priority_action(key: KeyBinding) -> PriorityAction {
    PriorityAction {
        condition: Condition(Box::new(|context, _, last_queued_time| {
            if !at_least_millis_passed_since(last_queued_time, COOLDOWN_BETWEEN_QUEUE_MILLIS) {
                return ConditionResult::Skip;
            }
            if !matches!(context.buffs[BuffKind::Familiar], Buff::Yes) {
                return ConditionResult::Skip;
            }
            if context.detector_unwrap().detect_familiar_essence_depleted() {
                ConditionResult::Queue
            } else {
                ConditionResult::Ignore
            }
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::Key(PlayerActionKey {
            key,
            link_key: None,
            count: 1,
            position: None,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 5,
            wait_before_use_ticks_random_range: 0,
            wait_after_use_ticks: 0,
            wait_after_use_ticks_random_range: 0,
        })),
        queue_to_front: true,
        ignoring: false,
        last_queued_time: None,
    }
}

/// Creates a [`PlayerAction::SolveRune`] priority action that triggers when a rune is available.
///
/// This action queues if all the following conditions are met:
/// - The player is not currently validating a rune.
/// - Enough time has passed since the last queue attempt.
/// - The minimap is in the [`Minimap::Idle`] state.
/// - A rune is present on the minimap.
/// - The player currently has no rune buff.
#[inline]
fn solve_rune_priority_action() -> PriorityAction {
    PriorityAction {
        condition: Condition(Box::new(|context, player, last_queued_time| {
            if player.is_validating_rune() {
                return ConditionResult::Skip;
            }
            if !at_least_millis_passed_since(last_queued_time, COOLDOWN_BETWEEN_QUEUE_MILLIS) {
                return ConditionResult::Skip;
            }
            if let Minimap::Idle(idle) = context.minimap
                && idle.rune.value().is_some()
                && matches!(context.buffs[BuffKind::Rune], Buff::No)
            {
                return ConditionResult::Queue;
            }
            ConditionResult::Skip
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::SolveRune),
        queue_to_front: true,
        ignoring: false,
        last_queued_time: None,
    }
}

/// Creates a [`PlayerAction::Key`] priority action to cast a specific buff when it's not active.
///
/// The action queues if:
/// - Enough time has passed since the last queue attempt.
/// - The minimap is in the [`Minimap::Idle`] state.
/// - The specified buff is currently missing.
#[inline]
fn buff_priority_action(buff: BuffKind, key: KeyBinding) -> PriorityAction {
    PriorityAction {
        condition: Condition(Box::new(move |context, _, last_queued_time| {
            if !at_least_millis_passed_since(last_queued_time, COOLDOWN_BETWEEN_QUEUE_MILLIS) {
                return ConditionResult::Skip;
            }
            if !matches!(context.minimap, Minimap::Idle(_)) {
                return ConditionResult::Skip;
            }
            if matches!(context.buffs[buff], Buff::No) {
                ConditionResult::Queue
            } else {
                ConditionResult::Skip
            }
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::Key(PlayerActionKey {
            key,
            link_key: None,
            count: 1,
            position: None,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 10,
            wait_before_use_ticks_random_range: 0,
            wait_after_use_ticks: 10,
            wait_after_use_ticks_random_range: 0,
        })),
        queue_to_front: true,
        ignoring: false,
        last_queued_time: None,
    }
}

#[inline]
fn panic_priority_action(mode: PanicMode) -> PriorityAction {
    let to = match mode {
        PanicMode::CycleChannel => PanicTo::Channel,
        PanicMode::GoToTown => PanicTo::Town,
    };

    PriorityAction {
        condition: Condition(Box::new(|context, _, last_queued_time| {
            if context.halting {
                return ConditionResult::Ignore;
            }
            match context.minimap {
                Minimap::Detecting => ConditionResult::Skip,
                Minimap::Idle(idle) => {
                    if !idle.has_any_other_player() || last_queued_time.is_none() {
                        return ConditionResult::Ignore;
                    }
                    if at_least_millis_passed_since(last_queued_time, 15000) {
                        ConditionResult::Queue
                    } else {
                        ConditionResult::Skip
                    }
                }
            }
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::Panic(PlayerActionPanic { to })),
        queue_to_front: true,
        ignoring: false,
        last_queued_time: None,
    }
}

#[inline]
fn elite_boss_change_channel_priority_action() -> PriorityAction {
    PriorityAction {
        condition: Condition(Box::new(|context, _, last_queued_time| {
            if !at_least_millis_passed_since(last_queued_time, 15000) {
                return ConditionResult::Skip;
            }
            if let Minimap::Idle(idle) = context.minimap
                && idle.has_elite_boss
            {
                ConditionResult::Queue
            } else {
                ConditionResult::Skip
            }
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::Panic(PlayerActionPanic {
            to: PanicTo::Channel,
        })),
        queue_to_front: true,
        ignoring: false,
        last_queued_time: None,
    }
}

#[inline]
fn at_least_millis_passed_since(last_queued_time: Option<Instant>, millis: u128) -> bool {
    last_queued_time
        .map(|instant| Instant::now().duration_since(instant).as_millis() >= millis)
        .unwrap_or(true)
}

#[inline]
fn should_queue_fixed_action(
    context: &Context,
    last_queued_time: Option<Instant>,
    condition: ActionCondition,
) -> bool {
    let millis_should_passed = match condition {
        ActionCondition::EveryMillis(millis) => millis as u128,
        ActionCondition::ErdaShowerOffCooldown => COOLDOWN_BETWEEN_QUEUE_MILLIS,
        ActionCondition::Linked | ActionCondition::Any => unreachable!(),
    };
    if !at_least_millis_passed_since(last_queued_time, millis_should_passed) {
        return false;
    }
    if matches!(condition, ActionCondition::ErdaShowerOffCooldown)
        && !matches!(context.skills[SkillKind::ErdaShower], Skill::Idle(_, _))
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use std::{
        assert_matches::assert_matches,
        time::{Duration, Instant},
    };

    use opencv::core::{Point, Vec4b};

    use super::*;
    use crate::{Position, buff::BuffKind, minimap::MinimapIdle, skill::SkillKind};

    const NORMAL_ACTION: Action = Action::Move(ActionMove {
        position: Position {
            x: 0,
            x_random_range: 0,
            y: 0,
            allow_adjusting: false,
        },
        condition: ActionCondition::Any,
        wait_after_move_millis: 0,
    });
    const PRIORITY_ACTION: Action = Action::Move(ActionMove {
        position: Position {
            x: 0,
            x_random_range: 0,
            y: 0,
            allow_adjusting: false,
        },
        condition: ActionCondition::ErdaShowerOffCooldown,
        wait_after_move_millis: 0,
    });

    #[test]
    fn rotator_at_least_millis_passed_since() {
        let now = Instant::now();
        assert!(at_least_millis_passed_since(None, 1000));
        assert!(at_least_millis_passed_since(
            Some(now - Duration::from_millis(2000)),
            1000
        ));
        assert!(!at_least_millis_passed_since(
            Some(now - Duration::from_millis(500)),
            1000
        ));
    }

    #[test]
    fn rotator_should_queue_fixed_action_every_millis() {
        let context = Context::new(None, None);
        let now = Instant::now();

        assert!(should_queue_fixed_action(
            &context,
            Some(now - Duration::from_millis(3000)),
            ActionCondition::EveryMillis(2000)
        ));
        assert!(!should_queue_fixed_action(
            &context,
            Some(now - Duration::from_millis(1000)),
            ActionCondition::EveryMillis(2000)
        ));
    }

    #[test]
    fn rotator_should_queue_fixed_action_erda_shower() {
        let mut context = Context::new(None, None);
        let now = Instant::now();

        context.skills[SkillKind::ErdaShower] = Skill::Idle(Point::default(), Vec4b::default());
        assert!(!should_queue_fixed_action(
            &context,
            Some(now - Duration::from_millis(COOLDOWN_BETWEEN_QUEUE_MILLIS as u64 - 1000)),
            ActionCondition::ErdaShowerOffCooldown
        ));
        assert!(should_queue_fixed_action(
            &context,
            Some(now - Duration::from_millis(COOLDOWN_BETWEEN_QUEUE_MILLIS as u64)),
            ActionCondition::ErdaShowerOffCooldown
        ));

        context.skills[SkillKind::ErdaShower] = Skill::Detecting;
        assert!(!should_queue_fixed_action(
            &context,
            Some(now - Duration::from_millis(COOLDOWN_BETWEEN_QUEUE_MILLIS as u64)),
            ActionCondition::ErdaShowerOffCooldown
        ));
    }

    #[test]
    fn rotator_build_actions() {
        let mut rotator = Rotator::default();
        let actions = vec![NORMAL_ACTION, NORMAL_ACTION, PRIORITY_ACTION];
        let buffs = vec![(BuffKind::Rune, KeyBinding::default()); 4];
        let args = RotatorBuildArgs {
            mode: RotatorMode::default(),
            actions: &actions,
            buffs: &buffs,
            potion_key: KeyBinding::default(),
            familiar_essence_key: KeyBinding::default(),
            familiar_swappable_slots: SwappableFamiliars::default(),
            familiar_swappable_rarities: &HashSet::default(),
            familiar_swap_check_millis: 0,
            panic_mode: PanicMode::default(),
            enable_panic_mode: false,
            enable_rune_solving: true,
            enable_change_channel_on_elite_boss_appear: false,
            enable_familiars_swapping: false,
            enable_reset_normal_actions_on_erda: false,
        };

        rotator.build_actions(args);
        assert_eq!(rotator.priority_actions.len(), 7);
        assert_eq!(rotator.normal_actions.len(), 2);
    }

    #[test]
    fn rotator_rotate_action_start_to_end_then_reverse() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let context = Context::new(None, None);
        rotator.normal_rotate_mode = RotatorMode::StartToEndThenReverse;
        for i in 0..2 {
            rotator
                .normal_actions
                .push((i, RotatorAction::Single(NORMAL_ACTION.into())));
        }

        rotator.rotate_action(&context, &mut player);
        assert!(player.has_normal_action());
        assert!(!rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 1);

        player.clear_actions_aborted();

        rotator.rotate_action(&context, &mut player);
        assert!(player.has_normal_action());
        assert!(rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 0);
    }

    #[test]
    fn rotator_rotate_action_start_to_end() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let context = Context::new(None, None);
        rotator.normal_rotate_mode = RotatorMode::StartToEnd;
        for i in 0..2 {
            rotator
                .normal_actions
                .push((i, RotatorAction::Single(NORMAL_ACTION.into())));
        }

        rotator.rotate_action(&context, &mut player);
        assert!(player.has_normal_action());
        assert!(!rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 1);

        player.clear_actions_aborted();

        rotator.rotate_action(&context, &mut player);
        assert!(player.has_normal_action());
        assert!(!rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 0);
    }

    #[test]
    fn rotator_priority_action_queue() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let mut minimap = MinimapIdle::default();
        minimap.rune.set_value(Point::default());
        let mut context = Context::new(None, None);
        context.minimap = Minimap::Idle(minimap);
        context.buffs[BuffKind::Rune] = Buff::No;
        rotator.priority_actions.insert(
            55,
            PriorityAction {
                condition: Condition(Box::new(|context, _, _| {
                    if matches!(context.minimap, Minimap::Idle(_)) {
                        ConditionResult::Queue
                    } else {
                        ConditionResult::Skip
                    }
                })),
                condition_kind: None,
                inner: RotatorAction::Single(PlayerAction::SolveRune),
                queue_to_front: true,
                ignoring: false,
                last_queued_time: None,
            },
        );

        rotator.rotate_action(&context, &mut player);
        assert_eq!(rotator.priority_actions_queue.len(), 0);
        assert_eq!(player.priority_action_id(), Some(55));
    }

    #[test]
    fn rotator_priority_action_queue_to_front() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let context = Context::new(None, None);
        // queue 2 non-front priority actions
        rotator.priority_actions.insert(
            2,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| ConditionResult::Queue)),
                condition_kind: None,
                inner: RotatorAction::Single(NORMAL_ACTION.into()),
                queue_to_front: false,
                ignoring: false,
                last_queued_time: None,
            },
        );
        rotator.priority_actions.insert(
            3,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| ConditionResult::Queue)),
                condition_kind: None,
                inner: RotatorAction::Single(NORMAL_ACTION.into()),
                queue_to_front: false,
                ignoring: false,
                last_queued_time: None,
            },
        );

        rotator.rotate_action(&context, &mut player);
        assert_eq!(rotator.priority_actions_queue.len(), 1);
        assert_eq!(player.priority_action_id(), Some(2));

        // add 1 front priority action
        rotator.priority_actions.insert(
            4,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| ConditionResult::Queue)),
                condition_kind: None,
                inner: RotatorAction::Single(NORMAL_ACTION.into()),
                queue_to_front: true,
                ignoring: false,
                last_queued_time: None,
            },
        );

        // non-front priority action get replaced
        rotator.rotate_action(&context, &mut player);
        assert_eq!(
            rotator.priority_actions_queue,
            VecDeque::from_iter([2, 3].into_iter())
        );
        assert_eq!(player.priority_action_id(), Some(4));

        // add another front priority action
        rotator.priority_actions.insert(
            5,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| ConditionResult::Queue)),
                condition_kind: None,
                inner: RotatorAction::Single(NORMAL_ACTION.into()),
                queue_to_front: true,
                ignoring: false,
                last_queued_time: None,
            },
        );

        // queued front priority action cannot be replaced
        // by another front priority action
        rotator.rotate_action(&context, &mut player);
        assert_eq!(
            rotator.priority_actions_queue,
            VecDeque::from_iter([5, 2, 3].into_iter())
        );
        assert_eq!(player.priority_action_id(), Some(4));
    }

    #[test]
    fn rotator_priority_linked_action() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let context = Context::new(None, None);
        rotator.priority_actions.insert(
            2,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| ConditionResult::Queue)),
                condition_kind: None,
                inner: RotatorAction::Linked(LinkedAction {
                    inner: NORMAL_ACTION.into(),
                    next: Some(Box::new(LinkedAction {
                        inner: NORMAL_ACTION.into(),
                        next: None,
                    })),
                }),
                queue_to_front: false,
                ignoring: false,
                last_queued_time: None,
            },
        );

        // linked action queued
        rotator.rotate_action(&context, &mut player);
        assert!(rotator.priority_actions_queue.is_empty());
        assert!(rotator.priority_queuing_linked_action.is_some());
        assert_eq!(player.priority_action_id(), Some(2));

        // linked action cannot be replaced by queue to front
        rotator.priority_actions.insert(
            4,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| ConditionResult::Queue)),
                condition_kind: None,
                inner: RotatorAction::Single(PlayerAction::SolveRune),
                queue_to_front: true,
                ignoring: false,
                last_queued_time: None,
            },
        );
        rotator.rotate_action(&context, &mut player);
        assert_eq!(
            rotator.priority_actions_queue,
            VecDeque::from_iter([4].into_iter())
        );

        player.clear_actions_aborted();
        rotator.rotate_action(&context, &mut player);
        assert!(rotator.priority_queuing_linked_action.is_none());
        assert_eq!(
            rotator.priority_actions_queue,
            VecDeque::from_iter([4].into_iter())
        );
        assert_eq!(player.priority_action_id(), Some(2));
    }

    #[test]
    fn rotate_ping_pong_direction() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let mut idle = MinimapIdle::default();
        idle.bbox = Rect::new(0, 0, 100, 100); // x: [0, 100]

        let mut context = Context::new(None, None);
        context.minimap = Minimap::Idle(idle);

        // Closer to right, further than left -> Go left
        player.last_known_pos = Some(Point::new(80, 50));
        rotator.rotate_ping_pong(
            &context,
            &mut player,
            PingPong {
                bound: Rect::new(20, 20, 80, 80).into(),
                key: KeyBinding::default(),
                key_count: 1,
                key_wait_before_millis: 0,
                key_wait_after_millis: 0,
            },
        );

        assert_matches!(
            player.normal_action(),
            Some(PlayerAction::PingPong(PlayerActionPingPong {
                direction: PingPongDirection::Left,
                ..
            }))
        );

        // Closer to left, further than right -> Go right
        player.clear_actions_aborted();
        player.last_known_pos = Some(Point::new(10, 50));
        rotator.rotate_ping_pong(
            &context,
            &mut player,
            PingPong {
                bound: Rect::new(20, 20, 80, 80).into(),
                key: KeyBinding::default(),
                key_count: 1,
                key_wait_before_millis: 0,
                key_wait_after_millis: 0,
            },
        );

        assert_matches!(
            player.normal_action(),
            Some(PlayerAction::PingPong(PlayerActionPingPong {
                direction: PingPongDirection::Right,
                ..
            }))
        );
    }
}
