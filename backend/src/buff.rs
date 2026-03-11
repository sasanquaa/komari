use std::{
    mem,
    ops::{Index, IndexMut},
};

use anyhow::Result;
use strum::EnumIter;

use crate::{
    Character, Settings,
    detect::BuffKind as DetectorBuffKind,
    ecs::Resources,
    player::Player,
    task::{Task, Update, update_detection_task},
};

const COMMON_FAIL_COUNT: u32 = 5;
const FAMILIAR_FAIL_COUNT: u32 = 2;
const RUNE_FAIL_COUNT: u32 = 1;

// An entity for buff.
#[derive(Debug)]
pub struct BuffEntity {
    pub state: Buff,
    pub context: BuffContext,
}

pub type BuffEntities = [BuffEntity; BuffKind::COUNT];

/// Stores states of a buff.
#[derive(Debug)]
pub struct BuffContext {
    /// The kind of buff.
    kind: BuffKind,
    /// Task for detecting if the coresponding buff exists.
    task: Option<Task<Result<bool>>>,
    /// The number of time [`Buff::Volatile`] has failed to detect if the buff exists.
    fail_count: u32,
    /// The maximum number of time [`Buff::Volatile`] can fail before transitioning
    /// to [`Buff:No`].
    max_fail_count: u32,
    /// Whether a buff is enabled.
    enabled: bool,
}

impl BuffContext {
    pub fn new(kind: BuffKind) -> Self {
        Self {
            kind,
            task: None,
            fail_count: 0,
            max_fail_count: match kind {
                BuffKind::Rune => RUNE_FAIL_COUNT,
                BuffKind::Familiar => FAMILIAR_FAIL_COUNT,
                BuffKind::LegionWealth
                | BuffKind::LegionLuck
                | BuffKind::WealthAcquisitionPotion
                | BuffKind::ExpAccumulationPotion
                | BuffKind::SmallWealthAcquisitionPotion
                | BuffKind::SmallExpAccumulationPotion
                | BuffKind::SayramElixir
                | BuffKind::AureliaElixir
                | BuffKind::ExpCouponX2
                | BuffKind::ExpCouponX3
                | BuffKind::ExpCouponX4
                | BuffKind::BonusExpCoupon
                | BuffKind::MvpBonusExpCoupon
                | BuffKind::ForTheGuild
                | BuffKind::HardHitter
                | BuffKind::ExtremeRedPotion
                | BuffKind::ExtremeBluePotion
                | BuffKind::ExtremeGreenPotion
                | BuffKind::ExtremeGoldPotion => COMMON_FAIL_COUNT,
            },
            enabled: true,
        }
    }

    /// Updates the enabled states of each buff to only detect if enabled.
    pub fn update_enabled_state(&mut self, character: &Character, settings: &Settings) {
        self.enabled = match self.kind {
            BuffKind::Rune => settings.enable_rune_solving,
            BuffKind::Familiar => character.familiar_buff_key.enabled,
            BuffKind::SayramElixir => character.sayram_elixir_key.enabled,
            BuffKind::AureliaElixir => character.aurelia_elixir_key.enabled,
            BuffKind::ExpCouponX2 | BuffKind::ExpCouponX3 | BuffKind::ExpCouponX4 => {
                character.exp_x2_key.enabled
                    || character.exp_x3_key.enabled
                    || character.exp_x4_key.enabled
            }
            BuffKind::BonusExpCoupon | BuffKind::MvpBonusExpCoupon => {
                character.bonus_exp_key.enabled
            }
            BuffKind::LegionWealth => character.legion_wealth_key.enabled,
            BuffKind::LegionLuck => character.legion_luck_key.enabled,
            BuffKind::WealthAcquisitionPotion | BuffKind::SmallWealthAcquisitionPotion => {
                character.wealth_acquisition_potion_key.enabled
                    || character.small_wealth_acquisition_potion_key.enabled
            }
            BuffKind::ExpAccumulationPotion | BuffKind::SmallExpAccumulationPotion => {
                character.exp_accumulation_potion_key.enabled
                    || character.small_exp_accumulation_potion_key.enabled
            }
            BuffKind::ForTheGuild => character.for_the_guild_key.enabled,
            BuffKind::HardHitter => character.hard_hitter_key.enabled,
            BuffKind::ExtremeRedPotion => character.extreme_red_potion_key.enabled,
            BuffKind::ExtremeBluePotion => character.extreme_blue_potion_key.enabled,
            BuffKind::ExtremeGreenPotion => character.extreme_green_potion_key.enabled,
            BuffKind::ExtremeGoldPotion => character.extreme_gold_potion_key.enabled,
        };
        if !self.enabled {
            self.fail_count = 0;
            self.task = None;
        }
    }
}

// The kind of buff.
#[derive(Clone, Copy, Debug, EnumIter)]
#[cfg_attr(test, derive(PartialEq))]
#[repr(usize)]
pub enum BuffKind {
    // NOTE: Upon failing to solving rune, there is a cooldown
    // that looks exactly like the normal rune buff.
    Rune,
    Familiar,
    SayramElixir,
    AureliaElixir,
    ExpCouponX2,
    ExpCouponX3,
    ExpCouponX4,
    BonusExpCoupon,
    MvpBonusExpCoupon,
    LegionWealth,
    LegionLuck,
    WealthAcquisitionPotion,
    ExpAccumulationPotion,
    SmallWealthAcquisitionPotion,
    SmallExpAccumulationPotion,
    ForTheGuild,
    HardHitter,
    ExtremeRedPotion,
    ExtremeBluePotion,
    ExtremeGreenPotion,
    ExtremeGoldPotion,
}

impl BuffKind {
    pub const COUNT: usize = mem::variant_count::<BuffKind>();
}

impl Index<BuffKind> for BuffEntities {
    type Output = BuffEntity;

    fn index(&self, index: BuffKind) -> &Self::Output {
        self.get(index as usize).unwrap()
    }
}

impl IndexMut<BuffKind> for BuffEntities {
    fn index_mut(&mut self, index: BuffKind) -> &mut Self::Output {
        self.get_mut(index as usize).unwrap()
    }
}

impl From<BuffKind> for DetectorBuffKind {
    fn from(kind: BuffKind) -> Self {
        match kind {
            BuffKind::Rune => DetectorBuffKind::Rune,
            BuffKind::Familiar => DetectorBuffKind::Familiar,
            BuffKind::SayramElixir => DetectorBuffKind::SayramElixir,
            BuffKind::AureliaElixir => DetectorBuffKind::AureliaElixir,
            BuffKind::ExpCouponX2 => DetectorBuffKind::ExpCouponX2,
            BuffKind::ExpCouponX3 => DetectorBuffKind::ExpCouponX3,
            BuffKind::ExpCouponX4 => DetectorBuffKind::ExpCouponX4,
            BuffKind::BonusExpCoupon => DetectorBuffKind::BonusExpCoupon,
            BuffKind::MvpBonusExpCoupon => DetectorBuffKind::MvpBonusExpCoupon,
            BuffKind::LegionWealth => DetectorBuffKind::LegionWealth,
            BuffKind::LegionLuck => DetectorBuffKind::LegionLuck,
            BuffKind::WealthAcquisitionPotion => DetectorBuffKind::WealthAcquisitionPotion,
            BuffKind::ExpAccumulationPotion => DetectorBuffKind::ExpAccumulationPotion,
            BuffKind::SmallWealthAcquisitionPotion => {
                DetectorBuffKind::SmallWealthAcquisitionPotion
            }
            BuffKind::SmallExpAccumulationPotion => DetectorBuffKind::SmallExpAccumulationPotion,
            BuffKind::ForTheGuild => DetectorBuffKind::ForTheGuild,
            BuffKind::HardHitter => DetectorBuffKind::HardHitter,
            BuffKind::ExtremeRedPotion => DetectorBuffKind::ExtremeRedPotion,
            BuffKind::ExtremeBluePotion => DetectorBuffKind::ExtremeBluePotion,
            BuffKind::ExtremeGreenPotion => DetectorBuffKind::ExtremeGreenPotion,
            BuffKind::ExtremeGoldPotion => DetectorBuffKind::ExtremeGoldPotion,
        }
    }
}

/// Buff contextual state.
#[derive(Clone, Copy, Debug)]
pub enum Buff {
    /// Player does not have this [`BuffKind`].
    No,
    /// Player has this [`BuffKind`].
    Yes,
    /// Player did have this [`BuffKind`] but currently unsure.
    Volatile,
}

#[inline]
pub fn run_system(resources: &Resources, buff: &mut BuffEntity, player_state: Player) {
    if !buff.context.enabled {
        buff.state = Buff::No;
        return;
    }
    if matches!(player_state, Player::CashShopThenExit(_)) {
        return;
    }

    let kind = buff.context.kind;
    let Update::Ok(has_buff) =
        update_detection_task(resources, 5000, &mut buff.context.task, move |detector| {
            Ok(detector.detect_player_buff(kind.into()))
        })
    else {
        return;
    };

    let is_volatile = matches!(buff.state, Buff::Volatile);
    buff.context.fail_count = if is_volatile && !has_buff {
        buff.context.fail_count + 1
    } else {
        0
    };

    let count = buff.context.fail_count;
    let max_count = buff.context.max_fail_count;
    match (has_buff, buff.state) {
        (true, Buff::Volatile) | (true, Buff::Yes) | (true, Buff::No) => {
            buff.state = Buff::Yes;
        }
        (false, Buff::No) => {
            buff.state = Buff::No;
        }
        (false, Buff::Yes) => {
            buff.state = if max_count > 1 {
                Buff::Volatile
            } else {
                Buff::No
            };
        }
        (false, Buff::Volatile) => {
            buff.state = if count >= max_count {
                Buff::No
            } else {
                Buff::Volatile
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;
    use std::mem::discriminant;
    use std::time::Duration;

    use strum::IntoEnumIterator;
    use tokio::time::advance;

    use super::*;
    use crate::detect::MockDetector;
    use crate::ecs::Resources;

    fn detector_with_kind(kind: BuffKind, result: bool) -> MockDetector {
        let mut detector = MockDetector::new();
        detector
            .expect_detect_player_buff()
            .withf(move |detector_kind| {
                discriminant(detector_kind) == discriminant(&DetectorBuffKind::from(kind))
            })
            .return_const(result);
        detector
    }

    async fn run_system_until_task_completed(resources: &Resources, buff: &mut BuffEntity) {
        while !buff
            .context
            .task
            .as_ref()
            .is_some_and(|task| task.completed())
        {
            run_system(resources, buff, Player::Idle);
            advance(Duration::from_millis(1000)).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn run_system_no_to_yes() {
        for kind in BuffKind::iter() {
            let detector = detector_with_kind(kind, true);
            let resources = Resources::new(None, Some(detector));
            let mut buff = BuffEntity {
                state: Buff::No,
                context: BuffContext::new(kind),
            };

            run_system_until_task_completed(&resources, &mut buff).await;

            assert_matches!(buff.state, Buff::Yes);
            assert_eq!(buff.context.fail_count, 0);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn run_system_yes_to_no() {
        for kind in BuffKind::iter() {
            let detector = detector_with_kind(kind, false);
            let resources = Resources::new(None, Some(detector));
            let mut buff = BuffEntity {
                state: Buff::Yes,
                context: BuffContext::new(kind),
            };
            buff.context.max_fail_count = 2;

            // First failure: Yes -> Volatile
            run_system_until_task_completed(&resources, &mut buff).await;
            assert_matches!(buff.state, Buff::Volatile);
            assert_eq!(buff.context.fail_count, 0);

            // Second failure: Volatile -> still Volatile
            buff.context.task = None;
            run_system_until_task_completed(&resources, &mut buff).await;
            assert_matches!(buff.state, Buff::Volatile);
            assert_eq!(buff.context.fail_count, 1);

            // Third failure: Volatile -> No (fail_count reached max)
            buff.context.task = None;
            run_system_until_task_completed(&resources, &mut buff).await;
            assert_matches!(buff.state, Buff::No);
            assert_eq!(buff.context.fail_count, 2);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn run_system_volatile_to_yes() {
        for kind in BuffKind::iter() {
            let detector = detector_with_kind(kind, true);
            let resources = Resources::new(None, Some(detector));
            let mut buff = BuffEntity {
                state: Buff::Volatile,
                context: BuffContext::new(kind),
            };
            buff.context.max_fail_count = 3;
            buff.context.fail_count = 2;

            run_system_until_task_completed(&resources, &mut buff).await;

            assert_matches!(buff.state, Buff::Yes);
            assert_eq!(buff.context.fail_count, 0);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn run_system_volatile_stay_before_threshold() {
        for kind in BuffKind::iter() {
            let detector = detector_with_kind(kind, false);
            let resources = Resources::new(None, Some(detector));
            let mut buff = BuffEntity {
                state: Buff::Volatile,
                context: BuffContext::new(kind),
            };
            buff.context.max_fail_count = 3;
            buff.context.fail_count = 1;

            run_system_until_task_completed(&resources, &mut buff).await;

            assert_matches!(buff.state, Buff::Volatile);
            assert_eq!(buff.context.fail_count, 2);
        }
    }

    #[test]
    fn update_enabled_state_reset_on_disabled() {
        let kind = BuffKind::Rune;
        let mut state = BuffContext::new(kind);
        state.enabled = true;
        state.fail_count = 5;

        let mut settings = Settings::default();
        let config = Character::default();
        settings.enable_rune_solving = false;

        state.update_enabled_state(&config, &settings);

        assert!(!state.enabled);
        assert_eq!(state.fail_count, 0);
        assert!(state.task.is_none());
    }
}
