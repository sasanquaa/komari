use std::fmt::Display;

use backend::{
    Action, ActionCondition, ActionKey, ActionKeyDirection, ActionKeyWith, IntoEnumIterator,
    KeyBinding, LinkKeyBinding, Position, update_minimap, upsert_map,
};
use dioxus::{cli_config::is_cli_enabled, prelude::*};
use futures_util::StreamExt;
use tokio::task::spawn_blocking;

use crate::{
    AppState,
    inputs::{Checkbox, KeyBindingInput, MillisInput, NumberInputI32, NumberInputU32},
    select::{EnumSelect, TextSelect},
};

const INPUT_LABEL_CLASS: &str = "label";
const INPUT_DIV_CLASS: &str = "flex flex-col gap-1";
const KEY_INPUT_CLASS: &str = "h-6 border border-gray-600 disabled:cursor-not-allowed";
const INPUT_CLASS: &str = "h-6 px-1 w-full paragraph-xs outline-none border border-gray-600 disabled:text-gray-600 disabled:cursor-not-allowed";

#[derive(Debug)]
enum ActionUpdate {
    Set,
    Create(String),
    Delete(String),
}

#[component]
pub fn Actions() -> Element {
    let mut minimap = use_context::<AppState>().minimap;
    let mut minimap_preset = use_context::<AppState>().minimap_preset;
    let minimap_presets = use_memo(move || {
        minimap()
            .map(|minimap| minimap.actions.into_keys().collect::<Vec<String>>())
            .unwrap_or_default()
    });
    // Maps currently selected `minimap_preset` to the index in `minimap_presets`
    let minimap_preset_index = use_memo(move || {
        let presets = minimap_presets();
        minimap_preset().and_then(|preset| {
            presets
                .into_iter()
                .enumerate()
                .find(|(_, p)| &preset == p)
                .map(|(i, _)| i)
        })
    });

    // Handles async operations for action-related
    let coroutine = use_coroutine(move |mut rx: UnboundedReceiver<ActionUpdate>| async move {
        while let Some(message) = rx.next().await {
            match message {
                ActionUpdate::Set => {
                    if let Some(minimap) = minimap() {
                        update_minimap(minimap_preset(), minimap).await;
                    }
                }
                ActionUpdate::Create(name) => {
                    let Some(mut current_minimap) = minimap() else {
                        return;
                    };
                    if current_minimap.actions.try_insert(name, vec![]).is_err() {
                        return;
                    }

                    let mut save_minimap = current_minimap.clone();
                    spawn_blocking(move || {
                        upsert_map(&mut save_minimap).expect("failed to update minimap actions");
                    })
                    .await
                    .unwrap();

                    minimap.set(Some(current_minimap));
                }
                ActionUpdate::Delete(name) => {
                    let Some(mut current_minimap) = minimap() else {
                        return;
                    };
                    if current_minimap.actions.remove(&name).is_none() {
                        return;
                    }

                    let mut save_minimap = current_minimap.clone();
                    spawn_blocking(move || {
                        upsert_map(&mut save_minimap).expect("failed to delete minimap actions");
                    })
                    .await
                    .unwrap();

                    minimap.set(Some(current_minimap));
                }
            }
        }
    });
    let mut is_editing_action = use_signal(|| false);

    // Sets a preset if there is not one
    use_effect(move || {
        if let Some(minimap) = minimap()
            && !minimap.actions.is_empty()
            && minimap_preset.peek().is_none()
        {
            minimap_preset.set(minimap.actions.into_keys().next());
        } else {
            minimap_preset.set(None);
        }
        coroutine.send(ActionUpdate::Set);
    });

    rsx! {
        div { class: "flex flex-col pb-15 h-full overflow-y-auto scrollbar",
            Section { name: "Legends" }
            Section { name: "Normal actions",
                ActionList {
                    on_add_click: move |_| {
                        is_editing_action.set(true);
                    },
                }
            }
            Section { name: "Erda Shower off cooldown priority actions" }
            Section { name: "Every milliseconds priority actions" }
        }
        if is_editing_action() {
            div { class: "px-8 pt-8 pb-9 w-full h-full absolute inset-0 z-1 bg-gray-950/80",
                ActionKeyInput {
                    on_cancel: move |_| {
                        is_editing_action.set(false);
                    },
                    on_value: move |action| {},
                    value: ActionKey::default(),
                }
            }
        }
        div { class: "flex items-center w-full h-10 bg-gray-950 absolute bottom-0",
            TextSelect {
                class: "h-6 flex-grow",
                options: minimap_presets(),
                disabled: minimap().is_none(),
                placeholder: "Create a preset...",
                on_create: move |name| {
                    coroutine.send(ActionUpdate::Create(name));
                    coroutine.send(ActionUpdate::Set);
                },
                on_delete: move |index: usize| {
                    if let Some(name) = minimap_presets.peek().get(index) {
                        coroutine.send(ActionUpdate::Delete(name.clone()));
                    }
                },
                on_select: move |(_, preset)| {
                    minimap_preset.set(Some(preset));
                    coroutine.send(ActionUpdate::Set);
                },
                selected: minimap_preset_index(),
            }
        }
    }
}

#[component]
fn Section(name: String, children: Element) -> Element {
    rsx! {
        div { class: "flex flex-col gap-2",
            div { class: "flex items-center title-xs h-10", {name} }
            {children}
        }
    }
}

#[component]
fn ActionKeyInput(
    on_cancel: EventHandler,
    on_value: EventHandler<ActionKey>,
    value: ActionKey,
) -> Element {
    let action_name = match value.condition {
        backend::ActionCondition::Any => "normal",
        backend::ActionCondition::EveryMillis(_) => "every milliseconds",
        backend::ActionCondition::ErdaShowerOffCooldown => "Erda Shower off cooldown",
        backend::ActionCondition::Linked => "linked",
    };
    let mut action = use_signal(|| value);

    use_effect(use_reactive!(|value| { action.set(value) }));

    rsx! {
        div { class: "bg-gray-900 h-full px-2",
            Section { name: "Add a new {action_name} action",
                div { class: "grid grid-cols-3 gap-3",
                    // Position
                    ActionsNumberInputI32 {
                        label: "X",
                        disabled: action().position.is_none(),
                        on_value: move |x| {
                            let action = action.write();
                            action.position.unwrap().x = x;
                        },
                        value: action().position.map(|pos| pos.x).unwrap_or_default(),
                    }
                    ActionsNumberInputI32 {
                        label: "Y",
                        disabled: action().position.is_none(),
                        on_value: move |y| {
                            let action = action.write();
                            action.position.unwrap().x = y;
                        },
                        value: action().position.map(|pos| pos.y).unwrap_or_default(),
                    }
                    ActionsCheckbox {
                        label: "Has position",
                        on_value: move |has_position: bool| {
                            let mut action = action.write();
                            action.position = has_position.then_some(Position::default());
                        },
                        value: action().position.is_some(),
                    }

                    // Key, count and link key
                    ActionsKeyBindingInput {
                        label: "Key",
                        disabled: false,
                        on_value: move |key: Option<KeyBinding>| {
                            let mut action = action.write();
                            action.key = key.expect("not optional");
                        },
                        value: Some(action().key),
                    }
                    ActionsNumberInputU32 {
                        label: "Use count",
                        on_value: move |count| {
                            let mut action = action.write();
                            action.count = count;
                        },
                        value: action().count,
                    }
                    ActionsCheckbox {
                        label: "Is linked action",
                        on_value: move |is_linked: bool| {
                            let mut action = action.write();
                            action.condition = if is_linked {
                                ActionCondition::Linked
                            } else {
                                value.condition
                            };
                        },
                        value: matches!(action().condition, ActionCondition::Linked),
                    }
                    ActionsKeyBindingInput {
                        label: "Link key",
                        disabled: action().link_key.is_none(),
                        on_value: move |key: Option<KeyBinding>| {
                            let mut action = action.write();
                            action.link_key = action
                                .link_key
                                .map(|link_key| link_key.with_key(key.expect("not optional")));
                        },
                        value: action().link_key.unwrap_or_default().key(),
                    }
                    ActionsSelect::<LinkKeyBinding> {
                        label: "Link key type",
                        disabled: action().link_key.is_none(),
                        on_select: move |link_key: LinkKeyBinding| {
                            let mut action = action.write();
                            action.link_key = Some(
                                link_key.with_key(action.link_key.expect("has link key if selectable").key()),
                            );
                        },
                        selected: action().link_key.unwrap_or_default(),
                    }
                    ActionsCheckbox {
                        label: "Has link key",
                        on_value: move |has_link_key: bool| {
                            let mut action = action.write();
                            action.link_key = has_link_key.then_some(LinkKeyBinding::default());
                        },
                        value: action().link_key.is_some(),
                    }

                    // Use with, direction
                    ActionsSelect::<ActionKeyWith> {
                        label: "Use key with",
                        disabled: false,
                        on_select: move |with| {
                            let mut action = action.write();
                            action.with = with;
                        },
                        selected: action().with,
                    }
                    ActionsSelect::<ActionKeyDirection> {
                        label: "Use key direction",
                        disabled: false,
                        on_select: move |direction| {
                            let mut action = action.write();
                            action.direction = direction;
                        },
                        selected: action().direction,
                    }
                    div {} // Spacer

                    // Wait before use
                    ActionsMillisInput {
                        label: "Wait before",
                        on_value: move |millis| {
                            let mut action = action.write();
                            action.wait_before_use_millis = millis;
                        },
                        value: action().wait_before_use_millis,
                    }
                    ActionsMillisInput {
                        label: "Wait random range",
                        on_value: move |millis| {
                            let mut action = action.write();
                            action.wait_before_use_millis_random_range = millis;
                        },
                        value: action().wait_before_use_millis_random_range,
                    }
                    div {} // Spacer

                    // Wait after use
                    ActionsMillisInput {
                        label: "Wait after",
                        on_value: move |millis| {
                            let mut action = action.write();
                            action.wait_after_use_millis = millis;
                        },
                        value: action().wait_after_use_millis,
                    }
                    ActionsMillisInput {
                        label: "Wait random range",
                        on_value: move |millis| {
                            let mut action = action.write();
                            action.wait_after_use_millis_random_range = millis;
                        },
                        value: action().wait_after_use_millis_random_range,
                    }
                }
                div { class: "flex gap-3",
                    ActionsButton {
                        text: "Add",
                        on_click: move |_| {
                            on_value(*action.peek());
                        },
                    }
                    ActionsButton {
                        text: "Cancel",
                        on_click: move |_| {
                            on_cancel(());
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn ActionList(on_add_click: EventHandler) -> Element {
    rsx! {
        div { class: "flex flex-col gap-2",
            ActionKeyItem { action: ActionKey::default() }
            ActionKeyItem {
                action: ActionKey {
                    position: Some(Position {
                        x: 100000,
                        x_random_range: 10,
                        y: 10000,
                        allow_adjusting: true,
                    }),
                    queue_to_front: Some(true),
                    link_key: Some(LinkKeyBinding::Before(KeyBinding::B)),
                    direction: ActionKeyDirection::Left,
                    ..ActionKey::default()
                },
            }
            ActionKeyItem {
                action: ActionKey {
                    link_key: Some(LinkKeyBinding::After(KeyBinding::B)),
                    direction: ActionKeyDirection::Right,
                    with: ActionKeyWith::Stationary,
                    ..ActionKey::default()
                },
            }
            ActionKeyItem {
                action: ActionKey {
                    position: Some(Position {
                        x: 200,
                        y: 200,
                        allow_adjusting: true,
                        ..Position::default()
                    }),
                    link_key: Some(LinkKeyBinding::Along(KeyBinding::B)),
                    with: ActionKeyWith::DoubleJump,
                    ..ActionKey::default()
                },
            }
            ActionKeyItem {
                action: ActionKey {
                    link_key: Some(LinkKeyBinding::AtTheSame(KeyBinding::B)),
                    queue_to_front: Some(true),
                    ..ActionKey::default()
                },
            }
            button {
                class: "px-2 w-full h-5 label bg-gray-900 hover:bg-gray-800",
                onclick: move |e| {
                    e.stop_propagation();
                    on_add_click(());
                },
                "Add action"
            }
        }
    }
}

#[component]
fn ActionKeyItem(action: ActionKey) -> Element {
    const TEXT_CLASS: &str =
        "text-center inline-block text-ellipsis overflow-hidden whitespace-nowrap";
    const BORDER_CLASS: &str = "border-r-2 border-gray-900";

    let ActionKey {
        key,
        link_key,
        count,
        position,
        direction,
        with,
        queue_to_front,
        ..
    } = action;

    let position = if let Some(Position {
        x,
        y,
        x_random_range,
        allow_adjusting,
    }) = position
    {
        let x_min = (x - x_random_range).max(0);
        let x_max = (x + x_random_range).max(0);
        let x = if x_min == x_max {
            format!("{x}")
        } else {
            format!("{x_min}~{x_max}")
        };
        let allow_adjusting = if allow_adjusting { " / Adjust" } else { "" };

        format!("{x}, {y}{allow_adjusting}")
    } else {
        "ㄨ".to_string()
    };
    let queue_to_front = if queue_to_front.unwrap_or_default() {
        "⇈ / "
    } else {
        ""
    };
    let link_key = match link_key {
        Some(LinkKeyBinding::Before(key)) => format!("{key} ↝ "),
        Some(LinkKeyBinding::After(key)) => format!("{key} ↜ "),
        Some(LinkKeyBinding::AtTheSame(key)) => format!("{key} ↭ "),
        Some(LinkKeyBinding::Along(key)) => format!("{key} ↷ "),
        None => "".to_string(),
    };

    rsx! {
        div { class: "grid grid-cols-[160px_70px_30px_auto] h-4 label hover:bg-gray-800",
            div { class: "{BORDER_CLASS} {TEXT_CLASS}", "{queue_to_front}{position}" }
            div { class: "{BORDER_CLASS} {TEXT_CLASS}", "{link_key}{key} × {count}" }
            div { class: "{BORDER_CLASS} {TEXT_CLASS}",
                match direction {
                    ActionKeyDirection::Any => "⇆",
                    ActionKeyDirection::Left => "←",
                    ActionKeyDirection::Right => "→",
                }
            }
            div { class: "flex justify-center",
                match with {
                    ActionKeyWith::Any => "Any",
                    ActionKeyWith::Stationary => "Stationary",
                    ActionKeyWith::DoubleJump => "Double jump",
                }
            }
        }
    }
}

#[component]
fn ActionsSelect<T: 'static + Clone + PartialEq + Display + IntoEnumIterator>(
    label: &'static str,
    disabled: bool,
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
            disabled,
            on_select,
            selected,
        }
    }
}

#[component]
fn ActionsNumberInputI32(
    label: &'static str,
    #[props(default = false)] disabled: bool,
    on_value: EventHandler<i32>,
    value: i32,
) -> Element {
    rsx! {
        NumberInputI32 {
            label,
            label_class: INPUT_LABEL_CLASS,
            div_class: INPUT_DIV_CLASS,
            input_class: INPUT_CLASS,
            disabled,
            on_value,
            value,
        }
    }
}

#[component]
fn ActionsNumberInputU32(
    label: &'static str,
    #[props(default = false)] disabled: bool,
    on_value: EventHandler<u32>,
    value: u32,
) -> Element {
    rsx! {
        NumberInputU32 {
            label,
            label_class: INPUT_LABEL_CLASS,
            div_class: INPUT_DIV_CLASS,
            input_class: INPUT_CLASS,
            minimum_value: 1,
            disabled,
            on_value,
            value,
        }
    }
}

#[component]
fn ActionsMillisInput(label: &'static str, on_value: EventHandler<u64>, value: u64) -> Element {
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

#[component]
fn ActionsCheckbox(label: &'static str, on_value: EventHandler<bool>, value: bool) -> Element {
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
fn ActionsKeyBindingInput(
    label: &'static str,
    disabled: bool,
    on_value: EventHandler<Option<KeyBinding>>,
    value: Option<KeyBinding>,
) -> Element {
    rsx! {
        KeyBindingInput {
            label,
            label_class: INPUT_LABEL_CLASS,
            div_class: INPUT_DIV_CLASS,
            input_class: KEY_INPUT_CLASS,
            disabled,
            optional: false,
            on_value: move |value: Option<KeyBinding>| {
                on_value(value);
            },
            value,
        }
    }
}

#[component]
fn ActionsButton(text: String, on_click: EventHandler) -> Element {
    rsx! {
        button {
            class: "px-2 h-6 paragraph-xs button-primary border border-gray-600",
            onclick: move |e| {
                e.stop_propagation();
                on_click(());
            },
            {text}
        }
    }
}
