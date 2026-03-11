use backend::{Bound, Map, MobbingKey, RotationMode};
use dioxus::prelude::*;

use crate::{
    actions::{
        ActionsCheckbox, ActionsContext, ActionsMillisInput, ActionsSelect,
        popup::{PopupMobbingBoundInputContent, PopupMobbingKeyInputContent},
    },
    components::{
        button::{Button, ButtonStyle},
        popup::{PopupContext, PopupTrigger},
        section::Section,
    },
};

#[derive(Clone, Copy, PartialEq)]
enum PopupContent {
    None,
    Bound(Bound),
    Key(MobbingKey),
}

#[component]
pub fn SectionRotation(disabled: bool) -> Element {
    let context = use_context::<ActionsContext>();
    let map = context.map;
    let save_map = context.save_map;

    let update_mobbing_key_disabled = use_memo(move || {
        let mode = map().rotation_mode;
        !matches!(mode, RotationMode::AutoMobbing | RotationMode::PingPong)
    });

    let edit_mobbing_key = move |rotation_mobbing_key| {
        save_map(Map {
            rotation_mobbing_key,
            ..map()
        });
    };

    let edit_mobbing_bound = move |bound| {
        let mut map = map();

        match map.rotation_mode {
            RotationMode::StartToEnd | RotationMode::StartToEndThenReverse => return,
            RotationMode::AutoMobbing => {
                map.rotation_auto_mob_bound = bound;
            }
            RotationMode::PingPong => {
                map.rotation_ping_pong_bound = bound;
            }
        }
        save_map(map);
    };

    let mut popup_content = use_signal(|| PopupContent::None);
    let mut popup_open = use_signal(|| false);

    let handle_mobbing_key_click = move || {
        let map = map.peek();
        let key = match map.rotation_mode {
            RotationMode::StartToEnd | RotationMode::StartToEndThenReverse => {
                unreachable!()
            }
            RotationMode::AutoMobbing | RotationMode::PingPong => map.rotation_mobbing_key,
        };
        popup_content.set(PopupContent::Key(key));
    };

    let handle_mobbing_bound_click = move || {
        let map = map.peek();
        let bound = match map.rotation_mode {
            RotationMode::StartToEnd | RotationMode::StartToEndThenReverse => {
                unreachable!()
            }
            RotationMode::AutoMobbing => map.rotation_auto_mob_bound,
            RotationMode::PingPong => map.rotation_ping_pong_bound,
        };
        popup_content.set(PopupContent::Bound(bound));
    };

    use_effect(move || {
        if !popup_open() {
            popup_content.set(PopupContent::None);
        }
    });

    rsx! {
        PopupContext {
            open: popup_open,
            on_open: move |open: bool| {
                popup_open.set(open);
            },
            Section { title: "Rotation",
                div { class: "grid grid-cols-2 gap-3",
                    ActionsSelect::<RotationMode> {
                        label: "Mode",
                        disabled,
                        on_selected: move |rotation_mode| {
                            save_map(Map {
                                rotation_mode,
                                ..map.peek().clone()
                            })
                        },
                        selected: map().rotation_mode,
                    }

                    div {}

                    PopupTrigger {
                        Button {
                            style: ButtonStyle::Primary,
                            class: "w-full",
                            disabled: disabled | update_mobbing_key_disabled(),
                            on_click: handle_mobbing_key_click,

                            "Update mobbing key"
                        }
                    }

                    PopupTrigger {
                        Button {
                            style: ButtonStyle::Primary,
                            class: "w-full",
                            disabled: disabled || update_mobbing_key_disabled(),
                            on_click: handle_mobbing_bound_click,

                            "Update mobbing bound"
                        }
                    }

                    ActionsCheckbox {
                        label: "Auto mobbing uses key when pathing",
                        tooltip: "Pathing means when the player is moving from one quad to another.",
                        disabled,
                        on_checked: move |auto_mob_use_key_when_pathing| {
                            save_map(Map {
                                auto_mob_use_key_when_pathing,
                                ..map.peek().clone()
                            })
                        },
                        checked: map().auto_mob_use_key_when_pathing,
                    }

                    ActionsMillisInput {
                        label: "Detect mobs when pathing every",
                        disabled,
                        on_value: move |auto_mob_use_key_when_pathing_update_millis| {
                            save_map(Map {
                                auto_mob_use_key_when_pathing_update_millis,
                                ..map.peek().clone()
                            })
                        },
                        value: map().auto_mob_use_key_when_pathing_update_millis,
                    }

                    ActionsCheckbox {
                        label: "Reset normal actions on Erda Shower resets",
                        disabled,
                        on_checked: move |actions_any_reset_on_erda_condition| {
                            save_map(Map {
                                actions_any_reset_on_erda_condition,
                                ..map.peek().clone()
                            })
                        },
                        checked: map().actions_any_reset_on_erda_condition,
                    }
                }
            }

            match popup_content() {
                #[allow(clippy::double_parens)]
                PopupContent::None => rsx! {},
                PopupContent::Bound(bound) => rsx! {
                    PopupMobbingBoundInputContent {
                        on_cancel: move |_| {
                            popup_open.set(false);
                        },
                        on_value: move |bound| {
                            edit_mobbing_bound(bound);
                            popup_open.set(false);
                        },
                        value: bound,
                    }
                },
                PopupContent::Key(key) => rsx! {
                    PopupMobbingKeyInputContent {
                        on_cancel: move |_| {
                            popup_open.set(false);
                        },
                        on_value: move |key| {
                            edit_mobbing_key(key);
                            popup_open.set(false);
                        },
                        value: key,
                    }
                },
            }
        }
    }
}
