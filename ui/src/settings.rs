use std::{fmt::Display, mem};

use backend::{
    CaptureMode, InputMethod, IntoEnumIterator, KeyBinding, KeyBindingConfiguration, Notifications,
    Settings, WebhookProvider, query_capture_handles, query_settings, refresh_capture_handles,
    select_capture_handle, upsert_settings,
};
use dioxus::{html::FileData, prelude::*};
use futures_util::StreamExt;

use crate::{
    AppState,
    components::{
        button::{Button, ButtonStyle},
        checkbox::Checkbox,
        duration::DurationInput,
        file::{FileInput, FileOutput},
        icons::{EyePasswordHideIcon, EyePasswordShowIcon},
        key::KeyInput,
        labeled::Labeled,
        numbers::MillisInput,
        section::Section,
        select::{Select, SelectOption},
        text::TextInput,
    },
};

#[derive(Debug)]
enum SettingsUpdate {
    Update(Settings),
}

#[derive(PartialEq, Clone)]
struct SettingsContext {
    settings: Memo<Settings>,
    save_settings: Callback<Settings>,
}

#[component]
pub fn SettingsScreen() -> Element {
    let mut settings = use_context::<AppState>().settings;
    let settings_view = use_memo(move || settings().unwrap_or_default());

    // Handles async operations for settings-related
    let coroutine = use_coroutine(
        move |mut rx: UnboundedReceiver<SettingsUpdate>| async move {
            while let Some(message) = rx.next().await {
                match message {
                    SettingsUpdate::Update(new_settings) => {
                        settings.set(Some(upsert_settings(new_settings).await));
                    }
                }
            }
        },
    );

    let save_settings = use_callback(move |new_settings: Settings| {
        coroutine.send(SettingsUpdate::Update(new_settings));
    });

    use_context_provider(|| SettingsContext {
        settings: settings_view,
        save_settings,
    });

    use_future(move || async move {
        if settings.peek().is_none() {
            settings.set(Some(query_settings().await));
        }
    });

    rsx! {
        div { class: "flex flex-col h-full overflow-y-auto",
            SectionCapture {}
            SectionInput {}
            SectionNotifications {}
            SectionHotkeys {}
            SectionRunTimer {}
            SectionOthers {}
        }
    }
}

#[component]
fn SectionCapture() -> Element {
    let context = use_context::<SettingsContext>();
    let settings = context.settings;
    let save_settings = context.save_settings;

    let mut selected_handle_index = use_signal(|| None);
    let mut handle_names = use_resource(move || async move {
        let (names, selected) = query_capture_handles().await;
        selected_handle_index.set(selected);
        names
    });
    let handle_names_with_default = use_memo(move || {
        let default = vec!["Default".to_string()];
        let names = handle_names().unwrap_or_default();

        [default, names].concat()
    });

    rsx! {
        Section { title: "Capture",
            div { class: "grid grid-cols-2 gap-3",
                SettingsSelect {
                    label: "Handle",
                    options: handle_names_with_default(),
                    on_selected: move |index| async move {
                        if index == 0 {
                            selected_handle_index.set(None);
                            select_capture_handle(None).await;
                        } else {
                            selected_handle_index.set(Some(index - 1));
                            select_capture_handle(Some(index - 1)).await;
                        }
                    },
                    selected: selected_handle_index().map(|index| index + 1).unwrap_or_default(),
                }
                SettingsEnumSelect::<CaptureMode> {
                    label: "Mode",
                    on_selected: move |capture_mode| {
                        save_settings(Settings {
                            capture_mode,
                            ..settings.peek().clone()
                        });
                    },
                    selected: settings().capture_mode,
                }
            }
            Button {
                style: ButtonStyle::Secondary,
                on_click: move |_| async move {
                    refresh_capture_handles().await;
                    handle_names.restart();
                },
                class: "mt-2",

                "Refresh handles"
            }
        }
    }
}

#[component]
fn SectionInput() -> Element {
    let context = use_context::<SettingsContext>();
    let settings = context.settings;
    let save_settings = context.save_settings;

    rsx! {
        Section { title: "Input",
            div { class: "grid grid-cols-3 gap-3",
                SettingsEnumSelect::<InputMethod> {
                    label: "Method",
                    on_selected: move |input_method| async move {
                        save_settings(Settings {
                            input_method,
                            ..settings.peek().clone()
                        });
                    },
                    selected: settings().input_method,
                }
                SettingsTextInput {
                    text_label: "RPC server URL",
                    button_label: "Update",
                    on_value: move |input_method_rpc_server_url| {
                        save_settings(Settings {
                            input_method_rpc_server_url,
                            ..settings.peek().clone()
                        });
                    },
                    value: settings().input_method_rpc_server_url,
                }
            }
        }
    }
}

#[component]
fn SectionNotifications() -> Element {
    let context = use_context::<SettingsContext>();
    let settings = context.settings;
    let save_settings = context.save_settings;
    let notifications = use_memo(move || settings().notifications);

    rsx! {
        Section { title: "Notifications",
            div { class: "grid grid-cols-2 gap-3 mb-2",
                SettingsEnumSelect::<WebhookProvider> {
                    label: "Webhook provider",
                    on_selected: move |webhook_provider| {
                        save_settings(Settings {
                            notifications: Notifications {
                                webhook_provider,
                                ..notifications.peek().clone()
                            },
                            ..settings.peek().clone()
                        });
                    },
                    selected: settings().notifications.webhook_provider,
                }
                div {}
                SettingsTextInput {
                    text_label: "Webhook URL",
                    button_label: "Update",
                    sensitive: true,
                    on_value: move |webhook_url| {
                        save_settings(Settings {
                            notifications: Notifications {
                                webhook_url,
                                ..notifications.peek().clone()
                            },
                            ..settings.peek().clone()
                        });
                    },
                    value: notifications().webhook_url,
                }
                SettingsTextInput {
                    text_label: "Discord ping user ID",
                    button_label: "Update",
                    sensitive: true,
                    on_value: move |discord_user_id| {
                        save_settings(Settings {
                            notifications: Notifications {
                                discord_user_id,
                                ..notifications.peek().clone()
                            },
                            ..settings.peek().clone()
                        });
                    },
                    value: notifications().discord_user_id,
                }
            }
            div { class: "grid grid-cols-3 gap-3",
                SettingsCheckbox {
                    label: "Rune spawns",
                    on_checked: move |notify_on_rune_appear| {
                        save_settings(Settings {
                            notifications: Notifications {
                                notify_on_rune_appear,
                                ..notifications.peek().clone()
                            },
                            ..settings.peek().clone()
                        });
                    },
                    checked: notifications().notify_on_rune_appear,
                }
                SettingsCheckbox {
                    label: "Elite boss spawns",
                    on_checked: move |notify_on_elite_boss_appear| {
                        save_settings(Settings {
                            notifications: Notifications {
                                notify_on_elite_boss_appear,
                                ..notifications.peek().clone()
                            },
                            ..settings.peek().clone()
                        });
                    },
                    checked: notifications().notify_on_elite_boss_appear,
                }
                SettingsCheckbox {
                    label: "Player dies",
                    on_checked: move |notify_on_player_die| {
                        save_settings(Settings {
                            notifications: Notifications {
                                notify_on_player_die,
                                ..notifications.peek().clone()
                            },
                            ..settings.peek().clone()
                        });
                    },
                    checked: notifications().notify_on_player_die,
                }
                SettingsCheckbox {
                    label: "Guildie appears",
                    on_checked: move |notify_on_player_guildie_appear| {
                        save_settings(Settings {
                            notifications: Notifications {
                                notify_on_player_guildie_appear,
                                ..notifications.peek().clone()
                            },
                            ..settings.peek().clone()
                        });
                    },
                    checked: notifications().notify_on_player_guildie_appear,
                }
                SettingsCheckbox {
                    label: "Stranger appears",
                    on_checked: move |notify_on_player_stranger_appear| {
                        save_settings(Settings {
                            notifications: Notifications {
                                notify_on_player_stranger_appear,
                                ..notifications.peek().clone()
                            },
                            ..settings.peek().clone()
                        });
                    },
                    checked: notifications().notify_on_player_stranger_appear,
                }
                SettingsCheckbox {
                    label: "Friend appears",
                    on_checked: move |notify_on_player_friend_appear| {
                        save_settings(Settings {
                            notifications: Notifications {
                                notify_on_player_friend_appear,
                                ..notifications.peek().clone()
                            },
                            ..settings.peek().clone()
                        });
                    },
                    checked: notifications().notify_on_player_friend_appear,
                }
                SettingsCheckbox {
                    label: "Detection fails or map changes",
                    on_checked: move |notify_on_fail_or_change_map| {
                        save_settings(Settings {
                            notifications: Notifications {
                                notify_on_fail_or_change_map,
                                ..notifications.peek().clone()
                            },
                            ..settings.peek().clone()
                        });
                    },
                    checked: notifications().notify_on_fail_or_change_map,
                }
                SettingsCheckbox {
                    label: "Lie detector appears",
                    on_checked: move |notify_on_lie_detector_appear| {
                        save_settings(Settings {
                            notifications: Notifications {
                                notify_on_lie_detector_appear,
                                ..notifications.peek().clone()
                            },
                            ..settings.peek().clone()
                        });
                    },
                    checked: notifications().notify_on_lie_detector_appear,
                }
                SettingsCheckbox {
                    label: "Run timer ends",
                    on_checked: move |notify_on_run_timer_end| {
                        save_settings(Settings {
                            notifications: Notifications {
                                notify_on_run_timer_end,
                                ..notifications.peek().clone()
                            },
                            ..settings.peek().clone()
                        });
                    },
                    checked: notifications().notify_on_run_timer_end,
                }
            }
        }
    }
}

#[component]
fn SectionHotkeys() -> Element {
    #[component]
    fn Hotkey(
        label: &'static str,
        on_value: Callback<KeyBindingConfiguration>,
        value: KeyBindingConfiguration,
    ) -> Element {
        rsx! {
            div { class: "flex gap-2",
                SettingsKeyInput {
                    label,
                    class: "flex-grow",
                    on_value: move |new_value: KeyBinding| {
                        on_value(KeyBindingConfiguration {
                            key: new_value,
                            ..value
                        });
                    },
                    value: value.key,
                }
                SettingsCheckbox {
                    label: "Enabled",
                    on_checked: move |enabled| {
                        on_value(KeyBindingConfiguration {
                            enabled,
                            ..value
                        });
                    },
                    checked: value.enabled,
                }
            }
        }
    }

    let context = use_context::<SettingsContext>();
    let settings = context.settings;
    let save_settings = context.save_settings;

    rsx! {
        Section { title: "Hotkeys",
            div { class: "grid grid-cols-2 gap-3",
                Hotkey {
                    label: "Toggle start/stop actions",
                    on_value: move |toggle_actions_key| {
                        save_settings(Settings {
                            toggle_actions_key,
                            ..settings.peek().clone()
                        });
                    },
                    value: settings().toggle_actions_key,
                }
                Hotkey {
                    label: "Add platform",
                    on_value: move |platform_add_key| {
                        save_settings(Settings {
                            platform_add_key,
                            ..settings.peek().clone()
                        });
                    },
                    value: settings().platform_add_key,
                }
                Hotkey {
                    label: "Mark platform start",
                    on_value: move |platform_start_key| {
                        save_settings(Settings {
                            platform_start_key,
                            ..settings.peek().clone()
                        });
                    },
                    value: settings().platform_start_key,
                }
                Hotkey {
                    label: "Mark platform end",
                    on_value: move |platform_end_key| {
                        save_settings(Settings {
                            platform_end_key,
                            ..settings.peek().clone()
                        });
                    },
                    value: settings().platform_end_key,
                }
            }
        }
    }
}

#[component]
fn SectionRunTimer() -> Element {
    let context = use_context::<SettingsContext>();
    let settings = context.settings;
    let save_settings = context.save_settings;

    rsx! {
        Section { title: "Run timer",
            div { class: "grid grid-cols-2 gap-3",
                Labeled { label: "Duration (hh:mm:ss)",
                    DurationInput {
                        on_value: move |run_timer_millis| {
                            save_settings(Settings {
                                run_timer_millis,
                                ..settings.peek().clone()
                            });
                        },
                        value: settings().run_timer_millis,
                    }
                }
                SettingsCheckbox {
                    label: "Enabled",
                    on_checked: move |run_timer| {
                        save_settings(Settings {
                            run_timer,
                            ..settings.peek().clone()
                        });
                    },
                    checked: settings().run_timer,
                }
            }
        }
    }
}

#[component]
fn SectionOthers() -> Element {
    let context = use_context::<SettingsContext>();
    let settings = context.settings;
    let save_settings = context.save_settings;

    let import_settings = use_callback(move |file: FileData| async move {
        let Some(id) = settings.peek().id else {
            return;
        };
        let Ok(bytes) = file.read_bytes().await else {
            return;
        };
        let Ok(mut settings) = serde_json::from_slice::<'_, Settings>(&bytes) else {
            return;
        };
        settings.id = Some(id);
        save_settings(settings);
    });

    rsx! {
        Section { title: "Others",
            div { class: "grid grid-cols-2 gap-3",
                SettingsCheckbox {
                    label: "Enable rune solving",
                    on_checked: move |enable_rune_solving| {
                        save_settings(Settings {
                            enable_rune_solving,
                            ..settings.peek().clone()
                        });
                    },
                    checked: settings().enable_rune_solving,
                }
                SettingsCheckbox {
                    label: "Enable transparent shape solving",
                    on_checked: move |enable_transparent_shape_solving| {
                        save_settings(Settings {
                            enable_transparent_shape_solving,
                            ..settings.peek().clone()
                        });
                    },
                    checked: settings().enable_transparent_shape_solving,
                }
                SettingsCheckbox {
                    label: "Enable Violetta solving",
                    on_checked: move |enable_violetta_solving| {
                        save_settings(Settings {
                            enable_violetta_solving,
                            ..settings.peek().clone()
                        });
                    },
                    checked: settings().enable_violetta_solving,
                }
                SettingsCheckbox {
                    label: "Enable panic mode",
                    on_checked: move |enable_panic_mode| {
                        save_settings(Settings {
                            enable_panic_mode,
                            ..settings.peek().clone()
                        });
                    },
                    checked: settings().enable_panic_mode,
                }
                SettingsCheckbox {
                    label: "Stop actions on fail or map changed",
                    on_checked: move |stop_on_fail_or_change_map| {
                        save_settings(Settings {
                            stop_on_fail_or_change_map,
                            ..settings.peek().clone()
                        });
                    },
                    checked: settings().stop_on_fail_or_change_map,
                }
                SettingsCheckbox {
                    label: "Stop actions on player dies",
                    on_checked: move |stop_on_player_die| {
                        save_settings(Settings {
                            stop_on_player_die,
                            ..settings.peek().clone()
                        });
                    },
                    checked: settings().stop_on_player_die,
                }
                div { class: "col-span-full",
                    div { class: "grid grid-cols-2 gap-3",
                        FileInput {
                            on_file: move |file| async move {
                                import_settings(file).await;
                            },
                            Button { class: "w-full", style: ButtonStyle::Primary, "Import" }
                        }

                        FileOutput {
                            on_file: move |_| { serde_json::to_vec_pretty(&*settings.peek()).unwrap_or_default() },
                            download: "settings.json",
                            Button { class: "w-full", style: ButtonStyle::Primary, "Export" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsSelect<T: 'static + Clone + PartialEq + Display>(
    label: &'static str,
    options: Vec<T>,
    on_selected: Callback<usize>,
    selected: usize,
) -> Element {
    rsx! {
        Labeled { label,
            Select::<usize> { on_selected,

                for (i , value) in options.into_iter().enumerate() {
                    SelectOption::<usize> {
                        value: i,
                        label: value.to_string(),
                        selected: selected == i,
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsMillisInput(
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
fn SettingsEnumSelect<T: 'static + Clone + PartialEq + Display + IntoEnumIterator>(
    label: &'static str,
    #[props(default)] disabled: bool,
    on_selected: Callback<T>,
    selected: ReadSignal<T>,
) -> Element {
    let selected_equal =
        use_callback(move |value: T| mem::discriminant(&selected()) == mem::discriminant(&value));

    rsx! {
        Labeled { label,
            Select::<T> { on_selected, disabled,

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
fn SettingsCheckbox(
    label: &'static str,
    #[props(default)] disabled: bool,
    on_checked: Callback<bool>,
    checked: bool,
) -> Element {
    rsx! {
        Labeled { label,
            Checkbox { disabled, on_checked, checked }
        }
    }
}

#[component]
fn SettingsKeyInput(
    label: &'static str,
    class: String,
    on_value: Callback<KeyBinding>,
    value: KeyBinding,
) -> Element {
    rsx! {
        Labeled { label, class,
            KeyInput {
                on_value: move |key: Option<KeyBinding>| {
                    on_value(key.expect("not optional"));
                },
                value: Some(value),
            }
        }
    }
}

#[component]
fn SettingsTextInput(
    text_label: String,
    button_label: String,
    #[props(default)] sensitive: bool,
    on_value: Callback<String>,
    value: String,
) -> Element {
    const EYE_ICON_CLASS: &str = "size-4";

    let mut text = use_signal(String::default);
    let mut hidden = use_signal(|| sensitive);

    use_effect(use_reactive!(|value| text.set(value)));

    rsx! {
        div { class: "relative group",
            Labeled { label: text_label,
                TextInput {
                    class: "h-6",
                    sensitive: hidden(),
                    on_value: move |new_text| {
                        text.set(new_text);
                    },
                    value: text(),
                }
            }
            if sensitive {
                div {
                    class: "absolute right-1 bottom-1 invisible group-hover:visible bg-primary-surface",
                    onclick: move |_| {
                        hidden.toggle();
                    },
                    if hidden() {
                        EyePasswordShowIcon { class: EYE_ICON_CLASS }
                    } else {
                        EyePasswordHideIcon { class: EYE_ICON_CLASS }
                    }
                }
            }
        }
        div { class: "flex items-end",
            Button {
                class: "w-full mb-[1px]",
                style: ButtonStyle::Primary,
                on_click: move |_| {
                    on_value(text.peek().clone());
                },

                {button_label}
            }
        }
    }
}
