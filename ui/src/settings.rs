use std::{fmt::Display, str::FromStr};

use backend::{
    CaptureMode, InputMethod, IntoEnumIterator, KeyBindingConfiguration, PanicMode,
    Settings as SettingsData, query_capture_handles, select_capture_handle,
};
#[cfg(debug_assertions)]
use backend::{capture_image, infer_minimap, infer_rune, record_images, test_spin_rune};
use dioxus::prelude::*;

use crate::{
    AppMessage,
    input::{Checkbox, LabeledInput},
    key::KeyBindingConfigurationInput,
    select::{EnumSelect, Select},
};

const TOGGLE_ACTIONS: &str = "Start/Stop Actions";
const PLATFORM_START: &str = "Mark Platform Start";
const PLATFORM_END: &str = "Mark Platform End";
const PLATFORM_ADD: &str = "Add Platform";

const SELECT_DIV_CLASS: &str = "flex items-center space-x-4";
const SELECT_LABEL_CLASS: &str =
    "text-xs text-gray-700 flex-1 inline-block data-[disabled]:text-gray-400";
const SELECT_CLASS: &str = "w-44 h-7 text-xs text-gray-700 text-ellipsis border border-gray-300 rounded outline-none disabled:cursor-not-allowed disabled:text-gray-400";

#[component]
pub fn Settings(
    app_coroutine: Coroutine<AppMessage>,
    settings: ReadOnlySignal<Option<SettingsData>>,
) -> Element {
    let settings_view = use_memo(move || settings().unwrap_or_default());
    let active = use_signal(|| None);
    let on_settings = move |updated| {
        app_coroutine.send(AppMessage::UpdateSettings(updated));
    };
    #[cfg(debug_assertions)]
    let mut recording = use_signal(|| false);

    rsx! {
        div { class: "px-2 pb-2 pt-2 flex flex-col overflow-y-auto scrollbar h-full",
            ul { class: "list-disc text-xs text-gray-700 pl-4",
                li { class: "mb-1", "Platform keys must have a Map created and Platforms tab opened" }
                li { class: "mb-1", "BltBltArea can stay behind other windows but cannot be minimized" }
                li { class: "mb-1 font-bold",
                    "BitBltArea relies on high-quality game images for detection (e.g. no blurry)"
                }
                li { class: "mb-1 font-bold",
                    "When using BitBltArea, make sure the window on top of the capture area is the game or where the game images can be captured if the game is inside a something else (e.g. VM)"
                }
                li { class: "mb-1 font-bold",
                    "When using BitBltArea, the game must be contained inside the capture area even when resizing (e.g. going to cash shop)"
                }
                li { class: "mb-1 font-bold",
                    "When using BitBltArea, for key inputs to work, make sure the window on top of the capture area is focused by clicking it. For example, if you have Notepad on top of the game and focused, it will send input to the Notepad instead of the game."
                }
            }
            div { class: "h-2 border-b border-gray-300 mb-2" }
            div { class: "flex flex-col space-y-3.5",
                SettingsCheckbox {
                    label: "Enable Rune Solving",
                    on_input: move |enable_rune_solving| {
                        on_settings(SettingsData {
                            enable_rune_solving,
                            ..settings_view.peek().clone()
                        });
                    },
                    value: settings_view().enable_rune_solving,
                }
                SettingsCheckbox {
                    label: "Enable Change Channel On Elite Boss",
                    on_input: move |enable_change_channel_on_elite_boss_appear| {
                        on_settings(SettingsData {
                            enable_change_channel_on_elite_boss_appear,
                            ..settings_view.peek().clone()
                        });
                    },
                    value: settings_view().enable_change_channel_on_elite_boss_appear,
                }
                SettingsCheckbox {
                    label: "Enable Panic Mode",
                    on_input: move |enable_panic_mode| {
                        on_settings(SettingsData {
                            enable_panic_mode,
                            ..settings_view.peek().clone()
                        });
                    },
                    value: settings_view().enable_panic_mode,
                }
                SettingsEnumSelect::<PanicMode> {
                    label: "Panic Mode",
                    on_select: move |panic_mode| {
                        on_settings(SettingsData {
                            panic_mode,
                            ..settings_view.peek().clone()
                        });
                    },
                    disabled: false,
                    selected: settings_view().panic_mode,
                }
                SettingsCheckbox {
                    label: "Stop Actions If Fails / Changes Map",
                    on_input: move |stop_on_fail_or_change_map| {
                        on_settings(SettingsData {
                            stop_on_fail_or_change_map,
                            ..settings_view.peek().clone()
                        });
                    },
                    value: settings_view().stop_on_fail_or_change_map,
                }
                SettingsEnumSelect::<CaptureMode> {
                    label: "Capture Mode",
                    on_select: move |capture_mode| {
                        on_settings(SettingsData {
                            capture_mode,
                            ..settings_view.peek().clone()
                        });
                    },
                    disabled: false,
                    selected: settings_view().capture_mode,
                }
                SettingsCaptureHandleSelect { settings_view }
                SettingsInputMethodSelect { app_coroutine, settings_view }
                KeyBindingConfigurationInput {
                    label: TOGGLE_ACTIONS,
                    label_active: active,
                    is_toggleable: true,
                    is_disabled: false,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_settings(SettingsData {
                            toggle_actions_key: key.unwrap(),
                            ..settings_view.peek().clone()
                        });
                    },
                    value: Some(settings_view().toggle_actions_key),
                }
                KeyBindingConfigurationInput {
                    label: PLATFORM_START,
                    label_active: active,
                    is_toggleable: true,
                    is_disabled: false,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_settings(SettingsData {
                            platform_start_key: key.unwrap(),
                            ..settings_view.peek().clone()
                        });
                    },
                    value: Some(settings_view().platform_start_key),
                }
                KeyBindingConfigurationInput {
                    label: PLATFORM_END,
                    label_active: active,
                    is_toggleable: true,
                    is_disabled: false,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_settings(SettingsData {
                            platform_end_key: key.unwrap(),
                            ..settings_view.peek().clone()
                        });
                    },
                    value: Some(settings_view().platform_end_key),
                }
                KeyBindingConfigurationInput {
                    label: PLATFORM_ADD,
                    label_active: active,
                    is_toggleable: true,
                    is_disabled: false,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_settings(SettingsData {
                            platform_add_key: key.unwrap(),
                            ..settings_view.peek().clone()
                        });
                    },
                    value: Some(settings_view().platform_add_key),
                }
                {
                    #[cfg(debug_assertions)]
                    rsx! {
                        SettingsDebugButton {
                            label: "Capture Color Image",
                            on_click: move |_| async {
                                capture_image(false).await;
                            },
                        }
                        SettingsDebugButton {
                            label: "Capture Grayscale Image",
                            on_click: move |_| async {
                                capture_image(true).await;
                            },
                        }
                        SettingsDebugButton {
                            label: "Infer Rune",
                            on_click: move |_| async {
                                infer_rune().await;
                            },
                        }
                        SettingsDebugButton {
                            label: "Infer Minimap",
                            on_click: move |_| async {
                                infer_minimap().await;
                            },
                        }
                        SettingsDebugButton {
                            label: if recording() { "Stop Recording" } else { "Start Recording" },
                            on_click: move |_| async move {
                                let current = *recording.peek();
                                record_images(!current).await;
                                recording.set(!current);
                            },
                        }
                        SettingsDebugButton {
                            label: "Sandbox Spin Rune Test",
                            on_click: move |_| async {
                                test_spin_rune().await;
                            },
                        }
                    }
                }
            }
        }
    }
}

#[cfg(debug_assertions)]
#[component]
fn SettingsDebugButton(label: String, on_click: EventHandler) -> Element {
    rsx! {
        button {
            class: "button-primary h-8",
            onclick: move |_| {
                on_click(());
            },
            {label}
        }
    }
}

// TODO: Needs to group settings components
#[component]
pub fn SettingsCheckbox(label: String, on_input: EventHandler<bool>, value: bool) -> Element {
    rsx! {
        Checkbox {
            label,
            label_class: "text-xs text-gray-700 flex-1 inline-block data-[disabled]:text-gray-400",
            div_class: "flex items-center space-x-4 mt-2",
            input_class: "w-44 text-xs text-gray-700 text-ellipsis rounded outline-none disabled:cursor-not-allowed disabled:text-gray-400",
            disabled: false,
            on_input: move |checked| {
                on_input(checked);
            },
            value,
        }
    }
}

#[component]
pub fn SettingsTextInput(label: String, on_input: EventHandler<String>, value: String) -> Element {
    let mut value = use_signal(move || value);

    rsx! {
        LabeledInput {
            label,
            label_class: "text-xs text-gray-700 flex-1 inline-block data-[disabled]:text-gray-400",
            div_class: "flex space-x-2 items-center",
            disabled: false,
            input {
                class: "w-24 text-gray-700 text-xs p-1 border rounded border-gray-300",
                oninput: move |e| {
                    value.set(e.parsed::<String>().unwrap_or_default());
                },
                value: value(),
            }
            button {
                class: "button-primary w-18 h-full",
                onclick: move |_| {
                    on_input(value.peek().clone());
                },
                "Update"
            }
        }
    }
}

#[component]
fn SettingsInputMethodSelect(
    app_coroutine: Coroutine<AppMessage>,
    settings_view: Memo<SettingsData>,
) -> Element {
    let on_settings = move |updated| {
        app_coroutine.send(AppMessage::UpdateSettings(updated));
    };

    rsx! {
        SettingsEnumSelect::<InputMethod> {
            label: "Input Method",
            on_select: move |input_method| {
                on_settings(SettingsData {
                    input_method,
                    ..settings_view.peek().clone()
                });
            },
            disabled: false,
            selected: settings_view().input_method,
        }
        if matches!(settings_view().input_method, InputMethod::Rpc) {
            SettingsTextInput {
                label: "Server URL",
                on_input: move |url| {
                    on_settings(SettingsData {
                        input_method_rpc_server_url: url,
                        ..settings_view.peek().clone()
                    });
                },
                value: settings_view().input_method_rpc_server_url,
            }
        }
    }
}

// Dupe them till hard to manage
#[component]
pub fn SettingsEnumSelect<T: 'static + Clone + PartialEq + Display + FromStr + IntoEnumIterator>(
    label: String,
    on_select: EventHandler<T>,
    disabled: bool,
    selected: T,
) -> Element {
    rsx! {
        EnumSelect {
            label,
            disabled,
            div_class: SELECT_DIV_CLASS,
            label_class: SELECT_LABEL_CLASS,
            select_class: SELECT_CLASS,
            on_select: move |variant: T| {
                on_select(variant);
            },
            selected,
        }
    }
}

#[component]
fn SettingsCaptureHandleSelect(settings_view: Memo<SettingsData>) -> Element {
    const HANDLE_NOT_SELECTED: usize = usize::MAX;
    const HANDLES_REFRESH: usize = usize::MAX - 1;

    let mut selected_capture_handle = use_signal(|| None);
    let mut capture_handles = use_resource(move || async move {
        let (names, selected) = query_capture_handles().await;
        selected_capture_handle.set(selected);
        names
    });

    use_effect(move || {
        let index = selected_capture_handle();
        spawn(async move {
            select_capture_handle(index).await;
        });
    });

    rsx! {
        Select::<usize> {
            label: "Capture Handle",
            div_class: SELECT_DIV_CLASS,
            label_class: SELECT_LABEL_CLASS,
            select_class: SELECT_CLASS,
            options: match capture_handles() {
                Some(names) => {
                    [(HANDLE_NOT_SELECTED, "Default".to_string())]
                        .into_iter()
                        .chain(names.into_iter().enumerate())
                        .chain([(HANDLES_REFRESH, "Refresh handles...".to_string())])
                        .collect()
                }
                None => vec![],
            },
            disabled: matches!(settings_view().capture_mode, CaptureMode::BitBltArea),
            on_select: move |(_, i)| {
                if i == HANDLE_NOT_SELECTED {
                    selected_capture_handle.set(None);
                } else if i == HANDLES_REFRESH {
                    capture_handles.restart();
                } else {
                    selected_capture_handle.set(Some(i));
                }
            },
            selected: selected_capture_handle().unwrap_or(HANDLE_NOT_SELECTED),
        }
    }
}
