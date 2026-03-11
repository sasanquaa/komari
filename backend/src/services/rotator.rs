use std::fmt::Debug;

use strum::IntoEnumIterator;

use crate::bridge::KeyKind;
use crate::rotator::{Rotator, RotatorMode};
use crate::{
    Action, Character, KeyBinding, Map, RotationMode, Settings, buff::BuffKind,
    rotator::RotatorBuildArgs,
};
use crate::{
    ActionCondition, ActionConfigurationCondition, ActionKey, KeyBindingConfiguration, PotionMode,
};

/// A service to handle [`Rotator`]-related incoming requests.
pub trait RotatorService: Debug {
    /// Updates the current [`RotatorBuildArgs`] with new data from `map` and `preset`.
    fn update_from_map(&mut self, map: Option<&Map>, preset: Option<String>);

    /// Updates the current [`RotatorBuildArgs`] with new data from `character`.
    fn update_from_characters(&mut self, character: Option<&Character>);

    /// Updates the current [`RotatorBuildArgs`] with new data from `settings`.
    fn update_from_settings(&mut self, settings: &Settings);

    /// Updates `rotator` with the currently in-use [`RotatorBuildArgs`].
    fn apply(&self, rotator: &mut dyn Rotator);
}

#[derive(Debug, Default)]
pub struct DefaultRotatorService {
    args: RotatorBuildArgs,
}

impl RotatorService for DefaultRotatorService {
    fn update_from_map(&mut self, map: Option<&Map>, preset: Option<String>) {
        self.args.map_actions = map
            .zip(preset)
            .and_then(|(minimap, preset)| minimap.actions.get(&preset).cloned())
            .unwrap_or_default();
        self.args.mode = rotator_mode_from(map);
        self.args.enable_reset_normal_actions_on_erda = map
            .map(|map| map.actions_any_reset_on_erda_condition)
            .unwrap_or_default();
    }

    fn update_from_characters(&mut self, character: Option<&Character>) {
        self.args.character_actions = character.map(actions_from).unwrap_or_default();
        self.args.buffs = character.map(buffs_from).unwrap_or_default();
        self.args.familiars = character
            .map(|character| character.familiars.clone())
            .unwrap_or_default();
        self.args.familiar_essence_key = character
            .map(|character| character.familiar_essence_key.key)
            .unwrap_or_default()
            .into();
        self.args.elite_boss_behavior = character
            .map(|character| character.elite_boss_behavior)
            .unwrap_or_default();
        self.args.elite_boss_behavior_key = character
            .map(|character| character.elite_boss_behavior_key)
            .unwrap_or_default()
            .into();
        self.args.hexa_booster_exchange_condition = character
            .map(|character| character.hexa_booster_exchange_condition)
            .unwrap_or_default();
        self.args.hexa_booster_exchange_amount = character
            .map(|character| character.hexa_booster_exchange_amount)
            .unwrap_or(1);
        self.args.hexa_booster_exchange_all = character
            .map(|character| character.hexa_booster_exchange_all)
            .unwrap_or_default();
        self.args.enable_using_generic_booster = character
            .map(|character| character.generic_booster_key.enabled)
            .unwrap_or_default();
        self.args.enable_using_hexa_booster = character
            .map(|character| character.hexa_booster_key.enabled)
            .unwrap_or_default();
    }

    fn update_from_settings(&mut self, settings: &Settings) {
        self.args.enable_panic_mode = settings.enable_panic_mode;
        self.args.enable_rune_solving = settings.enable_rune_solving;
        self.args.enable_transparent_shape_solving = settings.enable_transparent_shape_solving;
        self.args.enable_violetta_solving = settings.enable_violetta_solving;
    }

    fn apply(&self, rotator: &mut dyn Rotator) {
        rotator.build_actions(self.args.clone());
    }
}

#[inline]
fn rotator_mode_from(map: Option<&Map>) -> RotatorMode {
    map.map(|map| match map.rotation_mode {
        RotationMode::StartToEnd => RotatorMode::StartToEnd,
        RotationMode::StartToEndThenReverse => RotatorMode::StartToEndThenReverse,
        RotationMode::AutoMobbing => {
            RotatorMode::AutoMobbing(map.rotation_mobbing_key, map.rotation_auto_mob_bound)
        }
        RotationMode::PingPong => {
            RotatorMode::PingPong(map.rotation_mobbing_key, map.rotation_ping_pong_bound)
        }
    })
    .unwrap_or_default()
}

fn actions_from(character: &Character) -> Vec<Action> {
    fn make_key_action(key: KeyBinding, millis: u64, count: u32) -> Action {
        Action::Key(ActionKey {
            key,
            count,
            condition: ActionCondition::EveryMillis(millis),
            wait_before_use_millis: 350,
            wait_after_use_millis: 350,
            ..ActionKey::default()
        })
    }

    let mut vec = Vec::new();

    if let KeyBindingConfiguration { key, enabled: true } = character.feed_pet_key {
        vec.push(make_key_action(
            key,
            character.feed_pet_millis,
            character.feed_pet_count,
        ));
    }

    if let KeyBindingConfiguration { key, enabled: true } = character.potion_key
        && let PotionMode::EveryMillis(millis) = character.potion_mode
    {
        vec.push(make_key_action(key, millis, 1));
    }

    let mut iter = character.actions.clone().into_iter().peekable();
    while let Some(action) = iter.next() {
        if !action.enabled || matches!(action.condition, ActionConfigurationCondition::Linked) {
            continue;
        }

        vec.push(action.into());
        while let Some(next) = iter.peek() {
            if !matches!(next.condition, ActionConfigurationCondition::Linked) {
                break;
            }

            vec.push((*next).into());
            iter.next();
        }
    }

    vec
}

fn buffs_from(character: &Character) -> Vec<(BuffKind, KeyKind)> {
    BuffKind::iter()
        .filter_map(|kind| {
            let enabled_key = match kind {
                BuffKind::Rune => None, // Internal buff
                BuffKind::Familiar => character
                    .familiar_buff_key
                    .enabled
                    .then_some(character.familiar_buff_key.key.into()),
                BuffKind::SayramElixir => character
                    .sayram_elixir_key
                    .enabled
                    .then_some(character.sayram_elixir_key.key.into()),
                BuffKind::AureliaElixir => character
                    .aurelia_elixir_key
                    .enabled
                    .then_some(character.aurelia_elixir_key.key.into()),
                BuffKind::ExpCouponX2 => character
                    .exp_x2_key
                    .enabled
                    .then_some(character.exp_x2_key.key.into()),
                BuffKind::ExpCouponX3 => character
                    .exp_x3_key
                    .enabled
                    .then_some(character.exp_x3_key.key.into()),
                BuffKind::ExpCouponX4 => character
                    .exp_x4_key
                    .enabled
                    .then_some(character.exp_x4_key.key.into()),
                BuffKind::BonusExpCoupon => character
                    .bonus_exp_key
                    .enabled
                    .then_some(character.bonus_exp_key.key.into()),
                BuffKind::MvpBonusExpCoupon => None,
                BuffKind::LegionLuck => character
                    .legion_luck_key
                    .enabled
                    .then_some(character.legion_luck_key.key.into()),
                BuffKind::LegionWealth => character
                    .legion_wealth_key
                    .enabled
                    .then_some(character.legion_wealth_key.key.into()),
                BuffKind::WealthAcquisitionPotion => character
                    .wealth_acquisition_potion_key
                    .enabled
                    .then_some(character.wealth_acquisition_potion_key.key.into()),
                BuffKind::ExpAccumulationPotion => character
                    .exp_accumulation_potion_key
                    .enabled
                    .then_some(character.exp_accumulation_potion_key.key.into()),
                BuffKind::SmallWealthAcquisitionPotion => character
                    .small_wealth_acquisition_potion_key
                    .enabled
                    .then_some(character.small_wealth_acquisition_potion_key.key.into()),
                BuffKind::SmallExpAccumulationPotion => character
                    .small_exp_accumulation_potion_key
                    .enabled
                    .then_some(character.small_exp_accumulation_potion_key.key.into()),
                BuffKind::ForTheGuild => character
                    .for_the_guild_key
                    .enabled
                    .then_some(character.for_the_guild_key.key.into()),
                BuffKind::HardHitter => character
                    .hard_hitter_key
                    .enabled
                    .then_some(character.hard_hitter_key.key.into()),
                BuffKind::ExtremeRedPotion => character
                    .extreme_red_potion_key
                    .enabled
                    .then_some(character.extreme_red_potion_key.key.into()),
                BuffKind::ExtremeBluePotion => character
                    .extreme_blue_potion_key
                    .enabled
                    .then_some(character.extreme_blue_potion_key.key.into()),
                BuffKind::ExtremeGreenPotion => character
                    .extreme_green_potion_key
                    .enabled
                    .then_some(character.extreme_green_potion_key.key.into()),
                BuffKind::ExtremeGoldPotion => character
                    .extreme_gold_potion_key
                    .enabled
                    .then_some(character.extreme_gold_potion_key.key.into()),
            };
            Some(kind).zip(enabled_key)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;
    use std::collections::HashSet;

    use strum::IntoEnumIterator;

    use super::*;
    use crate::{
        ActionConfiguration, ActionConfigurationCondition, Bound, EliteBossBehavior,
        FamiliarRarity, KeyBindingConfiguration, SwappableFamiliars,
    };

    #[test]
    fn update_rotator_mode() {
        let mut map = Map {
            rotation_auto_mob_bound: Bound {
                x: 1,
                y: 1,
                width: 1,
                height: 1,
            },
            rotation_ping_pong_bound: Bound {
                x: 2,
                y: 2,
                width: 2,
                height: 2,
            },
            ..Default::default()
        };

        for mode in RotationMode::iter() {
            map.rotation_mode = mode;

            let mut service = DefaultRotatorService::default();
            service.update_from_map(Some(&map), None);

            match (mode, &service.args.mode) {
                (RotationMode::StartToEnd, RotatorMode::StartToEnd) => {}
                (RotationMode::StartToEndThenReverse, RotatorMode::StartToEndThenReverse) => {}
                (RotationMode::AutoMobbing, RotatorMode::AutoMobbing(key, bound)) => {
                    assert_eq!(*key, map.rotation_mobbing_key);
                    assert_eq!(*bound, map.rotation_auto_mob_bound);
                }
                (RotationMode::PingPong, RotatorMode::PingPong(key, bound)) => {
                    assert_eq!(*key, map.rotation_mobbing_key);
                    assert_eq!(*bound, map.rotation_ping_pong_bound);
                }
                _ => panic!("rotation mode mismatch"),
            }
        }
    }

    #[test]
    fn update_with_buffs() {
        let character = Character {
            sayram_elixir_key: KeyBindingConfiguration {
                key: KeyBinding::F1,
                enabled: true,
            },
            ..Default::default()
        };

        let mut service = DefaultRotatorService::default();
        service.update_from_characters(Some(&character));

        assert_eq!(
            service.args.buffs,
            vec![(BuffKind::SayramElixir, KeyKind::F1)]
        );
    }

    #[test]
    fn update_with_familiar_essence_key() {
        let character = Character {
            familiar_essence_key: KeyBindingConfiguration {
                key: KeyBinding::Z,
                enabled: true,
            },
            ..Default::default()
        };

        let mut service = DefaultRotatorService::default();
        service.update_from_characters(Some(&character));

        assert_eq!(service.args.familiar_essence_key, KeyKind::Z);
    }

    #[test]
    fn update_with_familiar_swap_config() {
        let mut character = Character::default();
        character.familiars.swappable_familiars = SwappableFamiliars::SecondAndLast;
        character.familiars.swappable_rarities =
            HashSet::from_iter([FamiliarRarity::Epic, FamiliarRarity::Rare]);
        character.familiars.swap_check_millis = 5000;
        character.familiars.enable_familiars_swapping = true;

        let mut service = DefaultRotatorService::default();
        service.update_from_characters(Some(&character));

        assert_eq!(service.args.familiars, character.familiars);
    }

    #[test]
    fn update_with_elite_boss_behavior() {
        let character = Character {
            elite_boss_behavior: EliteBossBehavior::CycleChannel,
            elite_boss_behavior_key: KeyBinding::X,
            ..Default::default()
        };

        let mut service = DefaultRotatorService::default();
        service.update_from_characters(Some(&character));

        assert_eq!(
            service.args.elite_boss_behavior,
            EliteBossBehavior::CycleChannel
        );
        assert_eq!(service.args.elite_boss_behavior_key, KeyKind::X);
    }

    #[test]
    fn update_with_reset_normal_actions_on_erda() {
        let map = Map {
            actions_any_reset_on_erda_condition: true,
            ..Default::default()
        };

        let mut service = DefaultRotatorService::default();
        service.update_from_map(Some(&map), None);

        assert!(service.args.enable_reset_normal_actions_on_erda);
    }

    #[test]
    fn update_with_panic_mode_and_rune_solving() {
        let settings = Settings {
            enable_panic_mode: true,
            enable_rune_solving: true,
            ..Default::default()
        };

        let mut service = DefaultRotatorService::default();
        service.update_from_settings(&settings);

        assert!(service.args.enable_panic_mode);
        assert!(service.args.enable_rune_solving);
    }

    #[test]
    fn update_combine_actions_and_fixed_actions() {
        let map_actions = vec![
            Action::Key(ActionKey {
                key: KeyBinding::A,
                ..Default::default()
            }),
            Action::Key(ActionKey {
                key: KeyBinding::B,
                ..Default::default()
            }),
        ];

        let character = Character {
            actions: vec![
                ActionConfiguration {
                    key: KeyBinding::C,
                    enabled: true,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::D,
                    condition: ActionConfigurationCondition::Linked,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::E,
                    condition: ActionConfigurationCondition::Linked,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::F,
                    enabled: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let mut map = Map::default();
        map.actions
            .insert("preset".to_string(), map_actions.clone());

        let mut service = DefaultRotatorService::default();
        service.update_from_map(Some(&map), Some("preset".to_string()));
        service.update_from_characters(Some(&character));

        assert_matches!(
            service.args.character_actions.as_slice(),
            [
                Action::Key(ActionKey {
                    key: KeyBinding::C,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::D,
                    condition: ActionCondition::Linked,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::E,
                    condition: ActionCondition::Linked,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::F,
                    ..
                }),
            ]
        );

        assert_eq!(service.args.map_actions, map_actions);
    }

    #[test]
    fn update_character_actions_only() {
        let character = Character {
            actions: vec![
                ActionConfiguration {
                    key: KeyBinding::C,
                    enabled: true,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::D,
                    condition: ActionConfigurationCondition::Linked,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::E,
                    condition: ActionConfigurationCondition::Linked,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::F,
                    enabled: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let mut service = DefaultRotatorService::default();
        service.update_from_characters(Some(&character));

        assert_matches!(
            service.args.character_actions.as_slice(),
            [
                Action::Key(ActionKey {
                    key: KeyBinding::C,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::D,
                    condition: ActionCondition::Linked,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::E,
                    condition: ActionCondition::Linked,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::F,
                    ..
                }),
            ]
        );
    }
}
