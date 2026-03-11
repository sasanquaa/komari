use std::{
    assert_matches::debug_assert_matches,
    collections::VecDeque,
    fmt::Debug,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Instant,
};

use anyhow::Result;
use log::{debug, info};
#[cfg(test)]
use mockall::{automock, concretize};
use opencv::core::{Point, Rect};
use ordered_hash_map::OrderedHashMap;

use crate::{
    Bound,
    array::Array,
    bridge::{KeyKind, LinkKeyKind},
    buff::{Buff, BuffKind},
    detect::{Detector, QuickSlotsHexaBooster, SolErda},
    ecs::{Resources, World},
    minimap::Minimap,
    models::{
        Action, ActionCondition, ActionKey, ActionKeyDirection, ActionKeyWith, ActionMove,
        EliteBossBehavior, ExchangeHexaBoosterCondition, Familiars, MobbingKey, Position,
        WaitAfterBuffered,
    },
    player::{
        AutoMob, Booster, ExchangeBooster, FamiliarsSwap, GRAPPLING_THRESHOLD, Key, Panic, PanicTo,
        PingPong, PingPongDirection, PlayerAction, PlayerContext, PlayerEntity, Quadrant,
        UseBooster,
    },
    run::MS_PER_TICK,
    skill::{Skill, SkillKind},
    task::{Task, Update, update_detection_task},
};

const AUTO_MOB_SAME_QUAD_THRESHOLD: u32 = 5;

/// [`Condition`] evaluation result.
#[derive(Debug)]
enum ConditionResult {
    /// The action will be queued.
    Queue,
    /// The action is skipped.
    Skip,
    /// The action is skipped but `last_queued_time` is updated.
    Ignore,
}

type ConditionFn = Box<dyn FnMut(&Resources, &World, &PriorityActionQueueInfo) -> ConditionResult>;

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
    /// The metadata about this action.
    metadata: Option<ActionMetadata>,
    /// Whether to queue this action to the front of [`Rotator::priority_actions_queue`].
    queue_to_front: bool,
    queue_info: PriorityActionQueueInfo,
}

#[derive(Debug, Default)]
struct PriorityActionQueueInfo {
    /// Whether this action is being ignored.
    ///
    /// While ignored, [`Self::last_queued_time`] will be updated to [`Instant::now`].
    /// The action is ignored for as long as it is still in the queue or the player
    /// is still executing it.
    ignoring: bool,
    /// The last [`Instant`] when this action was queued
    last_queued_time: Option<Instant>,
}

/// Action metadata to help identifying action type.
#[derive(Debug, Copy, Clone)]
enum ActionMetadata {
    UseBooster,
    Buff { kind: BuffKind },
}

/// The action that will be passed to the player.
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

/// A linked list of actions.
#[derive(Clone, Debug)]
struct LinkedAction {
    inner: PlayerAction,
    next: Option<Box<LinkedAction>>,
}

/// The rotator's rotation mode.
#[derive(Default, Debug, Clone, Copy)]
pub enum RotatorMode {
    StartToEnd,
    #[default]
    StartToEndThenReverse,
    AutoMobbing(MobbingKey, Bound),
    PingPong(MobbingKey, Bound),
}

#[derive(Debug, Default, Clone)]
pub struct RotatorBuildArgs {
    pub mode: RotatorMode,
    pub character_actions: Vec<Action>,
    pub map_actions: Vec<Action>,
    pub buffs: Vec<(BuffKind, KeyKind)>,
    pub familiars: Familiars,
    pub familiar_essence_key: KeyKind,
    pub elite_boss_behavior: EliteBossBehavior,
    pub elite_boss_behavior_key: KeyKind,
    pub hexa_booster_exchange_condition: ExchangeHexaBoosterCondition,
    pub hexa_booster_exchange_amount: u32,
    pub hexa_booster_exchange_all: bool,
    pub enable_panic_mode: bool,
    pub enable_rune_solving: bool,
    pub enable_transparent_shape_solving: bool,
    pub enable_violetta_solving: bool,
    pub enable_reset_normal_actions_on_erda: bool,
    pub enable_using_generic_booster: bool,
    pub enable_using_hexa_booster: bool,
}

/// Handles rotating provided [`PlayerAction`]s.
#[cfg_attr(test, automock)]
pub trait Rotator: Debug + 'static {
    #[cfg_attr(test, concretize)]
    fn build_actions(&mut self, args: RotatorBuildArgs);

    /// Resets priority and normal actions queues.
    ///
    /// This does not remove previously built actions.
    fn reset_queue(&mut self);

    /// Injects an action to be executed.
    ///
    /// This can be useful for one-time action that needs to be run in response to some external
    /// event (e.g. chat). But should work co-operatively with previously built actions instead of
    /// directly overwriting through [`PlayerState::set_priority_action`].
    fn inject_action(&mut self, action: PlayerAction);

    /// Rotates actions previously built with [`Self::build_actions`].
    ///
    /// If [`Operation`] is currently halting, it does not rotate the built actions but only the
    /// side-loaded actions added by [`Self::inject_action`].
    fn rotate_action(&mut self, resources: &Resources, world: &mut World);
}

#[derive(Default, Debug)]
pub struct DefaultRotator {
    normal_actions: Vec<(u32, RotatorAction)>,
    normal_queuing_linked_action: Option<(u32, Box<LinkedAction>)>,
    normal_index: usize,
    /// Whether [`Self::normal_actions`] is being accessed from the end
    normal_actions_backward: bool,
    normal_actions_reset_on_erda: bool,
    normal_rotate_mode: RotatorMode,

    /// The [`Task`] used when [`Self::normal_rotate_mode`] is [`RotatorMode::AutoMobbing`]
    auto_mob_task: Option<Task<Result<Vec<Point>>>>,
    /// Tracks number of times a mob detection has been completed inside the same quad.
    ///
    /// This limits the number of detections can be done inside the same quad as to help player
    /// advances to the next quad.
    auto_mob_quadrant_consecutive_count: Option<(Quadrant, u32)>,

    priority_actions: OrderedHashMap<u32, PriorityAction>,
    /// The currently executing [`RotatorAction::Linked`] action
    priority_queuing_linked_action: Option<(u32, Box<LinkedAction>)>,
    /// A [`VecDeque`] of [`PriorityAction`] ids
    ///
    /// Populates from [`Self::priority_actions`] when its predicate for queuing is true
    priority_actions_queue: VecDeque<u32>,
    /// Side-loaded one-time priority actions.
    ///
    /// These are actions injected externally and to be executed as appropriate with the current
    /// [`Self::priority_actions_queue`]. These actions are run only once and do not have an ID.
    priority_actions_side_queue: VecDeque<RotatorAction>,
}

impl DefaultRotator {
    #[inline]
    fn reset_normal_actions_queue(&mut self) {
        self.normal_index = 0;
        self.normal_queuing_linked_action = None;
    }

    /// Rotates the actions inside the [`Self::priority_actions`]
    ///
    /// This function does not pass the action to the player but only pushes the action to
    /// [`Self::priority_actions_queue`]. It is responsible for checking queuing condition.
    fn rotate_priority_actions(&mut self, resources: &Resources, world: &mut World) {
        #[derive(Debug)]
        enum ResolveConflict {
            None,
            #[allow(dead_code)]
            Replace {
                id: u32,
            },
            Ignore,
        }

        /// Checks if the provided `id` is a priority linked action in queue or executing.
        #[inline]
        fn is_priority_linked_action_queuing_or_executing(
            rotator: &DefaultRotator,
            player_context: &PlayerContext,
            id: u32,
        ) -> bool {
            let queuing_id = rotator
                .priority_queuing_linked_action
                .as_ref()
                .map(|(action_id, _)| *action_id);
            if Some(id) == queuing_id {
                return true;
            }

            let Some(action_id) = player_context.priority_action_id() else {
                return false;
            };
            if action_id != id {
                return false;
            }

            rotator
                .priority_actions
                .get(&id)
                .is_some_and(|action| matches!(action.inner, RotatorAction::Linked(_)))
        }

        /// Checks if the player or the queue has
        /// a [`ActionCondition::ErdaShowerOffCooldown`] action.
        #[inline]
        fn has_erda_action_queuing_or_executing(
            rotator: &DefaultRotator,
            player_context: &PlayerContext,
        ) -> bool {
            if let Some(id) = player_context.priority_action_id()
                && let Some(action) = rotator.priority_actions.get(&id)
                && matches!(
                    action.condition_kind,
                    Some(ActionCondition::ErdaShowerOffCooldown)
                )
            {
                return true;
            }

            rotator.priority_actions_queue.iter().any(|id| {
                let condition = rotator
                    .priority_actions
                    .get(id)
                    .and_then(|action| action.condition_kind);
                matches!(condition, Some(ActionCondition::ErdaShowerOffCooldown))
            })
        }

        fn resolve_conflict_from_metadata(
            rotator: &DefaultRotator,
            player_context: &PlayerContext,
            metadata: ActionMetadata,
        ) -> ResolveConflict {
            match metadata {
                ActionMetadata::UseBooster => {
                    if player_context
                        .priority_action_id()
                        .and_then(|id| rotator.priority_actions.get(&id))
                        .and_then(|action| action.metadata)
                        .is_some_and(|metadata| matches!(metadata, ActionMetadata::UseBooster))
                    {
                        info!(target: "backend/rotator", "ignored booster usage due to conflict with another booster kind");
                        return ResolveConflict::Ignore;
                    }

                    for id in rotator.priority_actions_queue.iter() {
                        let action = rotator.priority_actions.get(id).expect("exists");
                        if matches!(action.metadata, Some(ActionMetadata::UseBooster)) {
                            info!(target: "backend/rotator", "ignored booster usage due to conflict with another booster kind");
                            return ResolveConflict::Ignore;
                        }
                    }
                }
                ActionMetadata::Buff {
                    kind: BuffKind::ExpCouponX2 | BuffKind::ExpCouponX3 | BuffKind::ExpCouponX4,
                } => {
                    // TODO:
                }
                ActionMetadata::Buff {
                    kind: BuffKind::WealthAcquisitionPotion | BuffKind::SmallWealthAcquisitionPotion,
                } => {
                    // TODO:
                }
                ActionMetadata::Buff {
                    kind: BuffKind::ExpAccumulationPotion | BuffKind::SmallExpAccumulationPotion,
                } => {
                    // TODO:
                }
                ActionMetadata::Buff { .. } => (),
            }

            ResolveConflict::None
        }

        // Keeps ignoring while there is any type of erda condition action inside the queue
        let has_erda_action = has_erda_action_queuing_or_executing(self, &world.player.context);
        let ids = self.priority_actions.keys().copied().collect::<Vec<_>>();
        let mut did_queue_erda_action = false;

        for id in ids {
            // Ignores for as long as the action is a linked action that is queuing
            // or executing
            let has_linked_action =
                is_priority_linked_action_queuing_or_executing(self, &world.player.context, id);
            let action = self.priority_actions.get_mut(&id).expect("action id exist");

            action.queue_info.ignoring = match action.condition_kind {
                Some(ActionCondition::ErdaShowerOffCooldown) => {
                    has_erda_action || has_linked_action
                }
                Some(ActionCondition::Linked) | Some(ActionCondition::EveryMillis(_)) | None => {
                    world
                        .player
                        .context // The player currently executing action
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
            if action.queue_info.ignoring {
                action.queue_info.last_queued_time = Some(Instant::now());
                continue;
            }

            let condition_fn = &mut action.condition.0;
            let result = condition_fn(resources, world, &mut action.queue_info);
            match result {
                ConditionResult::Queue => {
                    let conflict = if let Some(metadata) = action.metadata {
                        resolve_conflict_from_metadata(self, &world.player.context, metadata)
                    } else {
                        ResolveConflict::None
                    };
                    // Reborrow mutably here so the above `resolve_conflict_from_metadata`
                    // can do immutable borrow.
                    let action = self.priority_actions.get_mut(&id).expect("action id exist");

                    match conflict {
                        ResolveConflict::None => {
                            if action.queue_to_front {
                                self.priority_actions_queue.push_front(id);
                            } else {
                                self.priority_actions_queue.push_back(id);
                            }
                            action.queue_info.last_queued_time = Some(Instant::now());

                            if !did_queue_erda_action {
                                did_queue_erda_action = matches!(
                                    action.condition_kind,
                                    Some(ActionCondition::ErdaShowerOffCooldown)
                                );
                            }
                        }
                        ResolveConflict::Replace { id: replace_id } => {
                            if let Some(replace_id) = self
                                .priority_actions_queue
                                .iter_mut()
                                .find(|id| **id == replace_id)
                            {
                                *replace_id = id;
                            }

                            action.queue_info.last_queued_time = Some(Instant::now());
                        }
                        ResolveConflict::Ignore => {
                            action.queue_info.last_queued_time = Some(Instant::now());
                        }
                    }
                }
                ConditionResult::Skip => (),
                ConditionResult::Ignore => {
                    action.queue_info.last_queued_time = Some(Instant::now());
                }
            }
        }

        if did_queue_erda_action && self.normal_actions_reset_on_erda {
            self.reset_normal_actions_queue();
            world.player.context.reset_normal_action();
        }
    }

    /// Rotates the actions inside the [`Self::priority_actions_queue`].
    ///
    /// If there is any on-going linked action:
    /// - For normal action, it will wait until the action is completed by the normal rotation.
    /// - For priority action, it will rotate and wait until all the actions are executed.
    ///
    /// After that, it will rotate actions inside [`Self::priority_actions_queue`].
    fn rotate_priority_actions_queue(&mut self, player: &mut PlayerEntity) {
        /// Checks if the player is queuing or executing a normal [`RotatorAction::Linked`] action.
        ///
        /// This prevents [`Self::rotate_priority_actions_queue`] from overriding the normal
        /// linked action.
        #[inline]
        fn has_normal_linked_action_queuing_or_executing(
            rotator: &DefaultRotator,
            player_context: &PlayerContext,
        ) -> bool {
            if rotator.normal_queuing_linked_action.is_some() {
                return true;
            }
            player_context.normal_action_id().is_some_and(|id| {
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
        fn has_priority_linked_action_executing(
            rotator: &DefaultRotator,
            player_context: &PlayerContext,
        ) -> bool {
            player_context.priority_action_id().is_some_and(|id| {
                rotator
                    .priority_actions
                    .get(&id)
                    .is_some_and(|action| matches!(action.inner, RotatorAction::Linked(_)))
            })
        }

        if self.priority_actions_queue.is_empty()
            && self.priority_actions_side_queue.is_empty()
            && self.priority_queuing_linked_action.is_none()
        {
            return;
        }
        if !player
            .state
            .can_override_current_state(player.context.last_known_pos)
            || has_normal_linked_action_queuing_or_executing(self, &player.context)
            || has_priority_linked_action_executing(self, &player.context)
            || has_side_loaded_action_executing(&player.context)
        {
            return;
        }

        if self.rotate_queuing_linked_action(&mut player.context, true) {
            return;
        }
        if self.rotate_side_priority_action(&mut player.context) {
            return;
        }

        let player_has_queue_to_front = player
            .context
            .priority_action_id()
            .and_then(|id| {
                self.priority_actions
                    .get(&id)
                    .map(|action| action.queue_to_front)
            })
            .unwrap_or_default();
        if player_has_queue_to_front {
            return;
        }

        let Some(id) = self.priority_actions_queue.pop_front_if(|id| {
            self.priority_actions
                .get(id)
                .is_none_or(|action| !player.context.has_priority_action() || action.queue_to_front)
        }) else {
            return;
        };
        let Some(action) = self.priority_actions.get(&id) else {
            return;
        };

        match action.inner.clone() {
            RotatorAction::Single(inner) => {
                if action.queue_to_front {
                    if let Some(id) = player.context.replace_priority_action(Some(id), inner) {
                        self.priority_actions_queue.push_front(id);
                    }
                } else {
                    player.context.set_priority_action(Some(id), inner);
                }
            }
            RotatorAction::Linked(linked) => {
                if action.queue_to_front
                    && let Some(id) = player.context.take_priority_action()
                {
                    self.priority_actions_queue.push_front(id);
                }
                self.priority_queuing_linked_action = Some((id, Box::new(linked)));
                self.rotate_queuing_linked_action(&mut player.context, true);
            }
        }
    }

    fn rotate_auto_mobbing(
        &mut self,
        resources: &Resources,
        player_context: &mut PlayerContext,
        minimap_state: Minimap,
        key: MobbingKey,
        bound: Bound,
    ) {
        if player_context.has_normal_action() {
            return;
        }

        let Minimap::Idle(idle) = minimap_state else {
            return;
        };
        let Some(pos) = player_context.last_known_pos else {
            return;
        };
        let bound = bound.into();

        let name = player_context.name();
        let Update::Ok(points) =
            update_detection_task(resources, 0, &mut self.auto_mob_task, move |detector| {
                detector.detect_mobs(idle.bbox, bound, pos, name)
            })
        else {
            return;
        };
        // FIXME: Collect to a Vec first because `context.rng` needs to be borrowed again.
        let points = points
            .iter()
            .filter_map(|point| {
                let y = idle.bbox.height - point.y;
                let point = if y <= pos.y || (y - pos.y).abs() <= GRAPPLING_THRESHOLD {
                    Some(Point::new(point.x, y))
                } else {
                    None
                };
                debug!(target: "backend/rotator", "auto mob raw position {point:?}");
                point.and_then(|point| {
                    player_context.auto_mob_pick_reachable_y_position(
                        resources,
                        minimap_state,
                        point,
                    )
                })
            })
            .collect::<Vec<_>>();
        let mut use_pathing_point = false;

        if let Some(last_quad) = player_context.auto_mob_last_quadrant()
            && !points.is_empty()
        {
            if self
                .auto_mob_quadrant_consecutive_count
                .is_none_or(|(quad, _)| quad != last_quad)
            {
                self.auto_mob_quadrant_consecutive_count = Some((last_quad, 0));
            }
            let (_, count) = self
                .auto_mob_quadrant_consecutive_count
                .as_mut()
                .expect("is some");

            *count += 1;
            if *count >= AUTO_MOB_SAME_QUAD_THRESHOLD {
                *count = 0;
                use_pathing_point = true;
            }
        }

        let mut is_pathing = use_pathing_point;
        let point = if use_pathing_point {
            player_context.auto_mob_pathing_point(resources, minimap_state, bound)
        } else {
            resources
                .rng
                .random_choose(points.into_iter())
                .unwrap_or_else(|| {
                    is_pathing = true;
                    player_context.auto_mob_pathing_point(resources, minimap_state, bound)
                })
        };
        let key_hold_ticks = (key.key_hold_millis / MS_PER_TICK) as u32;
        let wait_before_ticks = (key.wait_before_millis / MS_PER_TICK) as u32;
        let wait_before_ticks_random_range =
            (key.wait_before_millis_random_range / MS_PER_TICK) as u32;
        let wait_after_ticks = (key.wait_after_millis / MS_PER_TICK) as u32;
        let wait_after_ticks_random_range =
            (key.wait_after_millis_random_range / MS_PER_TICK) as u32;
        let position = Position {
            x: point.x,
            x_random_range: 0,
            y: point.y,
            allow_adjusting: false,
        };

        player_context.set_normal_action(
            None,
            PlayerAction::AutoMob(AutoMob {
                key: key.key.into(),
                key_hold_ticks,
                link_key: key.link_key.into(),
                count: key.count.max(1),
                with: key.with,
                wait_before_ticks,
                wait_before_ticks_random_range,
                wait_after_ticks,
                wait_after_ticks_random_range,
                position,
                is_pathing,
            }),
        );
    }

    fn rotate_ping_pong(
        &mut self,
        player_context: &mut PlayerContext,
        minimap_state: Minimap,
        key: MobbingKey,
        bound: Bound,
    ) {
        if player_context.has_normal_action() {
            return;
        }

        let Minimap::Idle(idle) = minimap_state else {
            return;
        };
        let Some(pos) = player_context.last_known_pos else {
            return;
        };

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

        player_context.set_normal_action(
            None,
            PlayerAction::PingPong(PingPong {
                key: key.key.into(),
                key_hold_ticks: (key.key_hold_millis / MS_PER_TICK) as u32,
                link_key: key.link_key.into(),
                count: key.count.max(1),
                with: key.with,
                wait_before_ticks: (key.wait_before_millis / MS_PER_TICK) as u32,
                wait_before_ticks_random_range: (key.wait_before_millis_random_range / MS_PER_TICK)
                    as u32,
                wait_after_ticks: (key.wait_after_millis / MS_PER_TICK) as u32,
                wait_after_ticks_random_range: (key.wait_after_millis_random_range / MS_PER_TICK)
                    as u32,
                bound,
                direction,
            }),
        );
    }

    fn rotate_start_to_end(&mut self, player_context: &mut PlayerContext) {
        if player_context.has_normal_action() || self.normal_actions.is_empty() {
            return;
        }
        if self.rotate_queuing_linked_action(player_context, false) {
            return;
        }

        debug_assert!(self.normal_index < self.normal_actions.len());
        let (id, action) = self.normal_actions[self.normal_index].clone();
        self.normal_index = (self.normal_index + 1) % self.normal_actions.len();
        match action {
            RotatorAction::Single(action) => {
                player_context.set_normal_action(Some(id), action);
            }
            RotatorAction::Linked(action) => {
                self.normal_queuing_linked_action = Some((id, Box::new(action)));
                self.rotate_queuing_linked_action(player_context, false);
            }
        }
    }

    fn rotate_start_to_end_then_reverse(&mut self, player_context: &mut PlayerContext) {
        if player_context.has_normal_action() || self.normal_actions.is_empty() {
            return;
        }
        if self.rotate_queuing_linked_action(player_context, false) {
            return;
        }

        let len = self.normal_actions.len();
        if (self.normal_index + 1) == len {
            self.normal_actions_backward = !self.normal_actions_backward;
            self.normal_index = 0;
        }

        debug_assert!(self.normal_index < self.normal_actions.len());

        let i = if self.normal_actions_backward {
            (len - self.normal_index).saturating_sub(1)
        } else {
            self.normal_index
        };
        let (id, action) = self.normal_actions[i].clone();

        self.normal_index = (self.normal_index + 1) % len;
        match action {
            RotatorAction::Single(action) => {
                player_context.set_normal_action(Some(id), action);
            }
            RotatorAction::Linked(action) => {
                self.normal_queuing_linked_action = Some((id, Box::new(action)));
                self.rotate_queuing_linked_action(player_context, false);
            }
        }
    }

    #[inline]
    fn rotate_queuing_linked_action(
        &mut self,
        player_context: &mut PlayerContext,
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
            player_context.set_priority_action(Some(id), action.inner);
        } else {
            player_context.set_normal_action(Some(id), action.inner);
        }
        true
    }

    #[inline]
    fn rotate_side_priority_action(&mut self, player_context: &mut PlayerContext) -> bool {
        if let Some(action) = self.priority_actions_side_queue.pop_front() {
            debug_assert!(!player_context.has_priority_action());
            match action {
                RotatorAction::Single(action) => {
                    player_context.set_priority_action(None, action);
                }
                RotatorAction::Linked(_) => unreachable!(),
            }
            return true;
        }

        false
    }
}

impl Rotator for DefaultRotator {
    #[cfg_attr(test, concretize)]
    fn build_actions(&mut self, args: RotatorBuildArgs) {
        info!(target: "backend/rotator", "preparing actions {args:?}");
        let RotatorBuildArgs {
            mode,
            character_actions,
            map_actions,
            buffs,
            familiars,
            familiar_essence_key,
            elite_boss_behavior,
            elite_boss_behavior_key,
            hexa_booster_exchange_condition,
            hexa_booster_exchange_amount,
            hexa_booster_exchange_all,
            enable_panic_mode,
            enable_rune_solving,
            enable_transparent_shape_solving,
            enable_violetta_solving,
            enable_reset_normal_actions_on_erda,
            enable_using_generic_booster,
            enable_using_hexa_booster,
        } = args;
        self.reset_queue();
        self.normal_actions.clear();
        self.normal_rotate_mode = mode;
        self.normal_actions_reset_on_erda = enable_reset_normal_actions_on_erda;
        self.priority_actions.clear();

        // Low priority
        if enable_using_generic_booster {
            self.priority_actions.insert(
                next_action_id(),
                use_booster_priority_action(Booster::Generic),
            );
        }

        if enable_using_hexa_booster {
            self.priority_actions
                .insert(next_action_id(), use_booster_priority_action(Booster::Hexa));
        }

        if !matches!(
            hexa_booster_exchange_condition,
            ExchangeHexaBoosterCondition::None
        ) {
            self.priority_actions.insert(
                next_action_id(),
                exchange_hexa_booster_priority_action(
                    hexa_booster_exchange_condition,
                    hexa_booster_exchange_amount,
                    hexa_booster_exchange_all,
                ),
            );
        }

        if familiars.enable_familiars_swapping {
            self.priority_actions.insert(
                next_action_id(),
                familiars_swap_priority_action(
                    FamiliarsSwap {
                        swappable_slots: familiars.swappable_familiars,
                        swappable_rarities: Array::from_iter(familiars.swappable_rarities.clone()),
                    },
                    familiars.swap_check_millis,
                ),
            );
        }

        // Mid priority
        let mut i = 0;
        let actions = [character_actions, map_actions].concat();
        while i < actions.len() {
            let action = actions[i];
            let condition = action.condition();
            let queue_to_front = match action {
                Action::Move(_) => false,
                Action::Key(ActionKey { queue_to_front, .. }) => queue_to_front.unwrap_or_default(),
            };
            let (action, offset) = rotator_action(action, i, &actions);
            debug_assert!(i != 0 || !matches!(condition, ActionCondition::Linked));
            // Should not move i below the match because it could cause
            // infinite loop due to auto mobbing ignoring Any condition
            i += offset;
            match condition {
                ActionCondition::EveryMillis(_) | ActionCondition::ErdaShowerOffCooldown => {
                    self.priority_actions.insert(
                        next_action_id(),
                        priority_action(action, condition, queue_to_front),
                    );
                }
                ActionCondition::Any => {
                    if matches!(self.normal_rotate_mode, RotatorMode::AutoMobbing(_, _)) {
                        continue;
                    }
                    self.normal_actions.push((next_action_id(), action))
                }
                ActionCondition::Linked => unreachable!(),
            }
        }

        // High priority
        if enable_rune_solving {
            self.priority_actions
                .insert(next_action_id(), solve_rune_priority_action());
        }
        if enable_transparent_shape_solving {
            self.priority_actions
                .insert(next_action_id(), solve_transparent_shape_priority_action());
        }
        if enable_violetta_solving {
            self.priority_actions
                .insert(next_action_id(), solve_violetta_priority_action());
        }

        match elite_boss_behavior {
            EliteBossBehavior::None => (),
            EliteBossBehavior::CycleChannel => {
                self.priority_actions.insert(
                    next_action_id(),
                    elite_boss_change_channel_priority_action(),
                );
            }
            EliteBossBehavior::UseKey => {
                self.priority_actions.insert(
                    next_action_id(),
                    elite_boss_use_key_priority_action(elite_boss_behavior_key),
                );
            }
        }

        if enable_panic_mode {
            self.priority_actions
                .insert(next_action_id(), panic_priority_action());
        }

        if buffs
            .iter()
            .any(|(buff, _)| matches!(buff, BuffKind::Familiar))
        {
            self.priority_actions.insert(
                next_action_id(),
                familiar_essence_replenish_priority_action(familiar_essence_key),
            );
        }
        for (i, key) in buffs.iter().copied() {
            self.priority_actions
                .insert(next_action_id(), buff_priority_action(i, key));
        }

        self.priority_actions
            .insert(next_action_id(), unstuck_priority_action());
    }

    #[inline]
    fn reset_queue(&mut self) {
        self.normal_actions_backward = false;
        self.reset_normal_actions_queue();
        self.priority_actions_queue.clear();
        self.priority_queuing_linked_action = None;
        self.auto_mob_task = None;
        self.auto_mob_quadrant_consecutive_count = None;
    }

    #[inline]
    fn inject_action(&mut self, action: PlayerAction) {
        self.priority_actions_side_queue
            .push_back(RotatorAction::Single(action));
    }

    #[inline]
    fn rotate_action(&mut self, resources: &Resources, world: &mut World) {
        if resources.operation.halting() {
            if !has_side_loaded_action_executing(&world.player.context) {
                self.rotate_side_priority_action(&mut world.player.context);
            }
            return;
        }

        self.rotate_priority_actions(resources, world);
        self.rotate_priority_actions_queue(&mut world.player);

        match self.normal_rotate_mode {
            RotatorMode::StartToEnd => self.rotate_start_to_end(&mut world.player.context),
            RotatorMode::StartToEndThenReverse => {
                self.rotate_start_to_end_then_reverse(&mut world.player.context)
            }
            RotatorMode::AutoMobbing(key, bound) => self.rotate_auto_mobbing(
                resources,
                &mut world.player.context,
                world.minimap.state,
                key,
                bound,
            ),
            RotatorMode::PingPong(key, bound) => {
                self.rotate_ping_pong(&mut world.player.context, world.minimap.state, key, bound)
            }
        }
    }
}

#[inline]
fn has_side_loaded_action_executing(player_context: &PlayerContext) -> bool {
    player_context.has_priority_action() && player_context.priority_action_id().is_none()
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
        condition: Condition(Box::new(move |_, world, info| {
            if should_queue_fixed_action(world, info.last_queued_time, condition) {
                ConditionResult::Queue
            } else {
                ConditionResult::Skip
            }
        })),
        condition_kind: Some(condition),
        metadata: None,
        queue_to_front,
        queue_info: PriorityActionQueueInfo::default(),
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
fn familiar_essence_replenish_priority_action(key: KeyKind) -> PriorityAction {
    let mut task: Option<Task<Result<bool>>> = None;
    let task_fn = move |detector: Arc<dyn Detector>| -> Result<bool> {
        Ok(detector.detect_familiar_essence_depleted())
    };

    PriorityAction {
        condition: Condition(Box::new(move |resources, world, info| {
            if !at_least_millis_passed_since(info.last_queued_time, 20000) {
                return ConditionResult::Skip;
            }

            if !matches!(world.buffs[BuffKind::Familiar].state, Buff::Yes) {
                return ConditionResult::Skip;
            }

            match update_detection_task(resources, 10000, &mut task, task_fn) {
                Update::Ok(true) => ConditionResult::Queue,
                Update::Err(_) | Update::Ok(false) => ConditionResult::Ignore,
                Update::Pending => ConditionResult::Skip,
            }
        })),
        condition_kind: None,
        metadata: None,
        inner: RotatorAction::Single(PlayerAction::Key(Key {
            key,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::None,
            count: 1,
            position: None,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 5,
            wait_before_use_ticks_random_range: 0,
            wait_after_use_ticks: 0,
            wait_after_use_ticks_random_range: 0,
            wait_after_buffered: WaitAfterBuffered::None,
        })),
        queue_to_front: true,
        queue_info: PriorityActionQueueInfo::default(),
    }
}

#[inline]
fn familiars_swap_priority_action(swap: FamiliarsSwap, swap_check_millis: u64) -> PriorityAction {
    PriorityAction {
        condition: Condition(Box::new(move |_, world, info| {
            if !at_least_millis_passed_since(info.last_queued_time, swap_check_millis.into()) {
                return ConditionResult::Skip;
            }

            if world
                .player
                .context
                .is_familiars_swap_fail_count_limit_reached()
            {
                return ConditionResult::Skip;
            }

            ConditionResult::Queue
        })),
        condition_kind: None,
        metadata: None,
        inner: RotatorAction::Single(PlayerAction::FamiliarsSwap(swap)),
        queue_to_front: true,
        queue_info: PriorityActionQueueInfo::default(),
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
        condition: Condition(Box::new(|_, world, info| {
            if world.player.context.is_validating_rune() {
                return ConditionResult::Ignore;
            }

            if !at_least_millis_passed_since(info.last_queued_time, 10000) {
                return ConditionResult::Skip;
            }

            if let Minimap::Idle(idle) = world.minimap.state
                && idle.rune().is_some()
                && matches!(world.buffs[BuffKind::Rune].state, Buff::No)
            {
                return ConditionResult::Queue;
            }

            ConditionResult::Skip
        })),
        condition_kind: None,
        metadata: None,
        inner: RotatorAction::Single(PlayerAction::SolveRune),
        queue_to_front: true,
        queue_info: PriorityActionQueueInfo::default(),
    }
}

#[inline]
fn solve_transparent_shape_priority_action() -> PriorityAction {
    let mut task: Option<Task<Result<bool>>> = None;
    let task_fn = move |detector: Arc<dyn Detector>| -> Result<bool> {
        Ok(detector.detect_lie_detector_shape().is_ok())
    };

    PriorityAction {
        condition: Condition(Box::new(move |resources, _, _| {
            if resources.detector.is_none() {
                return ConditionResult::Ignore;
            }

            match update_detection_task(resources, 3000, &mut task, task_fn) {
                Update::Ok(true) => ConditionResult::Queue,
                Update::Err(_) | Update::Ok(false) => ConditionResult::Ignore,
                Update::Pending => ConditionResult::Skip,
            }
        })),
        condition_kind: None,
        metadata: None,
        inner: RotatorAction::Single(PlayerAction::SolveShape),
        queue_to_front: true,
        queue_info: PriorityActionQueueInfo::default(),
    }
}

#[inline]
fn solve_violetta_priority_action() -> PriorityAction {
    let mut task: Option<Task<Result<bool>>> = None;
    let task_fn = move |detector: Arc<dyn Detector>| -> Result<bool> {
        Ok(detector.detect_lie_detector_violetta().is_ok())
    };

    PriorityAction {
        condition: Condition(Box::new(move |resources, _, _| {
            if resources.detector.is_none() {
                return ConditionResult::Ignore;
            }

            match update_detection_task(resources, 3000, &mut task, task_fn) {
                Update::Ok(true) => ConditionResult::Queue,
                Update::Err(_) | Update::Ok(false) => ConditionResult::Ignore,
                Update::Pending => ConditionResult::Skip,
            }
        })),
        condition_kind: None,
        metadata: None,
        inner: RotatorAction::Single(PlayerAction::SolveVioletta),
        queue_to_front: true,
        queue_info: PriorityActionQueueInfo::default(),
    }
}

/// Creates a [`PlayerAction::Key`] priority action to cast a specific buff when it's not active.
///
/// The action queues if:
/// - Enough time has passed since the last queue attempt.
/// - The minimap is in the [`Minimap::Idle`] state.
/// - The specified buff is currently missing.
#[inline]
fn buff_priority_action(buff: BuffKind, key: KeyKind) -> PriorityAction {
    macro_rules! skip_if_has_buff {
        ($world:expr, $variant:ident $(| $variants:ident)*) => {{
            $(
                if !matches!($world.buffs[BuffKind::$variants].state, Buff::No) {
                    return ConditionResult::Skip;
                }
            )*
            if !matches!($world.buffs[BuffKind::$variant].state, Buff::No) {
                return ConditionResult::Skip;
            }
        }};
    }

    PriorityAction {
        condition: Condition(Box::new(move |_, world, info| {
            if !at_least_millis_passed_since(info.last_queued_time, 20000) {
                return ConditionResult::Skip;
            }
            if !matches!(world.minimap.state, Minimap::Idle(_)) {
                return ConditionResult::Skip;
            }

            match buff {
                BuffKind::SmallWealthAcquisitionPotion => {
                    skip_if_has_buff!(world, WealthAcquisitionPotion)
                }
                BuffKind::WealthAcquisitionPotion => {
                    skip_if_has_buff!(world, SmallWealthAcquisitionPotion)
                }
                BuffKind::SmallExpAccumulationPotion => {
                    skip_if_has_buff!(world, ExpAccumulationPotion)
                }
                BuffKind::ExpAccumulationPotion => {
                    skip_if_has_buff!(world, SmallExpAccumulationPotion)
                }
                BuffKind::ExpCouponX2 => {
                    skip_if_has_buff!(world, ExpCouponX3 | ExpCouponX4)
                }
                BuffKind::ExpCouponX3 => {
                    skip_if_has_buff!(world, ExpCouponX2 | ExpCouponX4)
                }
                BuffKind::ExpCouponX4 => {
                    skip_if_has_buff!(world, ExpCouponX3 | ExpCouponX2)
                }
                BuffKind::BonusExpCoupon => {
                    skip_if_has_buff!(world, MvpBonusExpCoupon)
                }
                _ => (),
            }

            if matches!(world.buffs[buff].state, Buff::No) {
                ConditionResult::Queue
            } else {
                ConditionResult::Skip
            }
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::Key(Key {
            key,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::None,
            count: 1,
            position: None,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 10,
            wait_before_use_ticks_random_range: 0,
            wait_after_use_ticks: 10,
            wait_after_use_ticks_random_range: 0,
            wait_after_buffered: WaitAfterBuffered::None,
        })),
        metadata: Some(ActionMetadata::Buff { kind: buff }),
        queue_to_front: true,
        queue_info: PriorityActionQueueInfo::default(),
    }
}

#[inline]
fn panic_priority_action() -> PriorityAction {
    PriorityAction {
        condition: Condition(Box::new(|_, world, info| match world.minimap.state {
            Minimap::Detecting => ConditionResult::Skip,
            Minimap::Idle(idle) => {
                if !idle.has_any_other_player() || info.last_queued_time.is_none() {
                    return ConditionResult::Ignore;
                }

                if at_least_millis_passed_since(info.last_queued_time, 15000) {
                    ConditionResult::Queue
                } else {
                    ConditionResult::Skip
                }
            }
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::Panic(Panic {
            to: PanicTo::Channel,
        })),
        metadata: None,
        queue_to_front: true,
        queue_info: PriorityActionQueueInfo::default(),
    }
}

#[inline]
fn elite_boss_change_channel_priority_action() -> PriorityAction {
    let mut condition = elite_boss_condition();

    PriorityAction {
        condition: Condition(Box::new(move |resources, _, info| {
            if !at_least_millis_passed_since(info.last_queued_time, 15000) {
                return ConditionResult::Skip;
            }

            condition(resources)
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::Panic(Panic {
            to: PanicTo::Channel,
        })),
        metadata: None,
        queue_to_front: true,
        queue_info: PriorityActionQueueInfo::default(),
    }
}

#[inline]
fn elite_boss_use_key_priority_action(key: KeyKind) -> PriorityAction {
    let mut condition = elite_boss_condition();

    PriorityAction {
        condition: Condition(Box::new(move |resources, _, info| {
            if !at_least_millis_passed_since(info.last_queued_time, 15000) {
                return ConditionResult::Skip;
            }

            condition(resources)
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::Key(Key {
            key,
            key_hold_ticks: 0,
            key_hold_buffered_to_wait_after: false,
            link_key: LinkKeyKind::None,
            count: 1,
            position: None,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 10,
            wait_before_use_ticks_random_range: 0,
            wait_after_use_ticks: 10,
            wait_after_use_ticks_random_range: 0,
            wait_after_buffered: WaitAfterBuffered::None,
        })),
        metadata: None,
        queue_to_front: true,
        queue_info: PriorityActionQueueInfo::default(),
    }
}

fn elite_boss_condition() -> impl FnMut(&Resources) -> ConditionResult {
    let mut task: Option<Task<Result<bool>>> = None;
    let task_fn =
        move |detector: Arc<dyn Detector>| -> Result<bool> { Ok(detector.detect_elite_boss_bar()) };

    move |resources| {
        if resources.detector.is_none() {
            return ConditionResult::Ignore;
        }

        match update_detection_task(resources, 5000, &mut task, task_fn) {
            Update::Ok(true) => ConditionResult::Queue,
            Update::Err(_) | Update::Ok(false) => ConditionResult::Ignore,
            Update::Pending => ConditionResult::Skip,
        }
    }
}

#[inline]
fn use_booster_priority_action(kind: Booster) -> PriorityAction {
    let mut task: Option<Task<Result<bool>>> = None;
    let task_fn =
        move |detector: Arc<dyn Detector>| -> Result<bool> { Ok(!detector.detect_timer_visible()) };

    PriorityAction {
        condition: Condition(Box::new(move |resources, world, info| {
            if !at_least_millis_passed_since(info.last_queued_time, 20000) {
                return ConditionResult::Skip;
            }

            if world
                .player
                .context
                .is_booster_fail_count_limit_reached(kind)
            {
                return ConditionResult::Ignore;
            }

            if resources.detector.is_none() {
                return ConditionResult::Ignore;
            }

            match update_detection_task(resources, 10000, &mut task, task_fn) {
                Update::Ok(true) => ConditionResult::Queue,
                Update::Err(_) | Update::Ok(false) => ConditionResult::Ignore,
                Update::Pending => ConditionResult::Skip,
            }
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::UseBooster(UseBooster { kind })),
        metadata: Some(ActionMetadata::UseBooster),
        queue_to_front: true,
        queue_info: PriorityActionQueueInfo::default(),
    }
}

#[inline]
fn exchange_hexa_booster_priority_action(
    condition: ExchangeHexaBoosterCondition,
    amount: u32,
    all: bool,
) -> PriorityAction {
    let mut task: Option<Task<Result<bool>>> = None;
    let task_fn = move |detector: Arc<dyn Detector>| -> Result<bool> {
        let booster = detector.detect_quick_slots_hexa_booster()?;
        if !matches!(booster, QuickSlotsHexaBooster::Unavailable) {
            return Ok(false);
        }

        let sol_erda = detector.detect_hexa_sol_erda()?;
        let queue = match condition {
            ExchangeHexaBoosterCondition::None => unreachable!(),
            ExchangeHexaBoosterCondition::Full => {
                matches!(sol_erda, SolErda::Full)
            }
            ExchangeHexaBoosterCondition::AtLeastOne => {
                matches!(sol_erda, SolErda::AtLeastOne | SolErda::Full)
            }
        };

        Ok(queue)
    };

    PriorityAction {
        condition: Condition(Box::new(move |resources, _, info| {
            if !at_least_millis_passed_since(info.last_queued_time, 20000) {
                return ConditionResult::Skip;
            }

            if resources.detector.is_none() {
                return ConditionResult::Skip;
            }

            match update_detection_task(resources, 10000, &mut task, task_fn) {
                Update::Ok(true) => ConditionResult::Queue,
                Update::Err(_) | Update::Ok(false) => ConditionResult::Ignore,
                Update::Pending => ConditionResult::Skip,
            }
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::ExchangeBooster(ExchangeBooster {
            amount,
            all,
        })),
        metadata: None,
        queue_to_front: true,
        queue_info: PriorityActionQueueInfo::default(),
    }
}

#[inline]
fn unstuck_priority_action() -> PriorityAction {
    let mut task: Option<Task<Result<bool>>> = None;
    let task_fn =
        move |detector: Arc<dyn Detector>| -> Result<bool> { Ok(detector.detect_esc_settings()) };

    PriorityAction {
        condition: Condition(Box::new(move |resources, world, info| {
            if !at_least_millis_passed_since(info.last_queued_time, 3000) {
                return ConditionResult::Skip;
            }

            if !world.player.state.can_override_current_state(None) {
                return ConditionResult::Skip;
            }

            if resources.detector.is_none() {
                return ConditionResult::Skip;
            }

            if world.player.context.is_dead() {
                return ConditionResult::Skip;
            }

            match update_detection_task(resources, 3000, &mut task, task_fn) {
                Update::Ok(true) => ConditionResult::Queue,
                Update::Ok(false) | Update::Err(_) | Update::Pending => ConditionResult::Skip,
            }
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::Unstuck),
        metadata: None,
        queue_to_front: true,
        queue_info: PriorityActionQueueInfo::default(),
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
    world: &World,
    last_queued_time: Option<Instant>,
    condition: ActionCondition,
) -> bool {
    let millis_should_passed = match condition {
        ActionCondition::EveryMillis(millis) => millis as u128,
        ActionCondition::ErdaShowerOffCooldown => 20000,
        ActionCondition::Linked | ActionCondition::Any => unreachable!(),
    };
    if !at_least_millis_passed_since(last_queued_time, millis_should_passed) {
        return false;
    }
    if matches!(condition, ActionCondition::ErdaShowerOffCooldown)
        && !matches!(world.skills[SkillKind::ErdaShower].state, Skill::Idle(_, _))
    {
        return false;
    }
    true
}

fn next_action_id() -> u32 {
    static NEXT_ID: AtomicU32 = AtomicU32::new(0);

    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use std::{
        assert_matches::assert_matches,
        time::{Duration, Instant},
    };

    use opencv::core::{Point, Vec4b};
    use strum::IntoEnumIterator;
    use tokio::{task::yield_now, time::timeout};

    use super::*;
    use crate::{
        Position,
        buff::{BuffContext, BuffEntity, BuffKind},
        detect::MockDetector,
        minimap::{MinimapContext, MinimapEntity, MinimapIdle},
        player::Player,
        skill::{SkillContext, SkillEntity, SkillKind},
    };

    const COOLDOWN_BETWEEN_QUEUE_MILLIS: u128 = 20_000;
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

    fn mock_world() -> World {
        World {
            minimap: MinimapEntity {
                state: Minimap::Detecting,
                context: MinimapContext::default(),
            },
            player: PlayerEntity {
                state: Player::Idle,
                context: PlayerContext::default(),
            },
            skills: SkillKind::iter()
                .map(|kind| SkillEntity {
                    state: Skill::Detecting,
                    context: SkillContext::new(kind),
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            buffs: BuffKind::iter()
                .map(|kind| BuffEntity {
                    state: Buff::No,
                    context: BuffContext::new(kind),
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        }
    }

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
        let world = mock_world();
        let now = Instant::now();

        assert!(should_queue_fixed_action(
            &world,
            Some(now - Duration::from_millis(3000)),
            ActionCondition::EveryMillis(2000)
        ));
        assert!(!should_queue_fixed_action(
            &world,
            Some(now - Duration::from_millis(1000)),
            ActionCondition::EveryMillis(2000)
        ));
    }

    #[test]
    fn rotator_should_queue_fixed_action_erda_shower() {
        let mut world = mock_world();
        let now = Instant::now();

        world.skills[SkillKind::ErdaShower].state = Skill::Idle(Point::default(), Vec4b::default());
        assert!(!should_queue_fixed_action(
            &world,
            Some(now - Duration::from_millis(COOLDOWN_BETWEEN_QUEUE_MILLIS as u64 - 1000)),
            ActionCondition::ErdaShowerOffCooldown
        ));
        assert!(should_queue_fixed_action(
            &world,
            Some(now - Duration::from_millis(COOLDOWN_BETWEEN_QUEUE_MILLIS as u64)),
            ActionCondition::ErdaShowerOffCooldown
        ));

        world.skills[SkillKind::ErdaShower].state = Skill::Detecting;
        assert!(!should_queue_fixed_action(
            &world,
            Some(now - Duration::from_millis(COOLDOWN_BETWEEN_QUEUE_MILLIS as u64)),
            ActionCondition::ErdaShowerOffCooldown
        ));
    }

    #[test]
    fn rotator_build_actions() {
        let mut rotator = DefaultRotator::default();
        let actions = vec![NORMAL_ACTION, NORMAL_ACTION, PRIORITY_ACTION];
        let buffs = vec![(BuffKind::Rune, KeyKind::A); 4];
        let args = RotatorBuildArgs {
            mode: RotatorMode::default(),
            map_actions: actions,
            character_actions: vec![],
            buffs,
            familiars: Familiars::default(),
            familiar_essence_key: KeyKind::A,
            elite_boss_behavior: EliteBossBehavior::CycleChannel,
            elite_boss_behavior_key: KeyKind::A,
            hexa_booster_exchange_condition: ExchangeHexaBoosterCondition::None,
            hexa_booster_exchange_amount: 1,
            hexa_booster_exchange_all: false,
            enable_panic_mode: true,
            enable_rune_solving: true,
            enable_transparent_shape_solving: true,
            enable_violetta_solving: true,
            enable_reset_normal_actions_on_erda: false,
            enable_using_generic_booster: false,
            enable_using_hexa_booster: false,
        };

        rotator.build_actions(args);
        assert_eq!(rotator.priority_actions.len(), 11);
        assert_eq!(rotator.normal_actions.len(), 2);
    }

    #[test]
    fn rotator_rotate_action_start_to_end_then_reverse() {
        let mut rotator = DefaultRotator::default();
        let mut world = mock_world();
        let resources = Resources::new(None, None);
        rotator.normal_rotate_mode = RotatorMode::StartToEndThenReverse;
        for i in 0..3 {
            rotator
                .normal_actions
                .push((i, RotatorAction::Single(NORMAL_ACTION.into())));
        }

        rotator.rotate_action(&resources, &mut world);
        assert_eq!(world.player.context.normal_action_id(), Some(0));
        assert!(!rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 1);

        world.player.context.clear_actions_aborted(true);
        rotator.rotate_action(&resources, &mut world);
        assert_eq!(world.player.context.normal_action_id(), Some(1));
        assert!(!rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 2);

        world.player.context.clear_actions_aborted(true);
        rotator.rotate_action(&resources, &mut world);
        assert_eq!(world.player.context.normal_action_id(), Some(2));
        assert!(rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 1);

        world.player.context.clear_actions_aborted(true);
        rotator.rotate_action(&resources, &mut world);
        assert_eq!(world.player.context.normal_action_id(), Some(1));
        assert!(rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 2);

        world.player.context.clear_actions_aborted(true);
        rotator.rotate_action(&resources, &mut world);
        assert_eq!(world.player.context.normal_action_id(), Some(0));
        assert!(!rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 1);
    }

    #[test]
    fn rotator_rotate_action_start_to_end() {
        let mut world = mock_world();
        let mut rotator = DefaultRotator::default();
        let resources = Resources::new(None, None);
        rotator.normal_rotate_mode = RotatorMode::StartToEnd;
        for i in 0..2 {
            rotator
                .normal_actions
                .push((i, RotatorAction::Single(NORMAL_ACTION.into())));
        }

        rotator.rotate_action(&resources, &mut world);
        assert!(world.player.context.has_normal_action());
        assert!(!rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 1);

        world.player.context.clear_actions_aborted(true);

        rotator.rotate_action(&resources, &mut world);
        assert!(world.player.context.has_normal_action());
        assert!(!rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 0);
    }

    #[test]
    fn rotator_priority_actions_queue() {
        let mut rotator = DefaultRotator::default();
        let mut minimap = MinimapIdle::default();
        minimap.set_rune(Point::default());
        let mut world = mock_world();
        world.minimap.state = Minimap::Idle(minimap);
        world.buffs[BuffKind::Rune].state = Buff::No;
        rotator.priority_actions.insert(
            55,
            PriorityAction {
                condition: Condition(Box::new(|_, world, _| {
                    if matches!(world.minimap.state, Minimap::Idle(_)) {
                        ConditionResult::Queue
                    } else {
                        ConditionResult::Skip
                    }
                })),
                condition_kind: None,
                inner: RotatorAction::Single(PlayerAction::SolveRune),
                metadata: None,
                queue_to_front: true,
                queue_info: PriorityActionQueueInfo::default(),
            },
        );
        let resources = Resources::new(None, None);

        rotator.rotate_action(&resources, &mut world);
        assert_eq!(rotator.priority_actions_queue.len(), 0);
        assert_eq!(world.player.context.priority_action_id(), Some(55));
    }

    #[test]
    fn rotator_priority_actions_queue_to_front() {
        let mut rotator = DefaultRotator::default();
        let mut world = mock_world();
        let resources = Resources::new(None, None);
        // queue 2 non-front priority actions
        rotator.priority_actions.insert(
            2,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| ConditionResult::Queue)),
                condition_kind: None,
                inner: RotatorAction::Single(NORMAL_ACTION.into()),
                metadata: None,
                queue_to_front: false,
                queue_info: PriorityActionQueueInfo::default(),
            },
        );
        rotator.priority_actions.insert(
            3,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| ConditionResult::Queue)),
                condition_kind: None,
                inner: RotatorAction::Single(NORMAL_ACTION.into()),
                metadata: None,
                queue_to_front: false,
                queue_info: PriorityActionQueueInfo::default(),
            },
        );

        rotator.rotate_action(&resources, &mut world);
        assert_eq!(rotator.priority_actions_queue.len(), 1);
        assert_eq!(world.player.context.priority_action_id(), Some(2));

        // add 1 front priority action
        rotator.priority_actions.insert(
            4,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| ConditionResult::Queue)),
                condition_kind: None,
                inner: RotatorAction::Single(NORMAL_ACTION.into()),
                metadata: None,
                queue_to_front: true,
                queue_info: PriorityActionQueueInfo::default(),
            },
        );

        // non-front priority action get replaced
        rotator.rotate_action(&resources, &mut world);
        assert_eq!(
            rotator.priority_actions_queue,
            VecDeque::from_iter([2, 3].into_iter())
        );
        assert_eq!(world.player.context.priority_action_id(), Some(4));

        // add another front priority action
        rotator.priority_actions.insert(
            5,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| ConditionResult::Queue)),
                condition_kind: None,
                inner: RotatorAction::Single(NORMAL_ACTION.into()),
                metadata: None,
                queue_to_front: true,
                queue_info: PriorityActionQueueInfo::default(),
            },
        );

        // queued front priority action cannot be replaced
        // by another front priority action
        rotator.rotate_action(&resources, &mut world);
        assert_eq!(
            rotator.priority_actions_queue,
            VecDeque::from_iter([5, 2, 3].into_iter())
        );
        assert_eq!(world.player.context.priority_action_id(), Some(4));
    }

    #[test]
    fn rotator_priority_linked_action() {
        let mut rotator = DefaultRotator::default();
        let mut world = mock_world();
        let resources = Resources::new(None, None);
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
                metadata: None,
                queue_to_front: false,
                queue_info: PriorityActionQueueInfo::default(),
            },
        );

        // linked action queued
        rotator.rotate_action(&resources, &mut world);
        assert!(rotator.priority_actions_queue.is_empty());
        assert!(rotator.priority_queuing_linked_action.is_some());
        assert_eq!(world.player.context.priority_action_id(), Some(2));

        // linked action cannot be replaced by queue to front
        rotator.priority_actions.insert(
            4,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| ConditionResult::Queue)),
                condition_kind: None,
                inner: RotatorAction::Single(PlayerAction::SolveRune),
                metadata: None,
                queue_to_front: true,
                queue_info: PriorityActionQueueInfo::default(),
            },
        );
        rotator.rotate_action(&resources, &mut world);
        assert_eq!(
            rotator.priority_actions_queue,
            VecDeque::from_iter([4].into_iter())
        );

        world.player.context.clear_actions_aborted(true);
        rotator.rotate_action(&resources, &mut world);
        assert!(rotator.priority_queuing_linked_action.is_none());
        assert_eq!(
            rotator.priority_actions_queue,
            VecDeque::from_iter([4].into_iter())
        );
        assert_eq!(world.player.context.priority_action_id(), Some(2));
    }

    #[test]
    fn rotate_ping_pong_direction() {
        let mut player = PlayerContext::default();
        let mut rotator = DefaultRotator::default();
        let mut idle = MinimapIdle::default();
        idle.bbox = Rect::new(0, 0, 100, 100); // x: [0, 100]

        // Closer to right, further than left -> Go left
        player.last_known_pos = Some(Point::new(80, 50));
        rotator.rotate_ping_pong(
            &mut player,
            Minimap::Idle(idle),
            MobbingKey::default(),
            Rect::new(20, 20, 80, 80).into(),
        );

        assert_matches!(
            player.normal_action(),
            Some(PlayerAction::PingPong(PingPong {
                direction: PingPongDirection::Left,
                ..
            }))
        );

        // Closer to left, further than right -> Go right
        player.clear_actions_aborted(true);
        player.last_known_pos = Some(Point::new(10, 50));
        rotator.rotate_ping_pong(
            &mut player,
            Minimap::Idle(idle),
            MobbingKey::default(),
            Rect::new(20, 20, 80, 80).into(),
        );

        assert_matches!(
            player.normal_action(),
            Some(PlayerAction::PingPong(PingPong {
                direction: PingPongDirection::Right,
                ..
            }))
        );
    }

    #[test]
    fn rotator_priority_action_is_ignored_when_executing() {
        let mut rotator = DefaultRotator::default();
        let mut world = mock_world();
        let resources = Resources::new(None, None);

        // Insert a priority action with condition_kind = None
        let action_id = 99;
        rotator.priority_actions.insert(
            action_id,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| panic!("should not be called"))),
                condition_kind: None,
                inner: RotatorAction::Single(NORMAL_ACTION.into()),
                metadata: None,
                queue_to_front: false,
                queue_info: PriorityActionQueueInfo::default(),
            },
        );
        // Simulate the action is currently being executed by the player
        world
            .player
            .context
            .set_priority_action(Some(action_id), NORMAL_ACTION.into());

        // Call rotate_priority_actions
        rotator.rotate_priority_actions(&resources, &mut world);

        let action = rotator.priority_actions.get(&action_id).unwrap();

        // Assert the action was marked as ignored
        assert!(action.queue_info.ignoring);
        assert!(action.queue_info.last_queued_time.is_some());

        // It should not be in the queue
        assert!(!rotator.priority_actions_queue.contains(&action_id));
    }

    #[test]
    fn rotator_priority_linked_action_is_ignored_when_executing() {
        let mut rotator = DefaultRotator::default();
        let mut world = mock_world();
        let resources = Resources::new(None, None);

        let action_id = 42;
        rotator.priority_actions.insert(
            action_id,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| panic!("should not be called"))),
                condition_kind: Some(ActionCondition::Linked),
                inner: RotatorAction::Linked(LinkedAction {
                    inner: NORMAL_ACTION.into(),
                    next: None,
                }),
                metadata: None,
                queue_to_front: false,
                queue_info: PriorityActionQueueInfo::default(),
            },
        );

        // Simulate action is being executed
        world
            .player
            .context
            .set_priority_action(Some(action_id), NORMAL_ACTION.into());

        rotator.rotate_priority_actions(&resources, &mut world);

        let action = rotator.priority_actions.get(&action_id).unwrap();

        assert!(action.queue_info.ignoring);
        assert!(action.queue_info.last_queued_time.is_some());
        assert!(!rotator.priority_actions_queue.contains(&action_id));
    }

    #[test]
    fn rotator_erda_shower_action_ignored_if_another_erda_is_queued() {
        let mut rotator = DefaultRotator::default();
        let mut world = mock_world();
        let resources = Resources::new(None, None);

        let first_erda_id = 1;
        let second_erda_id = 2;

        rotator.priority_actions.insert(
            first_erda_id,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| ConditionResult::Queue)),
                condition_kind: Some(ActionCondition::ErdaShowerOffCooldown),
                inner: RotatorAction::Single(NORMAL_ACTION.into()),
                metadata: None,
                queue_to_front: false,
                queue_info: PriorityActionQueueInfo {
                    last_queued_time: Some(Instant::now()),
                    ..Default::default()
                },
            },
        );

        rotator.priority_actions.insert(
            second_erda_id,
            PriorityAction {
                condition: Condition(Box::new(|_, _, _| panic!("should not be called"))),
                condition_kind: Some(ActionCondition::ErdaShowerOffCooldown),
                inner: RotatorAction::Single(NORMAL_ACTION.into()),
                metadata: None,
                queue_to_front: false,
                queue_info: PriorityActionQueueInfo::default(),
            },
        );

        // Queue the first erda action manually
        rotator.priority_actions_queue.push_back(first_erda_id);

        // Run rotate
        rotator.rotate_priority_actions(&resources, &mut world);

        let second_erda = rotator.priority_actions.get(&second_erda_id).unwrap();

        assert!(second_erda.queue_info.ignoring);
        assert!(second_erda.queue_info.last_queued_time.is_some());
        assert!(!rotator.priority_actions_queue.contains(&second_erda_id));
    }

    fn mock_detector(f: fn(&mut MockDetector)) -> MockDetector {
        let mut detector = MockDetector::new();

        f(&mut detector);

        detector
    }

    async fn queue_or_timeout(mut f: impl FnMut() -> ConditionResult) {
        timeout(Duration::from_secs(1), async move {
            loop {
                let result = f();
                if matches!(result, ConditionResult::Queue) {
                    break;
                }

                yield_now().await;
            }
        })
        .await
        .expect("queue result");
    }

    #[tokio::test]
    async fn unstuck_priority_action_triggers_when_esc_settings_detected() {
        let resources = Resources::new(
            None,
            Some(mock_detector(|detector| {
                detector.expect_detect_esc_settings().returning(|| true);
                detector.expect_detect_player_is_dead().returning(|| false);
            })),
        );
        let world = mock_world();
        let info = PriorityActionQueueInfo::default();
        let mut action = unstuck_priority_action();

        queue_or_timeout(|| (action.condition.0)(&resources, &world, &info)).await;
    }

    #[tokio::test]
    async fn elite_boss_use_key_priority_action_triggers_when_elite_present() {
        let detector = mock_detector(|detector| {
            detector.expect_detect_elite_boss_bar().return_const(true);
        });
        let resources = Resources::new(None, Some(detector));
        let world = mock_world();

        let mut action = elite_boss_use_key_priority_action(KeyKind::A);
        let info = PriorityActionQueueInfo::default();

        queue_or_timeout(|| (action.condition.0)(&resources, &world, &info)).await;
    }

    #[tokio::test]
    async fn panic_priority_action_triggers_when_has_other_players() {
        let resources = Resources::new(None, None);
        let mut idle = MinimapIdle::default();
        idle.set_has_any_other_player(true);
        let mut world = mock_world();
        world.minimap.state = Minimap::Idle(idle);

        let mut action = panic_priority_action();
        let info = PriorityActionQueueInfo {
            last_queued_time: Some(Instant::now() - std::time::Duration::from_millis(16000)),
            ..Default::default()
        };

        queue_or_timeout(|| (action.condition.0)(&resources, &world, &info)).await;
    }

    // TODO: more tests
}
