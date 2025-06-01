use backend::{FamiliarRarity, Familiars as FamiliarsData, Settings, SwappableFamiliars};
use dioxus::prelude::*;

use crate::{
    AppMessage,
    input::MillisInput,
    settings::{SettingsCheckbox, SettingsEnumSelect},
};

#[component]
pub fn Familiars(
    app_coroutine: Coroutine<AppMessage>,
    settings: ReadOnlySignal<Option<Settings>>,
) -> Element {
    let settings_view = use_memo(move || settings().unwrap_or_default());
    let familiars_view = use_memo(move || settings_view().familiars);
    let on_familiars = move |updated| {
        app_coroutine.send(AppMessage::UpdateSettings(Settings {
            familiars: updated,
            ..settings_view.peek().clone()
        }));
    };

    rsx! {
        div { class: "px-2 pb-2 pt-2 flex flex-col space-y-3 overflow-y-auto scrollbar h-full",
            SettingsCheckbox {
                label: "Enable Familiars Swapping",
                on_input: move |enable_familiars_swapping| {
                    on_familiars(FamiliarsData {
                        enable_familiars_swapping,
                        ..familiars_view.peek().clone()
                    });
                },
                value: familiars_view().enable_familiars_swapping,
            }
            MillisInput {
                label: "Swap Check Every Milliseconds",
                div_class: "flex items-center space-x-4",
                label_class: "text-xs text-gray-700 flex-1 inline-block data-[disabled]:text-gray-400",
                input_class: "w-44 h-7 text-xs text-gray-700 text-ellipsis border border-gray-300 rounded outline-none disabled:cursor-not-allowed disabled:text-gray-400",
                disabled: false,
                on_input: move |swap_check_millis| {
                    on_familiars(FamiliarsData {
                        swap_check_millis,
                        ..familiars_view.peek().clone()
                    });
                },
                value: familiars_view().swap_check_millis,
            }
            SettingsEnumSelect::<SwappableFamiliars> {
                label: "Swappable Slots",
                on_select: move |swappable_familiars| {
                    on_familiars(FamiliarsData {
                        swappable_familiars,
                        ..familiars_view.peek().clone()
                    });
                },
                disabled: false,
                selected: familiars_view().swappable_familiars,
            }
            SettingsCheckbox {
                label: "Allow Swapping Rare Familiar",
                on_input: move |enabled| {
                    let mut familiars = familiars_view.peek().clone();
                    if enabled {
                        familiars.swappable_rarities.insert(FamiliarRarity::Rare);
                    } else {
                        familiars.swappable_rarities.remove(&FamiliarRarity::Rare);
                    }
                    on_familiars(familiars);
                },
                value: familiars_view().swappable_rarities.contains(&FamiliarRarity::Rare),
            }
            SettingsCheckbox {
                label: "Allow Swapping Epic Familiar",
                on_input: move |enabled| {
                    let mut familiars = familiars_view.peek().clone();
                    if enabled {
                        familiars.swappable_rarities.insert(FamiliarRarity::Epic);
                    } else {
                        familiars.swappable_rarities.remove(&FamiliarRarity::Epic);
                    }
                    on_familiars(familiars);
                },
                value: familiars_view().swappable_rarities.contains(&FamiliarRarity::Epic),
            }
        }
    }
}
