use std::time::Duration;

use dioxus::prelude::*;
use tw_merge::tw_merge;

use crate::components::{use_controlled, use_unique_id};

const CLASS: &str = "h-6 text-xs text-primary-text outline-none px-1 border border-primary-border disabled:text-tertiary-text disabled:cursor-not-allowed w-full";

#[derive(Clone, Copy, PartialEq, Default)]
pub enum DurationParts {
    #[default]
    Full,
    MinutesAndSeconds,
}

impl DurationParts {
    fn parse_to_millis(&self, input: String) -> Option<u64> {
        let parts_count = match self {
            DurationParts::Full => 3,
            DurationParts::MinutesAndSeconds => 2,
        };
        let parts: Vec<_> = input.split(':').collect();
        if parts.len() != parts_count {
            return None;
        }

        match self {
            DurationParts::Full => {
                let hours: u64 = parts[0].parse().ok()?;
                let minutes: u64 = parts[1].parse().ok()?;
                let seconds: u64 = parts[2].parse().ok()?;
                if minutes >= 60 || seconds >= 60 {
                    return None;
                }

                let total_seconds = hours
                    .saturating_mul(3600)
                    .saturating_add(minutes.saturating_mul(60))
                    .saturating_add(seconds);

                Some(total_seconds.saturating_mul(1000))
            }
            DurationParts::MinutesAndSeconds => {
                let minutes: u64 = parts[0].parse().ok()?;
                let seconds: u64 = parts[1].parse().ok()?;
                if seconds >= 60 {
                    return None;
                }

                let total_seconds = minutes.saturating_mul(60).saturating_add(seconds);

                Some(total_seconds.saturating_mul(1000))
            }
        }
    }

    fn format_from_millis(&self, ms: u64) -> String {
        let duration = Duration::from_millis(ms);

        match self {
            DurationParts::Full => {
                let hours = duration.as_secs() / 3600;
                let minutes = (duration.as_secs() % 3600) / 60;
                let seconds = duration.as_secs() % 60;

                format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
            }
            DurationParts::MinutesAndSeconds => {
                let minutes = duration.as_secs() / 60;
                let seconds = duration.as_secs() % 60;

                format!("{:02}:{:02}", minutes, seconds)
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct DurationInputProps {
    value: ReadSignal<Option<u64>>,
    #[props(default)]
    on_value: Callback<u64>,
    #[props(default)]
    disabled: ReadSignal<bool>,
    #[props(default)]
    parts: DurationParts,
    #[props(default)]
    class: String,
}

#[component]
pub fn DurationInput(props: DurationInputProps) -> Element {
    let id = use_unique_id();

    let disabled = props.disabled;
    let class = props.class;
    let components = props.parts;

    let (value, set_value) = use_controlled(props.value, 0, props.on_value);
    let mut text = use_signal(|| components.format_from_millis(value()));

    use_effect(move || {
        if let Some(ms) = (props.value)() {
            text.set(components.format_from_millis(ms));
        }
    });

    let on_input = move |e: Event<FormData>| {
        text.set(e.value());
    };

    let on_blur = move |_| {
        if let Some(parsed) = components.parse_to_millis(text()) {
            set_value(parsed);
            text.set(components.format_from_millis(parsed));
        } else {
            text.set(components.format_from_millis(value()));
        }
    };

    rsx! {
        input {
            id: id(),
            class: tw_merge!(CLASS, class),
            disabled,
            value: "{text}",
            oninput: on_input,
            onblur: on_blur,
            placeholder: match components {
                DurationParts::Full => "hh:mm:ss",
                DurationParts::MinutesAndSeconds => "mm:ss",
            },
        }
    }
}
