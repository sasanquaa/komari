use std::sync::Arc;

use backend::{
    Action, ActionKey, ActionMove, GameState, Minimap as MinimapData, RotationMode, create_minimap,
    delete_map, game_state, minimap_frame, minimap_platforms_bound, query_maps, redetect_minimap,
    rotate_actions, rotate_actions_halting, update_minimap, upsert_map,
};
use dioxus::{document::EvalError, prelude::*};
use futures_util::StreamExt;
use serde::Serialize;
use tokio::task::spawn_blocking;

use crate::{AppState, select::TextSelect};

const MINIMAP_JS: &str = r#"
    const canvas = document.getElementById("canvas-minimap");
    const canvasCtx = canvas.getContext("2d");
    let lastWidth = canvas.width;
    let lastHeight = canvas.height;

    while (true) {
        const [buffer, width, height, destinations] = await dioxus.recv();
        const data = new ImageData(new Uint8ClampedArray(buffer), width, height);
        const bitmap = await createImageBitmap(data);
        canvasCtx.beginPath()
        canvasCtx.fillStyle = "rgb(128, 255, 204)";
        canvasCtx.strokeStyle = "rgb(128, 255, 204)";
        canvasCtx.drawImage(bitmap, 0, 0);
        if (lastWidth != width || lastHeight != height) {
            lastWidth = width;
            lastHeight = height;
            canvas.width = width;
            canvas.height = height;
        }
        // TODO: ??????????????????????????
        let prevX = 0;
        let prevY = 0;
        for (let i = 0; i < destinations.length; i++) {
            let [x, y] = destinations[i];
            x = (x / width) * canvas.width;
            y = ((height - y) / height) * canvas.height;
            canvasCtx.fillRect(x - 2, y - 2, 2, 2);
            if (i > 0) {
                canvasCtx.moveTo(prevX, prevY);
                canvasCtx.lineTo(x, y);
                canvasCtx.stroke();
            }
            prevX = x;
            prevY = y;
        }
    }
"#;
const MINIMAP_ACTIONS_JS: &str = r#"
    const canvas = document.getElementById("canvas-minimap-actions");
    const canvasCtx = canvas.getContext("2d");
    const [width, height, actions, boundEnabled, bound, platforms] = await dioxus.recv();
    canvasCtx.clearRect(0, 0, canvas.width, canvas.height);
    const anyActions = actions.filter((action) => action.condition === "Any");
    const erdaActions = actions.filter((action) => action.condition === "ErdaShowerOffCooldown");
    const millisActions = actions.filter((action) => action.condition === "EveryMillis");

    canvasCtx.fillStyle = "rgb(255, 153, 128)";
    canvasCtx.strokeStyle = "rgb(255, 153, 128)";
    drawActions(canvas, canvasCtx, anyActions, true);
    if (boundEnabled) {
        const x = (bound.x / width) * canvas.width;
        const y = (bound.y / height) * canvas.height;
        const w = (bound.width / width) * canvas.width;
        const h = (bound.height / height) * canvas.height;
        canvasCtx.beginPath();
        canvasCtx.globalAlpha = 0.6;
        canvasCtx.fillRect(x, y, w, h);
        canvasCtx.globalAlpha = 1.0;
        canvasCtx.stroke();
    }
    for (const platform of platforms) {
        const xStart = (platform.x_start / width) * canvas.width;
        const xEnd = (platform.x_end / width) * canvas.width;
        const y = ((height - platform.y) / height) * canvas.height;
        canvasCtx.beginPath();
        canvasCtx.moveTo(xStart, y);
        canvasCtx.lineTo(xEnd, y);
        canvasCtx.stroke();
    }

    canvasCtx.fillStyle = "rgb(179, 198, 255)";
    canvasCtx.strokeStyle = "rgb(179, 198, 255)";
    drawActions(canvas, canvasCtx, erdaActions, true);

    canvasCtx.fillStyle = "rgb(128, 255, 204)";
    canvasCtx.strokeStyle = "rgb(128, 255, 204)";
    drawActions(canvas, canvasCtx, millisActions, false);

    function drawActions(canvas, ctx, actions, hasArc) {
        const rectSize = 4;
        const rectHalf = rectSize / 2;
        let lastAction = null;
        for (const action of actions) {
            const x = (action.x / width) * canvas.width;
            const y = ((height - action.y) / height) * canvas.height;
            ctx.fillRect(x, y, rectSize, rectSize);
            if (!hasArc) {
                continue;
            }
            if (lastAction !== null) {
                let [fromX, fromY] = lastAction;
                drawArc(ctx, fromX + rectHalf, fromY + rectHalf, x + rectHalf, y + rectHalf);
            }
            lastAction = [x, y];
        }
    }
    function drawArc(ctx, fromX, fromY, toX, toY) {
        const cx = (fromX + toX) / 2;
        const cy = (fromY + toY) / 2;
        const dx = cx - fromX;
        const dy = cy - fromY;
        const radius = Math.sqrt(dx * dx + dy * dy);
        const startAngle = Math.atan2(fromY - cy, fromX - cx);
        const endAngle = Math.atan2(toY - cy, toX - cx);
        ctx.beginPath();
        ctx.arc(cx, cy, radius, startAngle, endAngle, false);
        ctx.stroke();
    }
"#;

#[derive(Clone, PartialEq, Serialize)]
struct ActionView {
    x: i32,
    y: i32,
    condition: String,
}

#[derive(Debug)]
enum MinimapUpdate {
    Set,
    Create(String),
    Delete,
}

#[component]
pub fn Minimap() -> Element {
    let mut minimap = use_context::<AppState>().minimap;
    let mut minimaps = use_resource(|| async {
        spawn_blocking(|| query_maps().expect("failed to query maps"))
            .await
            .unwrap()
    });
    // Maps queried `minimaps` to names
    let minimap_names = use_memo(move || {
        minimaps()
            .unwrap_or_default()
            .into_iter()
            .map(|minimap| minimap.name)
            .collect()
    });
    // Maps currently selected `minimap` to the index in `minimaps`
    let minimap_index = use_memo(move || {
        minimaps().zip(minimap()).and_then(|(minimaps, minimap)| {
            minimaps
                .into_iter()
                .enumerate()
                .find(|(_, data)| minimap.id == data.id)
                .map(|(i, _)| i)
        })
    });

    // Game state for displaying info
    let state = use_signal::<Option<GameState>>(|| None);
    let detected_minimap_size = use_signal::<Option<(usize, usize)>>(|| None);
    // Handles async operations for minimap-related
    let coroutine = use_coroutine(move |mut rx: UnboundedReceiver<MinimapUpdate>| async move {
        while let Some(message) = rx.next().await {
            match message {
                MinimapUpdate::Set => {
                    update_minimap(None, minimap().expect("minimap must be already set")).await;
                }
                MinimapUpdate::Create(name) => {
                    let Some(mut new_minimap) = create_minimap(name).await else {
                        return;
                    };
                    let mut save_minimap = new_minimap.clone();
                    let save_id = spawn_blocking(move || {
                        upsert_map(&mut save_minimap).unwrap();
                        save_minimap
                            .id
                            .expect("minimap id must be valid after creation")
                    })
                    .await
                    .unwrap();

                    new_minimap.id = Some(save_id);
                    minimap.set(Some(new_minimap));
                    minimaps.restart();
                }
                MinimapUpdate::Delete => {
                    if let Some(minimap) = minimap.take() {
                        spawn_blocking(move || {
                            delete_map(&minimap).expect("failed to delete minimap");
                        })
                        .await
                        .unwrap();
                        minimaps.restart();
                    }
                }
            }
        }
    });

    // Sets a minimap if there is not one
    use_effect(move || {
        if let Some(minimaps) = minimaps()
            && !minimaps.is_empty()
            && minimap.peek().is_none()
        {
            minimap.set(minimaps.into_iter().next());
            coroutine.send(MinimapUpdate::Set);
        }
    });

    rsx! {
        div { class: "flex flex-col min-w-xs max-w-xs",
            Canvas { state, detected_minimap_size }
            Buttons {}
            Info { state, detected_minimap_size, minimap }
            div { class: "flex-grow flex items-end px-2",
                div { class: "h-10 w-full flex items-center",
                    TextSelect {
                        class: "h-6 w-full",
                        options: minimap_names(),
                        disabled: false,
                        placeholder: "Create a map...",
                        on_create: move |name| {
                            coroutine.send(MinimapUpdate::Create(name));
                            coroutine.send(MinimapUpdate::Set);
                        },
                        on_delete: move |_| {
                            coroutine.send(MinimapUpdate::Delete);
                        },
                        on_select: move |(index, _)| {
                            let selected = minimaps.peek().as_ref().unwrap().get(index).cloned().unwrap();
                            minimap.set(Some(selected));
                            coroutine.send(MinimapUpdate::Set);
                        },
                        selected: minimap_index(),
                    }
                }
            }
        }
    }
}

#[component]
fn Canvas(
    state: Signal<Option<GameState>>,
    detected_minimap_size: Signal<Option<(usize, usize)>>,
) -> Element {
    // Draw minimap and update game state
    use_effect(move || {
        spawn(async move {
            let mut canvas = document::eval(MINIMAP_JS);
            loop {
                let current_state = game_state().await;
                let destinations = current_state.destinations.clone();
                state.set(Some(current_state));

                let minimap_frame = minimap_frame().await;
                let Ok((frame, width, height)) = minimap_frame else {
                    if detected_minimap_size.peek().is_some() {
                        detected_minimap_size.set(None);
                    }
                    continue;
                };

                if detected_minimap_size.peek().is_none() {
                    detected_minimap_size.set(Some((width, height)));
                }

                let Err(error) = canvas.send((frame, width, height, destinations)) else {
                    continue;
                };
                if matches!(error, EvalError::Finished) {
                    // probably: https://github.com/DioxusLabs/dioxus/issues/2979
                    canvas = document::eval(MINIMAP_JS);
                }
            }
        });
    });

    rsx! {
        div { class: "h-31 rounded-2xl bg-gray-900",
            canvas { class: "rounded-2xl w-full h-full", id: "canvas-minimap" }
        }
    }
}

#[component]
fn Info(
    state: ReadOnlySignal<Option<GameState>>,
    detected_minimap_size: ReadOnlySignal<Option<(usize, usize)>>,
    minimap: ReadOnlySignal<Option<MinimapData>>
) -> Element {
    #[derive(Debug, PartialEq, Clone)]
    struct GameStateInfo {
        position: String,
        health: String,
        state: String,
        normal_action: String,
        priority_action: String,
        erda_shower_state: String,
        detected_minimap_size: String,
        selected_minimap_size: String,
    }

    let info = use_memo(move || {
        let mut info = GameStateInfo {
            position: "Unknown".to_string(),
            health: "Unknown".to_string(),
            state: "Unknown".to_string(),
            normal_action: "Unknown".to_string(),
            priority_action: "Unknown".to_string(),
            erda_shower_state: "Unknown".to_string(),
            detected_minimap_size: "Unknown".to_string(),
            selected_minimap_size: "Unknown".to_string(),
        };

        if let Some(minimap) = minimap() {
            info.selected_minimap_size = format!("{}px x {}px", minimap.width, minimap.height);
        }

        if let Some((width, height)) = detected_minimap_size() {
            info.detected_minimap_size = format!("{width}px x {height}px")
        }

        if let Some(state) = state() {
            info.state = state.state;
            info.erda_shower_state = state.erda_shower_state;
            if let Some((x, y)) = state.position {
                info.position = format!("{x}, {y}");
            }
            if let Some((current, max)) = state.health {
                info.health = format!("{current} / {max}");
            }
            if let Some(action) = state.normal_action {
                info.normal_action = action;
            }
            if let Some(action) = state.priority_action {
                info.priority_action = action;
            }
        }

        info
    });

    rsx! {
        div { class: "flex flex-col justify-center px-4 py-3 gap-1 border-b border-gray-600",
            InfoItem { name: "State", value: info().state }
            InfoItem { name: "Position", value: info().position }
            InfoItem { name: "Health", value: info().health }
            InfoItem { name: "Priority action", value: info().priority_action }
            InfoItem { name: "Normal action", value: info().normal_action }
            InfoItem { name: "Erda Shower", value: info().erda_shower_state }
            InfoItem { name: "Detected size", value: info().detected_minimap_size }
            InfoItem { name: "Selected size", value: info().selected_minimap_size }
        }
    }
}

#[component]
fn InfoItem(name: String, value: String) -> Element {
    rsx! {
        div { class: "flex paragraph font-mono", "{name} : {value}" }
    }
}

#[component]
fn Buttons() -> Element {
    let mut halting = use_signal(|| false);

    use_future(move || async move {
        loop {
            let current_halting = *halting.peek();
            let new_halting = rotate_actions_halting().await;
            if current_halting != new_halting {
                halting.toggle();
            }
        }
    });

    rsx! {
        div { class: "flex h-10 justify-center items-center gap-4",
            Button {
                text: if halting() { "Start" } else { "Stop" },
                on_click: move || async move {
                    rotate_actions(!*halting.peek()).await;
                },
            }
            Button {
                text: "Re-detect",
                on_click: move |_| async move {
                    redetect_minimap().await;
                },
            }
        }
    }
}

#[component]
fn Button(text: String, on_click: EventHandler) -> Element {
    rsx! {
        button {
            class: "px-2 w-20 h-6 paragraph-xs button-primary",
            onclick: move |e| {
                e.stop_propagation();
                on_click(());
            },
            {text}
        }
    }
}
