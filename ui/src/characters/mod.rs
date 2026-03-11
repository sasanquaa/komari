use std::{fmt::Display, mem};

use backend::{
    Character, EliteBossBehavior, ExchangeHexaBoosterCondition, FamiliarRarity, Familiars,
    IntoEnumIterator, KeyBinding, KeyBindingConfiguration, PotionMode, SwappableFamiliars,
    delete_character, query_characters, update_character, upsert_character,
};
use dioxus::{html::FileData, prelude::*};
use futures_util::StreamExt;

use crate::{
    AppState,
    characters::{actions::SectionFixedActions, bindings::SectionKeyBindings, buffs::SectionBuffs},
    components::{
        ContentAlign, ContentSide,
        button::Button,
        checkbox::Checkbox,
        duration::{DurationInput, DurationParts},
        file::{FileInput, FileOutput},
        key::KeyInput,
        labeled::Labeled,
        named_select::NamedSelect,
        numbers::{MillisInput, PercentageInput, PrimitiveIntegerInput},
        section::Section,
        select::{Select, SelectOption},
    },
};

mod actions;
mod bindings;
mod buffs;
mod list;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum CharactersUpdate {
    Set,
    Update(Character),
    Create(String),
    Delete,
}

#[derive(PartialEq, Clone, Copy)]
struct CharactersContext {
    character: Memo<Character>,
    save_character: Callback<Character>,
}

#[component]
pub fn CharactersScreen() -> Element {
    let mut character = use_context::<AppState>().character;
    let mut characters = use_resource(async || query_characters().await.unwrap_or_default());
    // Maps queried `characters` to names
    let character_names = use_memo::<Vec<String>>(move || {
        characters()
            .unwrap_or_default()
            .into_iter()
            .map(|character| character.name)
            .collect()
    });
    // Maps currently selected `character` to the index in `characters`
    let character_index = use_memo(move || {
        characters()
            .zip(character())
            .and_then(|(characters, character)| {
                characters
                    .into_iter()
                    .enumerate()
                    .find(|(_, cfg)| character.id == cfg.id)
                    .map(|(i, _)| i)
            })
    });
    // Default character if `character` is `None`
    let character_view = use_memo(move || character().unwrap_or_default());

    // Handles async operations for character-related
    let coroutine = use_coroutine(
        move |mut rx: UnboundedReceiver<CharactersUpdate>| async move {
            let mut save_character = async move |new_character: Character| {
                if let Some(new_character) = upsert_character(new_character).await {
                    character.set(Some(new_character));
                    characters.restart();
                }
            };

            while let Some(message) = rx.next().await {
                match message {
                    CharactersUpdate::Set => {
                        update_character(character()).await;
                    }
                    CharactersUpdate::Update(new_character) => {
                        save_character(new_character).await;
                    }
                    CharactersUpdate::Create(name) => {
                        save_character(Character {
                            name,
                            ..Character::default()
                        })
                        .await;
                        update_character(character()).await;
                    }
                    CharactersUpdate::Delete => {
                        if let Some(current_character) = character()
                            && delete_character(current_character).await
                        {
                            characters.restart();
                            character.set(None);
                        }
                    }
                }
            }
        },
    );

    let save_character = use_callback(move |new_character: Character| {
        coroutine.send(CharactersUpdate::Update(new_character));
    });

    let select_character = use_callback(move |index: usize| {
        let selected = characters
            .peek()
            .as_ref()
            .unwrap()
            .get(index)
            .cloned()
            .unwrap();

        character.set(Some(selected));
        coroutine.send(CharactersUpdate::Set);
    });

    use_context_provider(|| CharactersContext {
        character: character_view,
        save_character,
    });

    // Sets a character if there is not one
    use_effect(move || {
        if let Some(characters) = characters()
            && !characters.is_empty()
            && character.peek().is_none()
        {
            character.set(characters.into_iter().next());
            coroutine.send(CharactersUpdate::Set);
        }
    });

    rsx! {
        div { class: "flex flex-col pb-15 h-full overflow-y-auto",
            SectionKeyBindings {}
            SectionUsePotionAndFeedPet {}
            SectionUseBooster {}
            SectionMovement {}
            SectionFamiliars {}
            SectionBuffs {}
            SectionFixedActions {}
            SectionOthers {}
        }

        div { class: "flex items-center w-full h-10 bg-primary-surface absolute bottom-0 pr-2",
            NamedSelect {
                class: "flex-grow",
                on_create: move |name| {
                    coroutine.send(CharactersUpdate::Create(name));
                },
                on_delete: move |_| {
                    coroutine.send(CharactersUpdate::Delete);
                },
                delete_disabled: character_names().is_empty(),

                Select::<usize> {
                    class: "w-full",
                    placeholder: "Create a character...",
                    disabled: character_names().is_empty(),
                    on_selected: move |index| {
                        select_character(index);
                    },

                    for (i , name) in character_names().into_iter().enumerate() {
                        SelectOption::<usize> {
                            value: i,
                            selected: character_index() == Some(i),
                            label: name,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SectionUsePotionAndFeedPet() -> Element {
    rsx! {
        Section { title: "Use potion and feed pet",
            div { class: "flex flex-col gap-4",
                UsePotion {}
                FeedPet {}
            }
        }
    }
}

#[component]
fn FeedPet() -> Element {
    let context = use_context::<CharactersContext>();
    let character = context.character;
    let save_character = context.save_character;

    rsx! {
        div { class: "grid grid-cols-4 gap-4",
            CharactersKeyBindingConfigurationInput {
                label: "Feed key",
                disabled: character().id.is_none(),
                on_value: move |key_config: Option<KeyBindingConfiguration>| {
                    save_character(Character {
                        feed_pet_key: key_config.expect("not optional"),
                        ..character.peek().clone()
                    });
                },
                value: character().feed_pet_key,
            }
            CharactersNumberU32Input {
                label: "Count",
                disabled: character().id.is_none(),
                on_value: move |feed_pet_count| {
                    save_character(Character {
                        feed_pet_count,
                        ..character.peek().clone()
                    });
                },
                value: character().feed_pet_count,
            }
            CharactersDurationInput {
                label: "Every (mm:ss)",
                disabled: character().id.is_none(),
                on_value: move |feed_pet_millis| {
                    save_character(Character {
                        feed_pet_millis,
                        ..character.peek().clone()
                    });
                },
                value: character().feed_pet_millis,
            }
            CharactersCheckbox {
                label: "Enabled",
                disabled: character().id.is_none(),
                on_checked: move |enabled| {
                    let character = character.peek().clone();
                    save_character(Character {
                        feed_pet_key: KeyBindingConfiguration {
                            enabled,
                            ..character.feed_pet_key
                        },
                        ..character
                    });
                },
                checked: character().feed_pet_key.enabled,
            }
        }
    }
}

#[component]
fn UsePotion() -> Element {
    let context = use_context::<CharactersContext>();
    let character = context.character;
    let save_character = context.save_character;

    rsx! {
        div { class: "grid grid-cols-4 gap-4",
            CharactersKeyBindingConfigurationInput {
                label: "Potion key",
                disabled: character().id.is_none(),
                on_value: move |key_config: Option<KeyBindingConfiguration>| {
                    save_character(Character {
                        potion_key: key_config.expect("not optional"),
                        ..character.peek().clone()
                    });
                },
                value: character().potion_key,
            }
            CharactersSelect::<PotionMode> {
                label: "Mode",
                disabled: character().id.is_none(),
                on_selected: move |potion_mode| {
                    save_character(Character {
                        potion_mode,
                        ..character.peek().clone()
                    });
                },
                selected: character().potion_mode,
            }
            match character().potion_mode {
                PotionMode::EveryMillis(millis) => rsx! {
                    CharactersDurationInput {
                        label: "Every (mm:ss)",
                        disabled: character().id.is_none(),
                        on_value: move |millis| {
                            save_character(Character {
                                potion_mode: PotionMode::EveryMillis(millis),
                                ..character.peek().clone()
                            });
                        },
                        value: millis,
                    }
                },
                PotionMode::Percentage(percent) => rsx! {
                    div { class: "grid grid-cols-2 gap-2",
                        CharactersPercentageInput {
                            label: "HP below",
                            disabled: character().id.is_none(),
                            on_value: move |percent| {
                                save_character(Character {
                                    potion_mode: PotionMode::Percentage(percent as f32),
                                    ..character.peek().clone()
                                });
                            },
                            value: percent as u32,
                        }
                        CharactersMillisInput {
                            label: "HP update every",
                            disabled: character().id.is_none(),
                            on_value: move |millis| {
                                save_character(Character {
                                    health_update_millis: millis,
                                    ..character.peek().clone()
                                });
                            },
                            value: character().health_update_millis,
                        }
                    }
                },
            }
            CharactersCheckbox {
                label: "Enabled",
                disabled: character().id.is_none(),
                on_checked: move |enabled| {
                    let character = character.peek().clone();
                    save_character(Character {
                        potion_key: KeyBindingConfiguration {
                            enabled,
                            ..character.potion_key
                        },
                        ..character
                    });
                },
                checked: character().potion_key.enabled,
            }
        }
    }
}

#[component]
fn SectionUseBooster() -> Element {
    let context = use_context::<CharactersContext>();
    let character = context.character;
    let save_character = context.save_character;

    rsx! {
        Section { title: "Use booster",
            div { class: "grid grid-cols-3 gap-4",
                CharactersKeyBindingConfigurationInput {
                    label: "Generic Booster key",
                    value: character().generic_booster_key,
                    on_value: move |key_config: Option<KeyBindingConfiguration>| {
                        save_character(Character {
                            generic_booster_key: key_config.expect("not optional"),
                            ..character.peek().clone()
                        });
                    },
                    disabled: character().id.is_none(),
                    label_class: "col-span-2",
                }
                CharactersCheckbox {
                    label: "Enabled",
                    checked: character().generic_booster_key.enabled,
                    on_checked: move |enabled| {
                        let character = character.peek().clone();
                        save_character(Character {
                            generic_booster_key: KeyBindingConfiguration {
                                enabled,
                                ..character.generic_booster_key
                            },
                            ..character
                        });
                    },
                    disabled: character().id.is_none(),
                }
                CharactersKeyBindingConfigurationInput {
                    label: "HEXA Booster key",
                    value: character().hexa_booster_key,
                    on_value: move |key_config: Option<KeyBindingConfiguration>| {
                        save_character(Character {
                            hexa_booster_key: key_config.expect("not optional"),
                            ..character.peek().clone()
                        });
                    },
                    disabled: character().id.is_none(),
                    label_class: "col-span-2",
                }
                CharactersCheckbox {
                    label: "Enabled",
                    checked: character().hexa_booster_key.enabled,
                    on_checked: move |enabled| {
                        let character = character.peek().clone();
                        save_character(Character {
                            hexa_booster_key: KeyBindingConfiguration {
                                enabled,
                                ..character.hexa_booster_key
                            },
                            ..character
                        });
                    },
                    disabled: character().id.is_none(),
                }
                CharactersSelect::<ExchangeHexaBoosterCondition> {
                    label: "Exchange when Sol Erda",
                    tooltip: "Requires HEXA Booster to be visible in quick slots, Sol Erda tracker menu opened and HEXA Matrix configured in the quick menu. Exchange will only happen if there is no HEXA Booster.",
                    selected: character().hexa_booster_exchange_condition,
                    on_selected: move |hexa_booster_exchange_condition| {
                        save_character(Character {
                            hexa_booster_exchange_condition,
                            ..character.peek().clone()
                        });
                    },
                    disabled: character().id.is_none(),
                }
                CharactersNumberU32Input {
                    label: "Amount",
                    max_value: 20,
                    value: character().hexa_booster_exchange_amount,
                    on_value: move |hexa_booster_exchange_amount| {
                        save_character(Character {
                            hexa_booster_exchange_amount,
                            ..character.peek().clone()
                        });
                    },
                    disabled: character().id.is_none() || character().hexa_booster_exchange_all,
                }
                CharactersCheckbox {
                    label: "Exchange all",
                    checked: character().hexa_booster_exchange_all,
                    on_checked: move |hexa_booster_exchange_all| {
                        save_character(Character {
                            hexa_booster_exchange_all,
                            ..character.peek().clone()
                        });
                    },
                    disabled: character().id.is_none(),
                }
            }
        }
    }
}

#[component]
fn SectionMovement() -> Element {
    let context = use_context::<CharactersContext>();
    let character = context.character;
    let save_character = context.save_character;
    let disabled = use_memo(move || character().id.is_none());

    rsx! {
        Section { title: "Movement",
            div { class: "grid grid-cols-3 gap-4",
                CharactersCheckbox {
                    label: "Up jump is flight",
                    on_checked: move |up_jump_is_flight| {
                        save_character(Character {
                            up_jump_is_flight,
                            ..character.peek().clone()
                        });
                    },
                    checked: character().up_jump_is_flight,
                    tooltip: "Applicable only to mage class or when non-up-arrow up jump key is set.",
                    disabled,
                }
                CharactersCheckbox {
                    label: "Jump then up jump if possible",
                    on_checked: move |up_jump_specific_key_should_jump| {
                        save_character(Character {
                            up_jump_specific_key_should_jump,
                            ..character.peek().clone()
                        });
                    },
                    checked: character().up_jump_specific_key_should_jump,
                    tooltip: "Applicable only for non-mage class and when non-up-arrow up jump key is set.",
                    disabled,
                }
                CharactersCheckbox {
                    label: "Has extended teleport range",
                    on_checked: move |has_extended_teleport_range| {
                        save_character(Character {
                            has_extended_teleport_range,
                            ..character.peek().clone()
                        });
                    },
                    checked: character().has_extended_teleport_range,
                    tooltip: "Applicable only for mage class when teleport range increase buff is turned on.",
                    disabled,
                }
                CharactersCheckbox {
                    label: "Disable teleport on fall",
                    on_checked: move |disable_teleport_on_fall| {
                        save_character(Character {
                            disable_teleport_on_fall,
                            ..character.peek().clone()
                        });
                    },
                    checked: character().disable_teleport_on_fall,
                    tooltip: "Applicable only to mage class.",
                    disabled,
                }
                CharactersCheckbox {
                    label: "Disable double jumping",
                    on_checked: move |disable_double_jumping| {
                        save_character(Character {
                            disable_double_jumping,
                            ..character.peek().clone()
                        });
                    },
                    checked: character().disable_double_jumping,
                    tooltip: "Not applicable if an action requires double jumping.",
                    disabled,
                }
                CharactersCheckbox {
                    label: "Disable grapple on double jumping",
                    checked: character().disable_grapple_on_double_jumping,
                    on_checked: move |disable_grapple_on_double_jumping| {
                        save_character(Character {
                            disable_grapple_on_double_jumping,
                            ..character.peek().clone()
                        });
                    },
                    tooltip: "Applicable only if grapple key is set.",
                    disabled,
                }
                CharactersCheckbox {
                    label: "Disable walking",
                    checked: character().disable_adjusting,
                    on_checked: move |disable_adjusting| {
                        save_character(Character {
                            disable_adjusting,
                            ..character.peek().clone()
                        });
                    },
                    tooltip: "Not applicable if an action requires adjusting.",
                    disabled,
                }
            }
        }
    }
}

#[component]
fn SectionFamiliars() -> Element {
    let context = use_context::<CharactersContext>();
    let character = context.character;
    let save_character = context.save_character;
    let familiars = use_memo(move || character().familiars);

    rsx! {
        Section { title: "Familiars",
            div { class: "grid grid-cols-3 gap-4",
                CharactersSelect::<SwappableFamiliars> {
                    label: "Swappable slots",
                    disabled: !familiars().enable_familiars_swapping,
                    on_selected: move |swappable_familiars| async move {
                        save_character(Character {
                            familiars: Familiars {
                                swappable_familiars,
                                ..familiars.peek().clone()
                            },
                            ..character.peek().clone()
                        });
                    },
                    selected: familiars().swappable_familiars,
                }
                CharactersDurationInput {
                    label: "Swap check every (mm:ss)",
                    disabled: !familiars().enable_familiars_swapping,
                    on_value: move |swap_check_millis| {
                        save_character(Character {
                            familiars: Familiars {
                                swap_check_millis,
                                ..familiars.peek().clone()
                            },
                            ..character.peek().clone()
                        });
                    },
                    value: familiars().swap_check_millis,
                }

                CharactersCheckbox {
                    label: "Swapping enabled",
                    on_checked: move |enable_familiars_swapping| {
                        save_character(Character {
                            familiars: Familiars {
                                enable_familiars_swapping,
                                ..familiars.peek().clone()
                            },
                            ..character.peek().clone()
                        });
                    },
                    checked: familiars().enable_familiars_swapping,
                }

                CharactersCheckbox {
                    label: "Can swap rare familiars",
                    disabled: !familiars().enable_familiars_swapping,
                    on_checked: move |allowed| {
                        let mut rarities = familiars.peek().swappable_rarities.clone();
                        if allowed {
                            rarities.insert(FamiliarRarity::Rare);
                        } else {
                            rarities.remove(&FamiliarRarity::Rare);
                        }
                        save_character(Character {
                            familiars: Familiars {
                                swappable_rarities: rarities,
                                ..familiars.peek().clone()
                            },
                            ..character.peek().clone()
                        });
                    },
                    checked: familiars().swappable_rarities.contains(&FamiliarRarity::Rare),
                }
                CharactersCheckbox {
                    label: "Can swap epic familiars",
                    disabled: !familiars().enable_familiars_swapping,
                    on_checked: move |allowed| {
                        let mut rarities = familiars.peek().swappable_rarities.clone();
                        if allowed {
                            rarities.insert(FamiliarRarity::Epic);
                        } else {
                            rarities.remove(&FamiliarRarity::Epic);
                        }
                        save_character(Character {
                            familiars: Familiars {
                                swappable_rarities: rarities,
                                ..familiars.peek().clone()
                            },
                            ..character.peek().clone()
                        });
                    },
                    checked: familiars().swappable_rarities.contains(&FamiliarRarity::Epic),
                }
            }
        }
    }
}

#[component]
fn SectionOthers() -> Element {
    let context = use_context::<CharactersContext>();
    let character = context.character;
    let save_character = context.save_character;

    let export_name = use_memo(move || format!("{}.json", character().name));
    let export_content = move |_| serde_json::to_vec_pretty(&*character.peek()).unwrap_or_default();

    let import_character = use_callback(move |file: FileData| async move {
        let Ok(bytes) = file.read_bytes().await else {
            return;
        };
        let Ok(character) = serde_json::from_slice::<'_, Character>(&bytes) else {
            return;
        };

        save_character(character);
    });

    let disabled = use_memo(move || character().id.is_none());

    rsx! {
        Section { title: "Others",
            div { class: "grid grid-cols-[auto_auto_128px] gap-4",
                CharactersMillisInput {
                    label: "Link key timing",
                    disabled: disabled(),
                    on_value: move |link_key_timing_millis| {
                        save_character(Character {
                            link_key_timing_millis,
                            ..character.peek().clone()
                        });
                    },
                    value: character().link_key_timing_millis,
                }
                div {}
                div {}
                CharactersSelect::<EliteBossBehavior> {
                    label: "Elite boss spawns behavior",
                    disabled,
                    on_selected: move |elite_boss_behavior| {
                        save_character(Character {
                            elite_boss_behavior,
                            ..character.peek().clone()
                        });
                    },
                    selected: character().elite_boss_behavior,
                }
                CharactersKeyInput {
                    label: "Key to use",
                    disabled,
                    on_value: move |key: Option<KeyBinding>| {
                        save_character(Character {
                            elite_boss_behavior_key: key.expect("not optional"),
                            ..character.peek().clone()
                        });
                    },
                    value: Some(character().elite_boss_behavior_key),
                }
                div {}
                div { class: "flex gap-2 col-span-3",
                    FileInput {
                        on_file: move |file| async move {
                            import_character(file).await;
                        },
                        class: "flex-grow",
                        Button { class: "w-full", "Import" }
                    }
                    FileOutput {
                        class: "flex-grow",
                        on_file: export_content,
                        download: export_name(),
                        disabled,
                        Button { class: "w-full", disabled, "Export" }
                    }
                }
            }
        }
    }
}

#[component]
fn CharactersKeyBindingConfigurationInput(
    label: String,
    value: Option<KeyBindingConfiguration>,
    on_value: Callback<Option<KeyBindingConfiguration>>,
    #[props(default)] optional: bool,
    #[props(default)] tooltip: Option<String>,
    disabled: ReadSignal<bool>,
    #[props(default)] label_class: String,
    #[props(default)] input_class: String,
) -> Element {
    rsx! {
        CharactersKeyInput {
            label,
            value: value.map(|config| config.key),
            on_value: move |new_value: Option<KeyBinding>| {
                let new_value = new_value
                    .map(|key| {
                        let mut config = value.unwrap_or_default();
                        config.key = key;
                        config
                    });
                on_value(new_value);
            },
            optional,
            tooltip,
            disabled,
            label_class,
            input_class,
        }
    }
}

#[component]
fn CharactersKeyInput(
    label: String,
    value: Option<KeyBinding>,
    on_value: Callback<Option<KeyBinding>>,
    #[props(default)] optional: bool,
    #[props(default)] tooltip: Option<String>,
    #[props(default = ContentSide::Bottom)] tooltip_side: ContentSide,
    #[props(default = ContentAlign::Start)] tooltip_align: ContentAlign,
    #[props(default)] disabled: ReadSignal<bool>,
    #[props(default)] label_class: String,
    #[props(default)] input_class: String,
) -> Element {
    let label = if optional {
        format!("{label} (optional)")
    } else {
        label
    };

    rsx! {
        Labeled {
            label,
            class: label_class,
            tooltip,
            tooltip_side,
            tooltip_align,

            KeyInput {
                value,
                on_value,
                optional,
                disabled,
                class: input_class,
            }
        }
    }
}

#[component]
fn CharactersCheckbox(
    label: &'static str,
    checked: bool,
    on_checked: Callback<bool>,
    #[props(default)] tooltip: Option<String>,
    #[props(default = ContentSide::Top)] tooltip_side: ContentSide,
    #[props(default = ContentAlign::Center)] tooltip_align: ContentAlign,
    #[props(default)] disabled: ReadSignal<bool>,
) -> Element {
    rsx! {
        Labeled {
            label,
            tooltip,
            tooltip_side,
            tooltip_align,
            Checkbox { checked, on_checked, disabled }
        }
    }
}

#[component]
fn CharactersSelect<T: PartialEq + Clone + Display + IntoEnumIterator + 'static>(
    label: &'static str,
    #[props(default)] label_class: String,
    #[props(default)] tooltip: Option<String>,
    #[props(default = ContentAlign::Start)] tooltip_align: ContentAlign,
    on_selected: Callback<T>,
    selected: ReadSignal<T>,
    #[props(default)] disabled: ReadSignal<bool>,
) -> Element {
    let selected_equal =
        use_callback(move |value: T| mem::discriminant(&selected()) == mem::discriminant(&value));

    rsx! {
        Labeled {
            label,
            class: label_class,
            tooltip,
            tooltip_align,
            Select::<T> {
                on_selected: move |selected| {
                    on_selected(selected);
                },
                disabled,

                for value in T::iter() {
                    SelectOption::<T> {
                        value: value.clone(),
                        label: value.to_string(),
                        selected: selected_equal(value),
                        disabled,
                    }
                }
            }
        }
    }
}

#[component]
fn CharactersPercentageInput(
    label: &'static str,
    on_value: Callback<u32>,
    value: u32,
    disabled: bool,
) -> Element {
    rsx! {
        Labeled { label,
            PercentageInput { value, on_value, disabled }
        }
    }
}

#[component]
fn CharactersDurationInput(
    label: &'static str,
    value: u64,
    on_value: Callback<u64>,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        Labeled { label,
            DurationInput {
                value,
                on_value,
                disabled,
                parts: DurationParts::MinutesAndSeconds,
            }
        }
    }
}

#[component]
fn CharactersMillisInput(
    label: &'static str,
    value: u64,
    on_value: Callback<u64>,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        Labeled { label,
            MillisInput { value, on_value, disabled }
        }
    }
}

#[component]
fn CharactersNumberU32Input(
    label: &'static str,
    value: u32,
    on_value: Callback<u32>,
    #[props(default)] max_value: Option<u32>,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        Labeled { label,
            PrimitiveIntegerInput {
                value,
                on_value,
                min_value: 1,
                max_value,
                disabled,
            }
        }
    }
}
