#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![feature(variant_count)]
#![feature(map_try_insert)]

use std::{
    env::current_exe,
    io::stdout,
    string::ToString,
    sync::{Arc, LazyLock},
};

use backend::{
    Configuration, Minimap as MinimapData, Settings as SettingsData, query_configs, query_settings,
    update_configuration, update_settings, upsert_config, upsert_settings,
};
use characters::Characters;
use dioxus::{
    desktop::{
        WindowBuilder,
        tao::{platform::windows::WindowBuilderExtWindows, window::WindowSizeConstraints},
        wry::dpi::{PhysicalSize, PixelUnit, Size},
    },
    prelude::*,
};
use fern::Dispatch;
use futures_util::StreamExt;
use log::LevelFilter;
use minimap::Minimap;
use rand::distr::{Alphanumeric, SampleString};
use tokio::{
    sync::{
        Mutex,
        mpsc::{self},
    },
    task::spawn_blocking,
};

mod actions;
mod characters;
mod icons;
mod inputs;
mod minimap;
mod select;

const TAILWIND_CSS: Asset = asset!("public/tailwind.css");
const AUTO_NUMERIC_JS: Asset = asset!("assets/autoNumeric.min.js");

// TODO: Fix spaghetti UI
// TODO: I give up on UI, it is whatever
fn main() {
    let level = if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                humantime::format_rfc3339(std::time::SystemTime::now()),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(level)
        .chain(stdout())
        .chain(fern::log_file(current_exe().unwrap().parent().unwrap().join("log.txt")).unwrap())
        .apply()
        .unwrap();
    log_panics::init();

    backend::init();
    let window = WindowBuilder::new()
        .with_inner_size(Size::Physical(PhysicalSize::new(896, 480)))
        .with_resizable(false)
        .with_maximizable(false)
        .with_drag_and_drop(false)
        .with_title(Alphanumeric.sample_string(&mut rand::rng(), 16));
    let cfg = dioxus::desktop::Config::default()
        .with_menu(None)
        .with_window(window);
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

#[derive(Clone, Copy)]
pub struct AppState {
    config: Signal<Option<Configuration>>,
}

#[component]
fn App() -> Element {
    const TAB_ACTIONS: &str = "Actions";
    const TAB_CHARACTERS: &str = "Characters";
    const TAB_SETTINGS: &str = "Settings";
    static TABS: LazyLock<Vec<String>> = LazyLock::new(|| {
        vec![
            TAB_ACTIONS.to_string(),
            TAB_CHARACTERS.to_string(),
            TAB_SETTINGS.to_string(),
        ]
    });

    let mut selected_tab = use_signal(|| TAB_ACTIONS.to_string());
    let mut script_loaded = use_signal(|| false);

    use_context_provider(|| AppState {
        config: Signal::new(None),
    });

    // Thanks dioxus
    use_future(move || async move {
        let mut eval = document::eval(
            r#"
            const scriptInterval = setInterval(async () => {
                try {
                    AutoNumeric;
                    await dioxus.send(true);
                    clearInterval(scriptInterval);
                } catch(_) { }
            }, 10);
        "#,
        );
        eval.recv::<bool>().await.unwrap();
        script_loaded.set(true);
    });

    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Script { src: AUTO_NUMERIC_JS }
        if script_loaded() {
            div { class: "flex min-w-4xl min-h-120 h-full bg-gray-950",
                Minimap {}
                Tabs {
                    tabs: TABS.clone(),
                    on_select_tab: move |tab| {
                        selected_tab.set(tab);
                    },
                    selected_tab: selected_tab(),
                }
                div { class: "relative w-full",
                    match selected_tab().as_str() {
                        TAB_ACTIONS => rsx! {},
                        TAB_CHARACTERS => rsx! {
                            Characters {}
                        },
                        TAB_SETTINGS => rsx! {},
                        _ => unreachable!(),
                    }
                }
            }
        }
    }
}

#[derive(PartialEq, Props, Clone)]
struct TabsProps {
    tabs: Vec<String>,
    on_select_tab: EventHandler<String>,
    selected_tab: String,
}

#[component]
fn Tabs(
    TabsProps {
        tabs,
        on_select_tab,
        selected_tab,
    }: TabsProps,
) -> Element {
    rsx! {
        div { class: "flex flex-col px-2 gap-3",
            for tab in tabs {
                Tab {
                    name: tab.clone(),
                    on_click: move |_| {
                        on_select_tab(tab.clone());
                    },
                }
            }
        }
    }
}

#[component]
fn Tab(name: String, on_click: EventHandler) -> Element {
    rsx! {
        button {
            class: "flex items-center gap-2 w-32 h-10 hover:bg-gray-900",
            onclick: move |_| {
                on_click(());
            },
            div { class: "w-[20px] h-[20px] bg-zinc-700" }
            p { class: "title", {name} }
        }
    }
}
