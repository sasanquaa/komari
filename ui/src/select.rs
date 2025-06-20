use std::{fmt::Display, str::FromStr};

use backend::IntoEnumIterator;
use dioxus::prelude::*;

use crate::inputs::LabeledInput;

#[derive(PartialEq, Props, Clone)]
pub struct SelectProps<T: 'static + Clone + PartialEq + Display> {
    #[props(default = String::default())]
    label: String,
    #[props(default = String::from("collapse"))]
    label_class: String,
    #[props(default = String::default())]
    div_class: String,
    #[props(default = String::default())]
    select_class: String,
    #[props(default = false)]
    disabled: bool,
    options: Vec<T>,
    on_select: EventHandler<(usize, T)>,
    selected: usize,
}

// #[component]
// pub fn EnumSelect<T: 'static + Clone + PartialEq + Display + FromStr + IntoEnumIterator>(
//     #[props(default = String::default())] label: String,
//     #[props(default = String::from("collapse"))] label_class: String,
//     #[props(default = String::default())] div_class: String,
//     #[props(default = String::default())] select_class: String,
//     #[props(default = false)] disabled: bool,
//     on_select: EventHandler<T>,
//     selected: T,
//     #[props(default = Vec::new())] excludes: Vec<T>,
// ) -> Element {
//     let options = T::iter()
//         .filter(|variant| !excludes.contains(variant))
//         .map(|variant| (variant.to_string(), variant.to_string()))
//         .collect::<Vec<_>>();
//     let selected = selected.to_string();

//     rsx! {
//         Select {
//             label,
//             disabled,
//             div_class,
//             label_class,
//             select_class,
//             options,
//             on_select: move |(_, variant): (usize, String)| {
//                 if let Ok(value) = T::from_str(variant.as_str()) {
//                     on_select(value);
//                 }
//             },
//             selected,
//         }
//     }
// }

#[component]
pub fn TextSelect(
    class: String,
    options: Vec<String>,
    disabled: bool,
    on_create: EventHandler<String>,
    on_delete: EventHandler<usize>,
    on_select: EventHandler<(usize, String)>,
    selected: Option<usize>,
) -> Element {
    let mut creating_text = use_signal::<Option<String>>(|| None);
    let mut creating_error = use_signal(|| false);
    let reset_creating = use_callback(move |_| {
        creating_text.set(None);
        creating_error.set(false);
    });

    use_effect(use_reactive!(|selected| {
        if selected.is_none() {
            reset_creating(());
        }
    }));
    use_effect(use_reactive!(|disabled| {
        if disabled {
            reset_creating(());
        }
    }));

    rsx! {
        div { class: "flex gap-1 {class}",
            div { class: "flex-grow",
                if let Some(text) = creating_text() {
                    input {
                        class: "rounded w-full h-full px-1 border border-gray-300 paragraph-xs",
                        placeholder: "Enter a name...",
                        onchange: move |e| {
                            creating_text.set(Some(e.value()));
                        },
                        value: text,
                    }
                } else {
                    Select {
                        div_class: "h-full",
                        select_class: "w-full h-full border border-gray-600 paragraph-xs outline-none",
                        disabled,
                        options,
                        on_select: move |(usize, text)| {
                            on_select((usize, text));
                        },
                        selected: selected.unwrap_or_default(),
                    }
                }
            }
            button {
                class: "button-primary w-20",
                onclick: move |_| {
                    let text = creating_text.peek().clone();
                    if let Some(text) = text {
                        if text.is_empty() {
                            creating_error.set(true);
                            return;
                        }
                        reset_creating(());
                        on_create(text);
                    } else {
                        creating_text.set(Some("".to_string()));
                    }
                },
                if creating_text().is_some() {
                    "Save"
                } else {
                    "Create"
                }
            }
            button {
                class: "button-danger w-20",
                onclick: move |_| {
                    if creating_text.peek().is_some() {
                        reset_creating(());
                    } else if let Some(index) = selected {
                        on_delete(index);
                    }
                },
                if creating_text().is_some() {
                    "Cancel"
                } else {
                    "Delete"
                }
            }
        }
    }
}

#[component]
pub fn Select<T>(
    SelectProps {
        label,
        div_class,
        label_class,
        select_class,
        options,
        disabled,
        on_select,
        selected,
    }: SelectProps<T>,
) -> Element
where
    T: 'static + Clone + PartialEq + Display,
{
    rsx! {
        LabeledInput {
            label,
            label_class,
            div_class,
            disabled,
            select {
                class: select_class,
                disabled,
                onchange: move |e| {
                    let i = e.value().parse::<usize>().unwrap();
                    let value = options[i].clone();
                    on_select((i, value))
                },
                for (i , option) in options.iter().enumerate() {
                    option {
                        disabled,
                        selected: i == selected,
                        value: i.to_string(),
                        label: option.to_string(),
                    }
                }
            }
        }
    }
}
