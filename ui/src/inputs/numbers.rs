use std::{fmt::Display, str::FromStr};

use dioxus::{document::EvalError, prelude::*};
use num_traits::PrimInt;
use rand::distr::{Alphanumeric, SampleString};

use super::GenericInputProps;
use crate::inputs::LabeledInput;

pub fn use_auto_numeric(
    id: Memo<String>,
    value: String,
    on_value: Option<EventHandler<String>>,
    minimum_value: String,
    maximum_value: String,
    suffix: String,
) {
    let has_input = on_value.is_some();
    let value = use_memo(use_reactive!(|value| value));
    let minimum_value = use_memo(move || minimum_value.clone());
    let maximum_value = use_memo(move || maximum_value.clone());
    let suffix = use_memo(move || suffix.clone());

    use_effect(move || {
        let value = value();
        spawn(async move {
            let js = format!(
                r#"
                const hasInput = {has_input};
                const element = document.getElementById("{id}");
                let autoNumeric = AutoNumeric.getAutoNumericElement(element);
                if (autoNumeric === null) {{
                    autoNumeric = new AutoNumeric(element, {value}, {{
                        allowDecimalPadding: false,
                        emptyInputBehavior: "{minimum_value}",
                        maximumValue: "{maximum_value}",
                        minimumValue: "{minimum_value}",
                        suffixText: "{suffix}"
                    }});
                }} else {{
                    autoNumeric.set({value});
                }}
                if (hasInput) {{
                    element.addEventListener("autoNumeric:rawValueModified", async (e) => {{
                        await dioxus.send(e.detail.newRawValue);
                    }}, {{ once: true }});
                }}
            "#
            );
            let mut eval = document::eval(js.as_str());
            if let Some(on_value) = on_value {
                loop {
                    let value = eval.recv::<String>().await;
                    if let Err(EvalError::Finished) = value {
                        return;
                    };
                    on_value(value.unwrap());
                }
            }
        });
    });
}

#[component]
pub fn MillisInput(
    GenericInputProps {
        label,
        label_class,
        div_class,
        input_class,
        disabled,
        on_value,
        value,
    }: GenericInputProps<u64>,
) -> Element {
    rsx! {
        PrimIntInput {
            label,
            label_class,
            div_class,
            input_class,
            disabled,
            on_value,
            value,
            suffix: "ms",
        }
    }
}

// FIXME: :smiling-doge:
#[component]
pub fn PercentageInput(
    GenericInputProps {
        label,
        label_class,
        div_class,
        input_class,
        disabled,
        on_value,
        value,
    }: GenericInputProps<f32>,
) -> Element {
    let input_id = use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));
    use_auto_numeric(
        input_id,
        value.to_string(),
        Some(EventHandler::new(move |value: String| {
            if let Ok(value) = value.parse::<f32>() {
                on_value(value)
            }
        })),
        "0".to_string(),
        "100".to_string(),
        "%".to_string(),
    );

    rsx! {
        LabeledInput {
            label,
            label_class,
            div_class,
            disabled,
            input { id: input_id(), disabled, class: input_class }
        }
    }
}

// TODO: Please https://github.com/DioxusLabs/dioxus/issues/3938
#[component]
pub fn NumberInputU32(
    label: String,
    #[props(default = String::default())] label_class: String,
    #[props(default = String::default())] div_class: String,
    #[props(default = String::default())] input_class: String,
    #[props(default = false)] disabled: bool,
    minimum_value: u32,
    on_value: EventHandler<u32>,
    value: u32,
) -> Element {
    rsx! {
        PrimIntInput {
            label,
            label_class,
            div_class,
            input_class,
            minimum_value,
            disabled,
            on_value,
            value,
        }
    }
}

#[component]
pub fn NumberInputI32(
    GenericInputProps {
        label,
        label_class,
        div_class,
        input_class,
        disabled,
        on_value,
        value,
    }: GenericInputProps<i32>,
) -> Element {
    rsx! {
        PrimIntInput {
            label,
            label_class,
            div_class,
            input_class,
            minimum_value: 0,
            disabled,
            on_value,
            value,
        }
    }
}

#[component]
fn PrimIntInput<T: 'static + IntoAttributeValue + PrimInt + FromStr + Display>(
    label: String,
    #[props(default = String::default())] label_class: String,
    #[props(default = String::default())] div_class: String,
    #[props(default = String::default())] input_class: String,
    #[props(default = None)] maximum_value: Option<T>,
    #[props(default = T::min_value())] minimum_value: T,
    #[props(default = String::default())] suffix: String,
    #[props(default = false)] disabled: bool,
    on_value: EventHandler<T>,
    value: T,
) -> Element {
    let input_id = use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));
    use_auto_numeric(
        input_id,
        value.to_string(),
        Some(EventHandler::new(move |value: String| {
            if let Ok(value) = value.parse::<T>() {
                on_value(value)
            }
        })),
        minimum_value.to_string(),
        maximum_value.unwrap_or(T::max_value()).to_string(),
        suffix,
    );

    rsx! {
        LabeledInput {
            label,
            label_class,
            div_class,
            disabled,
            input { id: input_id(), disabled, class: input_class }
        }
    }
}
