use std::fmt::Display;

use backend::{
    Class, Configuration, IntoEnumIterator, KeyBinding, KeyBindingConfiguration, PotionMode,
    delete_config, query_configs, update_configuration, upsert_config,
};
use dioxus::prelude::*;
use futures_util::StreamExt;
use tokio::task::spawn_blocking;

use crate::{
    AppState,
    inputs::{Checkbox, KeyBindingInput, MillisInput, PercentageInput},
    select::{EnumSelect, TextSelect},
};

const INPUT_LABEL_CLASS: &str = "label";
const INPUT_DIV_CLASS: &str = "flex flex-col gap-1";
const KEY_INPUT_CLASS: &str = "h-6";
const INPUT_CLASS: &str = "h-6 px-1 w-full paragraph-xs outline-none border border-gray-600";

#[derive(Debug)]
enum ConfigurationUpdate {
    Set,
    Save,
    Create(String),
    Delete,
}

#[component]
pub fn Characters() -> Element {
    let mut config = use_context::<AppState>().config;
    let mut configs = use_resource(move || async move {
        spawn_blocking(|| query_configs().expect("failed to query configs"))
            .await
            .unwrap()
    });
    // Maps queried `configs` to names
    let config_names = use_memo(move || {
        configs()
            .unwrap_or_default()
            .into_iter()
            .map(|config| config.name)
            .collect()
    });
    // Maps currently selected `config` to the index in `configs`
    let config_index = use_memo(move || {
        configs().zip(config()).and_then(|(configs, config)| {
            configs
                .into_iter()
                .enumerate()
                .find(|(_, cfg)| config.id == cfg.id)
                .map(|(i, _)| i)
        })
    });
    // Default config if `config` is `None`
    let config_view = use_memo(move || config().unwrap_or_default());

    // Handles async operations for configuration-related
    let coroutine = use_coroutine(
        move |mut rx: UnboundedReceiver<ConfigurationUpdate>| async move {
            while let Some(message) = rx.next().await {
                match message {
                    ConfigurationUpdate::Set => {
                        update_configuration(config().expect("config must be already set")).await;
                    }
                    ConfigurationUpdate::Save => {
                        let mut config = config().expect("config must be already set");
                        debug_assert!(config.id.is_some(), "saving invalid config");

                        spawn_blocking(move || {
                            upsert_config(&mut config).unwrap();
                        })
                        .await
                        .unwrap();
                    }
                    ConfigurationUpdate::Create(name) => {
                        let mut new_config = Configuration {
                            name,
                            ..Configuration::default()
                        };
                        let mut save_config = new_config.clone();
                        let save_id = spawn_blocking(move || {
                            upsert_config(&mut save_config).unwrap();
                            save_config
                                .id
                                .expect("config id must be valid after creation")
                        })
                        .await
                        .unwrap();

                        new_config.id = Some(save_id);
                        config.set(Some(new_config));
                        configs.restart();
                    }
                    ConfigurationUpdate::Delete => {
                        if let Some(config) = config.take() {
                            spawn_blocking(move || {
                                delete_config(&config).expect("failed to delete config");
                            })
                            .await
                            .unwrap();
                            configs.restart();
                        }
                    }
                }
            }
        },
    );
    let save_config = use_callback(move |new_config: Configuration| {
        config.set(Some(new_config));
        coroutine.send(ConfigurationUpdate::Save);
        coroutine.send(ConfigurationUpdate::Set);
    });

    // Sets a configuration if there is not one
    use_effect(move || {
        if let Some(configs) = configs()
            && config.peek().is_none()
        {
            config.set(configs.into_iter().next());
            coroutine.send(ConfigurationUpdate::Set);
        }
    });

    rsx! {
        div { class: "flex flex-col pb-15 h-full overflow-y-auto scrollbar",
            SectionKeyBindings { config_view, save_config }
            Section { name: "Buffs",
                div { class: "grid grid-cols-5 gap-4",
                    Buff {}
                    Buff {}
                    Buff {}
                    Buff {}
                    Buff {}
                    Buff {}
                    Buff {}
                }
            }
            Section { name: "Fixed actions" }
            SectionOthers { config_view, save_config }
        }
        div { class: "flex items-center w-full h-10 bg-gray-950 absolute bottom-0",
            TextSelect {
                class: "h-6 flex-grow",
                options: config_names(),
                disabled: false,
                on_create: move |name| {
                    coroutine.send(ConfigurationUpdate::Create(name));
                    coroutine.send(ConfigurationUpdate::Set);
                },
                on_delete: move |_| {
                    coroutine.send(ConfigurationUpdate::Delete);
                },
                on_select: move |(index, _)| {
                    let selected = configs.peek().as_ref().unwrap().get(index).cloned().unwrap();
                    config.set(Some(selected));
                    coroutine.send(ConfigurationUpdate::Set);
                },
                selected: config_index(),
            }
        }
    }
}

#[component]
fn Section(name: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "flex flex-col pr-4 pb-3",
            div { class: "flex items-center title-xs h-10", {name} }
            {children}
        }
    }
}

#[component]
fn SectionKeyBindings(
    config_view: Memo<Configuration>,
    save_config: Callback<Configuration>,
) -> Element {
    rsx! {
        Section { name: "Key bindings",
            div { class: "grid grid-cols-2 gap-4",
                KeyBindingConfigurationInput {
                    label: "Rope lift",
                    optional: true,
                    on_value: move |ropelift_key| {
                        save_config(Configuration {
                            ropelift_key,
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().ropelift_key,
                }
                KeyBindingConfigurationInput {
                    label: "Teleport",
                    optional: true,
                    on_value: move |teleport_key| {
                        save_config(Configuration {
                            teleport_key,
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().teleport_key,
                }
                KeyBindingConfigurationInput {
                    label: "Jump",
                    on_value: move |key_config: Option<KeyBindingConfiguration>| {
                        save_config(Configuration {
                            jump_key: key_config.expect("not optional"),
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().jump_key,
                }
                KeyBindingConfigurationInput {
                    label: "Up jump",
                    optional: true,
                    on_value: move |up_jump_key| {
                        save_config(Configuration {
                            up_jump_key,
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().up_jump_key,
                }
                KeyBindingConfigurationInput {
                    label: "Interact",
                    on_value: move |key_config: Option<KeyBindingConfiguration>| {
                        save_config(Configuration {
                            interact_key: key_config.expect("not optional"),
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().interact_key,
                }
                KeyBindingConfigurationInput {
                    label: "Cash shop",
                    on_value: move |key_config: Option<KeyBindingConfiguration>| {
                        save_config(Configuration {
                            cash_shop_key: key_config.expect("not optional"),
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().cash_shop_key,
                }
                KeyBindingConfigurationInput {
                    label: "Maple guide",
                    on_value: move |key_config: Option<KeyBindingConfiguration>| {
                        save_config(Configuration {
                            maple_guide_key: key_config.expect("not optional"),
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().maple_guide_key,
                }
                KeyBindingConfigurationInput {
                    label: "Change channel",
                    on_value: move |key_config: Option<KeyBindingConfiguration>| {
                        save_config(Configuration {
                            change_channel_key: key_config.expect("not optional"),
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().change_channel_key,
                }
                KeyBindingConfigurationInput {
                    label: "Feed pet",
                    on_value: move |key_config: Option<KeyBindingConfiguration>| {
                        save_config(Configuration {
                            feed_pet_key: key_config.expect("not optional"),
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().feed_pet_key,
                }
                KeyBindingConfigurationInput {
                    label: "Potion",
                    on_value: move |key_config: Option<KeyBindingConfiguration>| {
                        save_config(Configuration {
                            potion_key: key_config.expect("not optional"),
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().potion_key,
                }
                div { class: "col-span-full grid-cols-3 grid gap-2 justify-items-stretch",
                    KeyBindingConfigurationInput {
                        label: "Familiar menu",
                        on_value: move |key_config: Option<KeyBindingConfiguration>| {
                            save_config(Configuration {
                                familiar_menu_key: key_config.expect("not optional"),
                                ..config_view.peek().clone()
                            });
                        },
                        value: config_view().familiar_menu_key,
                    }
                    KeyBindingConfigurationInput {
                        label: "Familiar skill",
                        on_value: move |key_config: Option<KeyBindingConfiguration>| {
                            save_config(Configuration {
                                familiar_buff_key: key_config.expect("not optional"),
                                ..config_view.peek().clone()
                            });
                        },
                        value: config_view().familiar_buff_key,
                    }
                    KeyBindingConfigurationInput {
                        label: "Familiar essence",
                        on_value: move |key_config: Option<KeyBindingConfiguration>| {
                            save_config(Configuration {
                                familiar_essence_key: key_config.expect("not optional"),
                                ..config_view.peek().clone()
                            });
                        },
                        value: config_view().familiar_essence_key,
                    }
                }
            }
        }
    }
}

#[component]
fn SectionOthers(
    config_view: Memo<Configuration>,
    save_config: Callback<Configuration>,
) -> Element {
    rsx! {
        Section { name: "Others",
            div { class: "grid grid-cols-2 gap-4",
                CharactersMillisInput {
                    label: "Feed pet every milliseconds",
                    on_value: move |feed_pet_millis| {
                        save_config(Configuration {
                            feed_pet_millis,
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().feed_pet_millis,
                }
                div {} // Spacer

                CharactersSelect::<PotionMode> {
                    label: "Use potion mode",
                    on_select: move |potion_mode| {
                        save_config(Configuration {
                            potion_mode,
                            ..config_view.peek().clone()
                        });
                    },
                    selected: config_view().potion_mode,
                }
                match config_view().potion_mode {
                    PotionMode::EveryMillis(millis) => rsx! {
                        CharactersMillisInput {
                            label: "Use potion every milliseconds",
                            on_value: move |millis| {
                                save_config(Configuration {
                                    potion_mode: PotionMode::EveryMillis(millis),
                                    ..config_view.peek().clone()
                                });
                            },
                            value: millis,
                        }
                    },
                    PotionMode::Percentage(percent) => rsx! {
                        CharactersPercentageInput {
                            label: "Use potion health below percentage",
                            on_value: move |percent| {
                                save_config(Configuration {
                                    potion_mode: PotionMode::Percentage(percent),
                                    ..config_view.peek().clone()
                                });
                            },
                            value: percent,
                        }
                    },
                }

                CharactersSelect::<Class> {
                    label: "Link key timing class",
                    on_select: move |class| {
                        save_config(Configuration {
                            class,
                            ..config_view.peek().clone()
                        });
                    },
                    selected: config_view().class,
                }
                CharactersCheckbox {
                    label: "Disable walking",
                    on_value: move |disable_adjusting| {
                        save_config(Configuration {
                            disable_adjusting,
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().disable_adjusting,
                }
            }
        }
    }
}

#[component]
fn Buff() -> Element {
    rsx! {
        div { class: "w-7 h-7 bg-gray-800" }
    }
}

#[component]
fn KeyBindingConfigurationInput(
    label: &'static str,
    #[props(default = false)] optional: bool,
    on_value: EventHandler<Option<KeyBindingConfiguration>>,
    value: Option<KeyBindingConfiguration>,
) -> Element {
    let label = if optional {
        format!("{label} (optional)")
    } else {
        label.to_string()
    };

    rsx! {
        KeyBindingInput {
            label,
            label_class: INPUT_LABEL_CLASS,
            div_class: INPUT_DIV_CLASS,
            input_class: KEY_INPUT_CLASS,
            optional,
            on_value: move |new_value: Option<KeyBinding>| {
                let new_value = new_value
                    .map(|key| {
                        let mut config = value.unwrap_or_default();
                        config.key = key;
                        config
                    });
                on_value(new_value);
            },
            value: value.map(|config| config.key),
        }
    }
}

#[component]
fn CharactersCheckbox(label: &'static str, on_value: EventHandler<bool>, value: bool) -> Element {
    rsx! {
        Checkbox {
            label,
            label_class: INPUT_LABEL_CLASS,
            div_class: INPUT_DIV_CLASS,
            input_class: "w-6 h-6 border border-gray-600",
            on_value,
            value,
        }
    }
}

#[component]
fn CharactersSelect<T: 'static + Clone + PartialEq + Display + IntoEnumIterator>(
    label: &'static str,
    on_select: EventHandler<T>,
    selected: T,
) -> Element {
    rsx! {
        EnumSelect {
            label,
            label_class: INPUT_LABEL_CLASS,
            div_class: INPUT_DIV_CLASS,
            select_class: format!("{INPUT_CLASS} items-center picker:scrollbar"),
            option_class: "bg-gray-900 paragraph-xs pl-1 pr-2 hover:bg-gray-800",
            on_select,
            selected,
        }
    }
}

#[component]
fn CharactersPercentageInput(
    label: &'static str,
    on_value: EventHandler<f32>,
    value: f32,
) -> Element {
    rsx! {
        PercentageInput {
            label,
            label_class: INPUT_LABEL_CLASS,
            div_class: INPUT_DIV_CLASS,
            input_class: INPUT_CLASS,
            on_value,
            value,
        }
    }
}

#[component]
fn CharactersMillisInput(label: &'static str, on_value: EventHandler<u64>, value: u64) -> Element {
    rsx! {
        MillisInput {
            label,
            label_class: INPUT_LABEL_CLASS,
            div_class: INPUT_DIV_CLASS,
            input_class: INPUT_CLASS,
            on_value,
            value,
        }
    }
}
