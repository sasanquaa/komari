use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

use super::{
    ActionConfiguration, KeyBinding, KeyBindingConfiguration, deserialize_with_ok_or_default,
    impl_identifiable,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Character {
    #[serde(skip_serializing, default)]
    pub id: Option<i64>,
    pub name: String,
    pub ropelift_key: Option<KeyBindingConfiguration>,
    pub teleport_key: Option<KeyBindingConfiguration>,
    #[serde(default = "jump_key_default")]
    pub jump_key: KeyBindingConfiguration,
    pub up_jump_key: Option<KeyBindingConfiguration>,
    #[serde(default = "key_default")]
    pub interact_key: KeyBindingConfiguration,
    pub cash_shop_key: Option<KeyBindingConfiguration>,
    pub familiar_menu_key: Option<KeyBindingConfiguration>,
    pub to_town_key: Option<KeyBindingConfiguration>,
    pub change_channel_key: Option<KeyBindingConfiguration>,
    pub feed_pet_key: KeyBindingConfiguration,
    pub feed_pet_millis: u64,
    #[serde(default = "feed_pet_count_default", alias = "num_pets")]
    pub feed_pet_count: u32,
    pub potion_key: KeyBindingConfiguration,
    pub potion_mode: PotionMode,
    pub health_update_millis: u64,
    #[serde(default)]
    pub familiars: Familiars,
    pub familiar_buff_key: KeyBindingConfiguration,
    #[serde(default = "key_default")]
    pub familiar_essence_key: KeyBindingConfiguration,
    pub sayram_elixir_key: KeyBindingConfiguration,
    pub aurelia_elixir_key: KeyBindingConfiguration,
    #[serde(default)]
    pub exp_x2_key: KeyBindingConfiguration,
    pub exp_x3_key: KeyBindingConfiguration,
    #[serde(default)]
    pub exp_x4_key: KeyBindingConfiguration,
    pub bonus_exp_key: KeyBindingConfiguration,
    pub legion_wealth_key: KeyBindingConfiguration,
    pub legion_luck_key: KeyBindingConfiguration,
    pub wealth_acquisition_potion_key: KeyBindingConfiguration,
    pub exp_accumulation_potion_key: KeyBindingConfiguration,
    #[serde(default)]
    pub small_wealth_acquisition_potion_key: KeyBindingConfiguration,
    #[serde(default)]
    pub small_exp_accumulation_potion_key: KeyBindingConfiguration,
    #[serde(default)]
    pub for_the_guild_key: KeyBindingConfiguration,
    #[serde(default)]
    pub hard_hitter_key: KeyBindingConfiguration,
    pub extreme_red_potion_key: KeyBindingConfiguration,
    pub extreme_blue_potion_key: KeyBindingConfiguration,
    pub extreme_green_potion_key: KeyBindingConfiguration,
    pub extreme_gold_potion_key: KeyBindingConfiguration,
    #[serde(default, alias = "vip_booster_key")]
    pub generic_booster_key: KeyBindingConfiguration,
    #[serde(default)]
    pub hexa_booster_key: KeyBindingConfiguration,
    #[serde(default)]
    pub hexa_booster_exchange_condition: ExchangeHexaBoosterCondition,
    #[serde(default = "hexa_booster_exchange_amount_default")]
    pub hexa_booster_exchange_amount: u32,
    #[serde(default)]
    pub hexa_booster_exchange_all: bool,
    #[serde(default)]
    pub link_key_timing_millis: u64,
    #[serde(default)]
    pub disable_double_jumping: bool,
    pub disable_adjusting: bool,
    #[serde(default)]
    pub disable_teleport_on_fall: bool,
    #[serde(default)]
    pub disable_grapple_on_double_jumping: bool,
    #[serde(default)]
    pub up_jump_is_flight: bool,
    #[serde(default)]
    pub up_jump_specific_key_should_jump: bool,
    #[serde(default)]
    pub has_extended_teleport_range: bool,
    pub actions: Vec<ActionConfiguration>,
    #[serde(default, deserialize_with = "deserialize_with_ok_or_default")]
    pub elite_boss_behavior: EliteBossBehavior,
    #[serde(default)]
    pub elite_boss_behavior_key: KeyBinding,
}

impl_identifiable!(Character);

impl Default for Character {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            ropelift_key: None,
            teleport_key: None,
            jump_key: jump_key_default(),
            up_jump_key: None,
            interact_key: key_default(),
            cash_shop_key: None,
            familiar_menu_key: None,
            to_town_key: None,
            change_channel_key: None,
            feed_pet_key: KeyBindingConfiguration::default(),
            feed_pet_millis: 320000,
            feed_pet_count: feed_pet_count_default(),
            potion_key: KeyBindingConfiguration::default(),
            potion_mode: PotionMode::EveryMillis(180000),
            health_update_millis: 1000,
            familiars: Familiars::default(),
            familiar_buff_key: KeyBindingConfiguration::default(),
            familiar_essence_key: key_default(),
            sayram_elixir_key: KeyBindingConfiguration::default(),
            aurelia_elixir_key: KeyBindingConfiguration::default(),
            exp_x2_key: KeyBindingConfiguration::default(),
            exp_x3_key: KeyBindingConfiguration::default(),
            exp_x4_key: KeyBindingConfiguration::default(),
            bonus_exp_key: KeyBindingConfiguration::default(),
            legion_wealth_key: KeyBindingConfiguration::default(),
            legion_luck_key: KeyBindingConfiguration::default(),
            wealth_acquisition_potion_key: KeyBindingConfiguration::default(),
            exp_accumulation_potion_key: KeyBindingConfiguration::default(),
            small_wealth_acquisition_potion_key: KeyBindingConfiguration::default(),
            small_exp_accumulation_potion_key: KeyBindingConfiguration::default(),
            for_the_guild_key: KeyBindingConfiguration::default(),
            hard_hitter_key: KeyBindingConfiguration::default(),
            extreme_red_potion_key: KeyBindingConfiguration::default(),
            extreme_blue_potion_key: KeyBindingConfiguration::default(),
            extreme_green_potion_key: KeyBindingConfiguration::default(),
            extreme_gold_potion_key: KeyBindingConfiguration::default(),
            generic_booster_key: KeyBindingConfiguration::default(),
            hexa_booster_key: KeyBindingConfiguration::default(),
            hexa_booster_exchange_condition: ExchangeHexaBoosterCondition::default(),
            hexa_booster_exchange_amount: hexa_booster_exchange_amount_default(),
            hexa_booster_exchange_all: false,
            link_key_timing_millis: 0,
            disable_double_jumping: false,
            disable_adjusting: false,
            disable_teleport_on_fall: false,
            disable_grapple_on_double_jumping: false,
            up_jump_is_flight: false,
            up_jump_specific_key_should_jump: false,
            has_extended_teleport_range: false,
            actions: vec![],
            elite_boss_behavior_key: KeyBinding::default(),
            elite_boss_behavior: EliteBossBehavior::default(),
        }
    }
}

fn feed_pet_count_default() -> u32 {
    3
}

fn hexa_booster_exchange_amount_default() -> u32 {
    1
}

fn jump_key_default() -> KeyBindingConfiguration {
    // Enabled is not neccessary but for semantic purpose
    KeyBindingConfiguration {
        key: KeyBinding::Space,
        enabled: true,
    }
}

fn key_default() -> KeyBindingConfiguration {
    // Enabled is not neccessary but for semantic purpose
    KeyBindingConfiguration {
        key: KeyBinding::default(),
        enabled: true,
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize, EnumIter, Display, EnumString)]
pub enum PotionMode {
    EveryMillis(u64),
    Percentage(f32),
}

impl Default for PotionMode {
    fn default() -> Self {
        Self::EveryMillis(0)
    }
}

#[derive(
    Clone, Copy, PartialEq, Default, Debug, Serialize, Deserialize, EnumIter, Display, EnumString,
)]
pub enum EliteBossBehavior {
    #[default]
    None,
    CycleChannel,
    UseKey,
}

#[derive(
    Clone, Copy, PartialEq, Default, Debug, Serialize, Deserialize, EnumIter, Display, EnumString,
)]
pub enum ExchangeHexaBoosterCondition {
    #[default]
    None,
    Full,
    AtLeastOne,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Familiars {
    pub enable_familiars_swapping: bool,
    #[serde(default = "familiars_swap_check_millis")]
    pub swap_check_millis: u64,
    pub swappable_familiars: SwappableFamiliars,
    pub swappable_rarities: HashSet<FamiliarRarity>,
}

impl Default for Familiars {
    fn default() -> Self {
        Self {
            enable_familiars_swapping: false,
            swap_check_millis: familiars_swap_check_millis(),
            swappable_familiars: SwappableFamiliars::default(),
            swappable_rarities: HashSet::default(),
        }
    }
}

fn familiars_swap_check_millis() -> u64 {
    300000
}

#[derive(
    Clone, Copy, PartialEq, Default, Debug, Serialize, Deserialize, EnumIter, Display, EnumString,
)]
pub enum SwappableFamiliars {
    #[default]
    All,
    Last,
    SecondAndLast,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, Hash, Serialize, Deserialize)]
pub enum FamiliarRarity {
    #[default]
    Rare,
    Epic,
}
