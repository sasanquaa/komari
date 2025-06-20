#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![feature(variant_count)]
#![feature(map_try_insert)]

use std::{env::current_exe, io::stdout, string::ToString, sync::Arc};

use action::Actions;
use backend::{
    Configuration as ConfigurationData, Minimap as MinimapData, Settings as SettingsData,
    query_configs, query_settings, update_configuration, update_settings, upsert_config,
    upsert_settings,
};
use configuration::Configuration;
use dioxus::{
    desktop::{
        WindowBuilder,
        tao::{platform::windows::WindowBuilderExtWindows, window::WindowSizeConstraints},
        wry::dpi::{PhysicalSize, PixelUnit, Size},
    },
    prelude::*,
};
use familiar::Familiars;
use fern::Dispatch;
use futures_util::StreamExt;
use log::LevelFilter;
use minimap::{Minimap, MinimapMessage};
use notification::Notifications;
use rand::distr::{Alphanumeric, SampleString};
use settings::Settings;
use tab::Tab;
use tokio::{
    sync::{
        Mutex,
        mpsc::{self},
    },
    task::spawn_blocking,
};

mod action;
mod configuration;
mod familiar;
mod icons;
mod input;
mod key;
mod minimap;
mod notification;
mod platform;
mod rotation;
mod select;
mod settings;
mod tab;

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
        .with_inner_size(Size::Physical(PhysicalSize::new(540, 864)))
        .with_inner_size_constraints(WindowSizeConstraints::new(
            Some(PixelUnit::Physical(540.into())),
            Some(PixelUnit::Physical(864.into())),
            None,
            None,
        ))
        .with_resizable(true)
        .with_drag_and_drop(false)
        .with_title(Alphanumeric.sample_string(&mut rand::rng(), 16));
    let cfg = dioxus::desktop::Config::default()
        .with_menu(None)
        .with_window(window);
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

pub enum AppMessage {
    UpdateConfig(ConfigurationData, bool),
    UpdateMinimap(MinimapData),
    UpdatePreset(String),
    UpdateSettings(SettingsData),
}

#[component]
fn App() -> Element {
    const TAB_CONFIGURATION: &str = "Configuration";
    const TAB_ACTIONS: &str = "Actions";
    const TAB_SETTINGS: &str = "Settings";
    const TAB_SETTINGS_NOTIFICATIONS: &str = "Notifications";
    const TAB_SETTINGS_FAMILIARS: &str = "Familiars";

    // TODO: Move to AppMessage?
    let (minimap_tx, minimap_rx) = mpsc::channel::<MinimapMessage>(1);
    let minimap_rx = use_signal(move || Arc::new(Mutex::new(minimap_rx)));
    let minimap = use_signal::<Option<MinimapData>>(|| None);
    let preset = use_signal::<Option<String>>(|| None);
    let mut config = use_signal::<Option<ConfigurationData>>(|| None);
    let mut configs = use_resource(move || async move {
        let configs = spawn_blocking(|| query_configs().unwrap()).await.unwrap();
        if config.peek().is_none() {
            config.set(configs.first().cloned());
            update_configuration(config.peek().clone().unwrap()).await;
        }
        configs
    });
    let mut settings = use_resource(|| async { spawn_blocking(query_settings).await.unwrap() });
    let copy_position = use_signal::<Option<(i32, i32)>>(|| None);
    let coroutine = use_coroutine(move |mut rx: UnboundedReceiver<AppMessage>| {
        let minimap_tx = minimap_tx.clone();
        async move {
            while let Some(msg) = rx.next().await {
                match msg {
                    AppMessage::UpdateConfig(mut new_config, save) => {
                        let mut id = None;
                        if save {
                            let mut new_config = new_config.clone();
                            id = spawn_blocking(move || {
                                upsert_config(&mut new_config).unwrap();
                                new_config.id
                            })
                            .await
                            .unwrap();
                        }
                        if id.is_some() && new_config.id.is_none() {
                            new_config.id = id;
                        }
                        config.set(Some(new_config.clone()));
                        update_configuration(new_config.clone()).await;
                        configs.restart();
                    }
                    AppMessage::UpdateMinimap(minimap) => {
                        let _ = minimap_tx
                            .send(MinimapMessage::UpdateMinimap(minimap, true))
                            .await;
                    }
                    AppMessage::UpdatePreset(preset) => {
                        let _ = minimap_tx
                            .send(MinimapMessage::UpdateMinimapPreset(preset))
                            .await;
                    }
                    AppMessage::UpdateSettings(mut new_settings) => {
                        update_settings(new_settings.clone()).await;
                        spawn_blocking(move || {
                            upsert_settings(&mut new_settings).unwrap();
                        })
                        .await
                        .unwrap();
                        settings.restart();
                    }
                }
            }
        }
    });
    let mut active_tab = use_signal(|| TAB_CONFIGURATION.to_string());
    let mut script_loaded = use_signal(|| false);

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
            div { class: "flex flex-col max-w-2xl h-screen mx-auto space-y-2",
                Minimap {
                    minimap_rx,
                    minimap,
                    preset,
                    copy_position,
                }
                Tab {
                    tabs: vec![
                        TAB_CONFIGURATION.to_string(),
                        TAB_ACTIONS.to_string(),
                        TAB_SETTINGS.to_string(),
                        TAB_SETTINGS_NOTIFICATIONS.to_string(),
                        TAB_SETTINGS_FAMILIARS.to_string(),
                    ],
                    class: "py-2 px-3 font-medium text-sm focus:outline-none",
                    selected_class: "bg-white text-gray-800",
                    unselected_class: "hover:text-gray-700 text-gray-400 bg-gray-100",
                    on_tab: move |tab| {
                        active_tab.set(tab);
                    },
                    tab: active_tab(),
                }
                match active_tab().as_str() {
                    TAB_CONFIGURATION => rsx! {
                        Configuration { app_coroutine: coroutine, configs, config }
                    },
                    TAB_ACTIONS => rsx! {
                        Actions {
                            app_coroutine: coroutine,
                            minimap,
                            settings,
                            preset,
                            copy_position,
                        }
                    },
                    TAB_SETTINGS => rsx! {
                        Settings { app_coroutine: coroutine, settings }
                    },
                    TAB_SETTINGS_NOTIFICATIONS => rsx! {
                        Notifications { app_coroutine: coroutine, settings }
                    },
                    TAB_SETTINGS_FAMILIARS => rsx! {
                        Familiars { app_coroutine: coroutine, settings }
                    },
                    _ => unreachable!(),
                }
            }
        }
    }
}
