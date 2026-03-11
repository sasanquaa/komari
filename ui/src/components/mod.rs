//! Copied from [`DioxusLabs/components`].
//!
//! [`DioxusLabs/components`]: https://github.com/DioxusLabs/components/blob/3571d90aa55773c59b05119d9fff8879da6b8a3d/primitives/src/lib.rs

use std::{
    fmt::Display,
    str::FromStr,
    sync::atomic::{AtomicUsize, Ordering},
};

use dioxus::{document::EvalError, prelude::*};

pub mod button;
pub mod checkbox;
pub mod duration;
pub mod file;
pub mod icons;
pub mod key;
pub mod labeled;
pub mod list;
pub mod named_select;
pub mod numbers;
pub mod popup;
pub mod position;
pub mod section;
pub mod select;
pub mod text;
pub mod tooltip;

fn use_unique_id() -> Memo<String> {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    use_memo(|| {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let id_str = format!("id-{id}");
        id_str
    })
}

fn use_user_id_or_unique(user_id: ReadSignal<Option<String>>) -> Memo<String> {
    let unique_id = use_unique_id();

    use_memo(move || {
        if let Some(id) = user_id() {
            id
        } else {
            unique_id()
        }
    })
}

fn use_controlled<T: Clone + PartialEq + 'static>(
    current_value: ReadSignal<Option<T>>,
    default_value: T,
    on_value: Callback<T>,
) -> (Memo<T>, Callback<T>) {
    let mut inner_value = use_signal(|| current_value.cloned().unwrap_or(default_value));
    let value = use_memo(move || current_value.cloned().unwrap_or_else(&*inner_value));

    let set_value = use_callback(move |x: T| {
        inner_value.set(x.clone());
        on_value.call(x);
    });

    (value, set_value)
}

fn use_controlled_required<T: Clone + PartialEq + 'static>(
    current_value: ReadSignal<T>,
    on_value: Callback<T>,
) -> (ReadSignal<T>, Callback<T>) {
    let mut inner_value = use_signal(|| current_value.cloned());

    let set_value = use_callback(move |x: T| {
        inner_value.set(x.clone());
        on_value.call(x);
    });

    use_effect(move || {
        inner_value.set(current_value.cloned());
    });

    (inner_value.into(), set_value)
}

fn use_auto_numeric<T>(
    id: Memo<String>,
    initial_value: Memo<T>,
    on_value: Callback<T>,
    min_value: T,
    max_value: T,
    suffix: String,
) -> Callback<T>
where
    T: PartialEq + Clone + Display + FromStr + 'static,
{
    use_effect(move || {
        let initial_value = initial_value.peek().clone();
        let script = format!(
            r#"
            async function onRawValueModified(autoNumeric) {{
                await dioxus.send(autoNumeric.rawValue);
            }}

            const element = document.getElementById("{id}");
            let autoNumeric = AutoNumeric.getAutoNumericElement(element);
            if (autoNumeric === null) {{
                autoNumeric = new AutoNumeric(element, {initial_value}, {{
                    allowDecimalPadding: false,
                    emptyInputBehavior: "{min_value}",
                    maximumValue: "{max_value}",
                    minimumValue: "{min_value}",
                    suffixText: "{suffix}"
                }});
                element.addEventListener("input", () => onRawValueModified(autoNumeric));
            }}
            "#
        );
        let mut eval = document::eval(script.as_str());

        spawn(async move {
            loop {
                let result = eval.recv::<String>().await;
                match result {
                    Ok(str) => {
                        if let Ok(parsed) = str.parse::<T>() {
                            on_value(parsed);
                        }
                    }
                    Err(EvalError::Finished) => {
                        eval = document::eval(script.as_str());
                    }
                    Err(_) => break,
                }
            }
        });
    });

    use_callback(move |value: T| {
        let script = format!(
            r#"
            const element = document.getElementById("{id}");
            const autoNumeric = AutoNumeric.getAutoNumericElement(element);
            if (autoNumeric !== null) {{
                autoNumeric.set({value});
            }}
            "#
        );
        document::eval(script.as_str());
    })
}

/// The side where the content will be displayed relative to the trigger
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum ContentSide {
    /// The content will appear above the trigger
    Top,
    /// The content will appear to the right of the trigger
    Right,
    /// The content will appear below the trigger
    Bottom,
    /// The content will appear to the left of the trigger
    Left,
}

/// The alignment of the content relative to the trigger
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum ContentAlign {
    /// The content will be aligned to the start of the trigger
    Start,
    /// The content will be centered relative to the trigger
    Center,
    /// The content will be aligned to the end of the trigger
    End,
}
