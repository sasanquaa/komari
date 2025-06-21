use backend::{
    ActionKey, ActionKeyDirection, ActionKeyWith, KeyBinding, LinkKeyBinding, Position, upsert_map,
};
use dioxus::prelude::*;
use futures_util::StreamExt;
use tokio::task::spawn_blocking;

use crate::{AppState, select::TextSelect};

#[derive(Debug)]
enum ActionUpdate {
    Set,
    Create(String),
    Delete,
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
                ActionUpdate::Set => {}
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
                ActionUpdate::Delete => todo!(),
            }
        }
    });

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
            Section { name: "Normal actions", ActionList {} }
            Section { name: "Erda Shower off cooldown priority actions", ActionList {} }
            Section { name: "Every milliseconds priority actions", ActionList {} }
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
                on_delete: move |_| {
                    coroutine.send(ActionUpdate::Delete);
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
fn Section(name: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "flex flex-col pr-4 pb-3 gap-2",
            div { class: "flex items-center title-xs h-10", {name} }
            {children}
        }
    }
}

#[component]
fn ActionList() -> Element {
    rsx! {
        div { class: "flex flex-col gap-2",
            ActionKeyItem { action: ActionKey::default() }
            ActionKeyItem {
                action: ActionKey {
                    position: Some(Position {
                        x: 100,
                        x_random_range: 10,
                        y: 100,
                        ..Position::default()
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
                },
                "Add action"
            }
        }
    }
}

#[component]
fn ActionKeyItem(action: ActionKey) -> Element {
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
        div { class: "grid grid-cols-7 h-4 label hover:bg-gray-800",
            div { class: "border-r-2 border-gray-900 flex justify-center col-span-2",
                "{queue_to_front}{position}"
            }
            div { class: "border-r-2 border-gray-900 flex justify-center col-span-2",
                "{link_key}{key} × {count}"
            }
            div { class: "border-r-2 border-gray-900 flex justify-center col-span-1",
                match direction {
                    ActionKeyDirection::Any => "⇆",
                    ActionKeyDirection::Left => "←",
                    ActionKeyDirection::Right => "→",
                }
            }
            div { class: "flex justify-center col-span-2",
                match with {
                    ActionKeyWith::Any => "Any",
                    ActionKeyWith::Stationary => "Stationary",
                    ActionKeyWith::DoubleJump => "Double Jump",
                }
            }
        }
    }
}
