use backend::{Configuration, delete_config, query_configs, update_configuration, upsert_config};
use dioxus::prelude::*;
use futures_util::StreamExt;
use tokio::task::spawn_blocking;

use crate::{AppState, inputs::KeyBindingInput, select::TextSelect};

#[derive(Debug)]
enum ConfigurationUpdate {
    Set,
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
                .iter()
                .enumerate()
                .find(|(_, cfg)| config.id == cfg.id)
                .map(|(i, _)| i)
        })
    });
    // Handles async operations for configuration-related
    let coroutine = use_coroutine(
        move |mut rx: UnboundedReceiver<ConfigurationUpdate>| async move {
            while let Some(message) = rx.next().await {
                match message {
                    ConfigurationUpdate::Set => {
                        update_configuration(config().expect("config must be arleady set")).await;
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
    let active_key_label = use_signal(|| None);

    // Sets a configuration if there is not one
    use_effect(move || {
        if let Some(configs) = configs()
            && config.peek().is_none()
        {
            config.set(configs.first().cloned());
            coroutine.send(ConfigurationUpdate::Set);
        }
    });

    rsx! {
        div { class: "flex flex-col mb-10 h-110 overflow-y-auto scrollbar",
            Section { name: "Key Bindings",
                div { class: "grid grid-cols-2 gap-3",
                    KeyBinding { label: "Rope Lift", active_key_label }
                    KeyBinding { label: "Teleport", active_key_label }
                    KeyBinding { label: "Jump", active_key_label }
                    KeyBinding { label: "Up Jump", active_key_label }
                    KeyBinding { label: "Interact", active_key_label }
                    KeyBinding { label: "Cash Shop", active_key_label }
                    KeyBinding { label: "Maple Guide", active_key_label }
                    KeyBinding { label: "Change Channel", active_key_label }
                    KeyBinding { label: "Potion", active_key_label }
                    div { class: "col-span-full flex gap-2",
                        KeyBinding { label: "Familiar Menu", active_key_label }
                        KeyBinding { label: "Familiar Skill", active_key_label }
                        KeyBinding { label: "Familiar Essence", active_key_label }
                    }
                }
            }
            Section { name: "Buffs",
                div { class: "grid grid-cols-5 gap-3",
                    Buff {}
                    Buff {}
                    Buff {}
                    Buff {}
                    Buff {}
                    Buff {}
                    Buff {}
                }
            }
            Section { name: "Fixed Actions" }
            Section { name: "Others" }
        }
        div { class: "flex items-center w-full h-10 absolute bottom-0",
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
fn Section(name: String, children: Element) -> Element {
    rsx! {
        div { class: "flex flex-col px-4 pb-3",
            div { class: "flex items-center title-xs h-10", {name} }
            {children}
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
fn KeyBinding(label: &'static str, active_key_label: Signal<Option<&'static str>>) -> Element {
    let is_active = use_memo(move || active_key_label() == Some(label));

    rsx! {
        KeyBindingInput {
            label,
            label_class: "text-[11px] text-gray-400",
            div_class: "flex flex-col gap-1",
            input_class: "h-6",
            disabled: false,
            on_input: |key| {},
            value: None,
        }
    }
}
