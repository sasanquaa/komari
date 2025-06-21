use dioxus::prelude::*;

mod keys;
mod numbers;

pub use {keys::*, numbers::*};

#[derive(Clone, PartialEq, Props)]
pub struct GenericInputProps<T: 'static + Clone + PartialEq> {
    label: String,
    #[props(default = String::default())]
    label_class: String,
    #[props(default = String::default())]
    div_class: String,
    #[props(default = String::default())]
    input_class: String,
    #[props(default = false)]
    disabled: bool,
    on_value: EventHandler<T>,
    value: T,
}

#[component]
pub fn Checkbox(
    GenericInputProps {
        label,
        label_class,
        div_class,
        input_class,
        disabled,
        on_value,
        value,
    }: GenericInputProps<bool>,
) -> Element {
    rsx! {
        LabeledInput {
            label,
            label_class,
            div_class,
            disabled,
            div { class: input_class,
                input {
                    class: "appearance-none w-full h-full",
                    disabled,
                    r#type: "checkbox",
                    oninput: move |e| {
                        on_value(e.parsed::<bool>().unwrap());
                    },
                    checked: value,
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Props)]
pub struct LabeledInputProps {
    label: String,
    label_class: String,
    div_class: String,
    disabled: bool,
    children: Element,
}

#[component]
pub fn LabeledInput(props: LabeledInputProps) -> Element {
    let data_disabled = props.disabled.then_some(true);

    rsx! {
        div { class: props.div_class, "data-disabled": data_disabled,
            label { class: props.label_class, "data-disabled": data_disabled, {props.label} }
            {props.children}
        }
    }
}
