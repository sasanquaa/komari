use dioxus::prelude::*;

use crate::select::TextSelect;

// #[component]
// pub fn Actions() -> Element {
//     let mut config = use_context::<AppState>().config;
//     let mut configs = use_resource(move || async move {
//         spawn_blocking(|| query_configs().expect("failed to query configs"))
//             .await
//             .unwrap()
//     });
//     // Maps queried `configs` to names
//     let config_names = use_memo(move || {
//         configs()
//             .unwrap_or_default()
//             .into_iter()
//             .map(|config| config.name)
//             .collect()
//     });
//     // Maps currently selected `config` to the index in `configs`
//     let config_index = use_memo(move || {
//         configs().zip(config()).and_then(|(configs, config)| {
//             configs
//                 .iter()
//                 .enumerate()
//                 .find(|(_, cfg)| config.id == cfg.id)
//                 .map(|(i, _)| i)
//         })
//     });
//     // Default config if `config` is `None`
//     let config_view = use_memo(move || config().unwrap_or_default());
//     // Handles async operations for configuration-related
//     let coroutine = use_coroutine(
//         move |mut rx: UnboundedReceiver<ConfigurationUpdate>| async move {
//             while let Some(message) = rx.next().await {
//                 match message {
//                     ConfigurationUpdate::Set => {
//                         update_configuration(config().expect("config must be already set")).await;
//                     }
//                     ConfigurationUpdate::Save => {
//                         let mut config = config().expect("config must be already set");
//                         debug_assert!(config.id.is_some(), "saving invalid config");

//                         spawn_blocking(move || {
//                             upsert_config(&mut config).unwrap();
//                         })
//                         .await
//                         .unwrap();
//                     }
//                     ConfigurationUpdate::Create(name) => {
//                         let mut new_config = Configuration {
//                             name,
//                             ..Configuration::default()
//                         };
//                         let mut save_config = new_config.clone();
//                         let save_id = spawn_blocking(move || {
//                             upsert_config(&mut save_config).unwrap();
//                             save_config
//                                 .id
//                                 .expect("config id must be valid after creation")
//                         })
//                         .await
//                         .unwrap();

//                         new_config.id = Some(save_id);
//                         config.set(Some(new_config));
//                         configs.restart();
//                     }
//                     ConfigurationUpdate::Delete => {
//                         if let Some(config) = config.take() {
//                             spawn_blocking(move || {
//                                 delete_config(&config).expect("failed to delete config");
//                             })
//                             .await
//                             .unwrap();
//                             configs.restart();
//                         }
//                     }
//                 }
//             }
//         },
//     );
//     let save_config = use_callback(move |new_config: Configuration| {
//         config.set(Some(new_config));
//         coroutine.send(ConfigurationUpdate::Save);
//         coroutine.send(ConfigurationUpdate::Set);
//     });

//     // Sets a configuration if there is not one
//     use_effect(move || {
//         if let Some(configs) = configs()
//             && config.peek().is_none()
//         {
//             config.set(configs.first().cloned());
//             coroutine.send(ConfigurationUpdate::Set);
//         }
//     });

//     rsx! {
//         div { class: "flex flex-col pb-15 h-full overflow-y-auto scrollbar",
//         }
//         div { class: "flex items-center w-full h-10 bg-gray-950 absolute bottom-0",
//             TextSelect {
//                 class: "h-6 flex-grow",
//                 options: config_names(),
//                 disabled: false,
//                 on_create: move |name| {
//                     coroutine.send(ConfigurationUpdate::Create(name));
//                     coroutine.send(ConfigurationUpdate::Set);
//                 },
//                 on_delete: move |_| {
//                     coroutine.send(ConfigurationUpdate::Delete);
//                 },
//                 on_select: move |(index, _)| {
//                     let selected = configs.peek().as_ref().unwrap().get(index).cloned().unwrap();
//                     config.set(Some(selected));
//                     coroutine.send(ConfigurationUpdate::Set);
//                 },
//                 selected: config_index(),
//             }
//         }
//     }
// }
