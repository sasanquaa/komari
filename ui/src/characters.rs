use dioxus::prelude::*;

use crate::key::KeyInput;

#[component]
pub fn Characters() -> Element {
    let active_key_label = use_signal(|| None);

    rsx! {
        div { class: "flex flex-col",
            Section { name: "Key Bindings",
                div { class: "grid grid-cols-2 gap-3",
                    KeyBinding { label: "Rope Lift", active_key_label }
                    KeyBinding { label: "Teleport", active_key_label }
                    KeyBinding { label: "Jump", active_key_label }
                    KeyBinding { label: "Up Jump", active_key_label }
                    KeyBinding { label: "Interact", active_key_label }
                    KeyBinding { label: "Cash Shop", active_key_label }
                    KeyBinding { label: "Familiar Menu", active_key_label }
                    KeyBinding { label: "Maple Guide", active_key_label }
                    KeyBinding { label: "Change Channel", active_key_label }
                    KeyBinding { label: "Potion", active_key_label }
                }
            }
            Section { name: "Buffs" }
            Section { name: "Fixed Actions" }
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
fn KeyBinding(label: &'static str, active_key_label: Signal<Option<&'static str>>) -> Element {
    let is_active = use_memo(move || active_key_label() == Some(label));

    rsx! {
        div { class: "flex flex-col gap-1",
            label { class: "text-[11px] text-gray-500", {label} }
            KeyInput {
                class: "h-6 bg-gray-100",
                disabled: false,
                is_active: is_active(),
                on_active: move |is_active| {
                    if is_active {
                        active_key_label.set(Some(label));
                    } else {
                        active_key_label.set(None)
                    }
                },
                on_input: |key| {},
                value: None,
            }
        }
    }
}
