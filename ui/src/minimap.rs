use std::{
    ops::Deref,
    time::{Duration, Instant},
};

use backend::{
    Action, ActionKey, ActionMove, DatabaseEvent, Map, Operation, OperationUpdate, Position,
    RotationMode, create_map, database_event_receiver, delete_map, query_maps, redetect_minimap,
    state_receiver, update_map, update_operation, upsert_map,
};
use dioxus::{document::EvalError, html::FileData, prelude::*};
use futures_util::StreamExt;
use serde::Serialize;
use tokio::{sync::broadcast::error::RecvError, time::sleep};

use crate::{
    AppState,
    components::{
        button::{Button, ButtonStyle},
        file::{FileInput, FileOutput},
        named_select::NamedSelect,
        select::{Select, SelectOption},
    },
};

const BACKGROUND: Asset = asset!(
    "public/background.png",
    ImageAssetOptions::new().with_webp()
);

const MINIMAP_JS: &str = r#"
    const canvas = document.getElementById("canvas-map");
    const canvasCtx = canvas.getContext("2d");

    while (true) {
        const [buffer, width, height, destinations, bound, quadrant, portals] = await dioxus.recv();
        const data = new ImageData(new Uint8ClampedArray(buffer), width, height);
        const bitmap = await createImageBitmap(data);

        canvasCtx.fillStyle = "rgb(128, 255, 204)";
        canvasCtx.strokeStyle = "rgb(128, 255, 204)";
        canvasCtx.drawImage(bitmap, 0, 0, width, height, 0, 0, canvas.width, canvas.height);

        const destinationSize = 4;
        const destinationSizeHalf = destinationSize / 2;
        let prevX = 0;
        let prevY = 0;
        for (let i = 0; i < destinations.length; i++) {
            let [x, y] = destinations[i];
            x = (x / width) * canvas.width;
            y = ((height - y) / height) * canvas.height;

            canvasCtx.fillRect(x, y, destinationSize, destinationSize);

            if (i > 0) {
                canvasCtx.beginPath();
                canvasCtx.setLineDash([8]);
                canvasCtx.moveTo(prevX + destinationSizeHalf, prevY + destinationSizeHalf);
                canvasCtx.lineTo(x + destinationSizeHalf, y + destinationSizeHalf);
                canvasCtx.stroke();
            }

            prevX = x;
            prevY = y;
        }

        canvasCtx.setLineDash([8]);
        canvasCtx.strokeStyle = "rgb(160, 155, 255)";
        for (let i = 0; i < portals.length; i++) {
            const portal = portals[i];
            const x = (portal.x / width) * canvas.width;
            const y = ((height - portal.y - portal.height) / height) * canvas.height;
            const w = (portal.width / width) * canvas.width;
            const h = (portal.height / height) * canvas.height;

            canvasCtx.strokeRect(x, y, w, h);
        }

        if (quadrant !== null && bound !== null) {
            canvasCtx.strokeStyle = "rgb(254, 71, 57)";

            const x = (bound.x / width) * canvas.width;
            const y = (bound.y / height) * canvas.height;
            const w = (bound.width / width) * canvas.width;
            const h = (bound.height / height) * canvas.height;

            const widthHalf = w / 2;
            const heightHalf = h / 2;
            const widthQuarter = widthHalf / 2;
            const heightQuarter = heightHalf / 2;

            switch (quadrant) {
                case "TopLeft": {
                    const fromX = x + widthQuarter;
                    const fromY = y + heightQuarter;
                    const toX = x + widthHalf + widthQuarter;
                    drawArrow(canvasCtx, fromX, fromY, toX, fromY);
                    break;
                }
                case "TopRight": {
                    const fromX = x + widthHalf + widthQuarter;
                    const fromY = y + heightQuarter;
                    const toY = y + heightHalf + heightQuarter;
                    drawArrow(canvasCtx, fromX, fromY, fromX, toY);
                    break;
                }
                case "BottomRight": {
                    const fromX = x + widthHalf + widthQuarter;
                    const fromY = y + heightHalf + heightQuarter;
                    const toX = x + widthQuarter;
                    drawArrow(canvasCtx, fromX, fromY, toX, fromY);
                    break;
                }
                case "BottomLeft": {
                    const fromX = x + widthQuarter;
                    const fromY = y + heightHalf + heightQuarter;
                    const toY = y + heightQuarter;
                    drawArrow(canvasCtx, fromX, fromY, fromX, toY);
                    break;
                }
                default:
                    break;
            }
        }
    }

    function drawArrow(canvasCtx, fromX, fromY, toX, toY) {
        const headSize = 10; // Length of head in pixels
        const dx = toX - fromX;
        const dy = toY - fromY;
        const angle = Math.atan2(dy, dx);

        canvasCtx.beginPath();
        canvasCtx.setLineDash([8]);
        canvasCtx.moveTo(fromX, fromY);
        canvasCtx.lineTo(toX, toY);
        canvasCtx.stroke();

        canvasCtx.beginPath();
        canvasCtx.setLineDash([]);
        canvasCtx.moveTo(toX, toY);
        canvasCtx.lineTo(toX - headSize * Math.cos(angle - Math.PI / 6), toY - headSize * Math.sin(angle - Math.PI / 6));
        canvasCtx.moveTo(toX, toY);
        canvasCtx.lineTo(toX - headSize * Math.cos(angle + Math.PI / 6), toY - headSize * Math.sin(angle + Math.PI / 6));
        canvasCtx.stroke();
    }
"#;
const MINIMAP_ACTIONS_JS: &str = r#"
    const canvas = document.getElementById("canvas-map-actions");
    const canvasCtx = canvas.getContext("2d");
    const [width, height, actions, boundAndType, platforms] = await dioxus.recv();
    canvasCtx.clearRect(0, 0, canvas.width, canvas.height);
    const anyActions = actions.filter((action) => action.condition === "Any");
    const erdaActions = actions.filter((action) => action.condition === "ErdaShowerOffCooldown");
    const millisActions = actions.filter((action) => action.condition === "EveryMillis");

    drawBound(canvasCtx, boundAndType);

    canvasCtx.setLineDash([]);
    canvasCtx.strokeStyle = "rgb(255, 160, 37)";
    for (const platform of platforms) {
        const xStart = (platform.x_start / width) * canvas.width;
        const xEnd = (platform.x_end / width) * canvas.width;
        const y = ((height - platform.y) / height) * canvas.height;
        canvasCtx.beginPath();
        canvasCtx.moveTo(xStart, y);
        canvasCtx.lineTo(xEnd, y);
        canvasCtx.stroke();
    }

    canvasCtx.setLineDash([8]);
    canvasCtx.fillStyle = "rgb(255, 153, 128)";
    canvasCtx.strokeStyle = "rgb(255, 153, 128)";
    drawActions(canvas, canvasCtx, anyActions, true);

    canvasCtx.fillStyle = "rgb(179, 198, 255)";
    canvasCtx.strokeStyle = "rgb(179, 198, 255)";
    drawActions(canvas, canvasCtx, erdaActions, true);

    canvasCtx.fillStyle = "rgb(128, 255, 204)";
    canvasCtx.strokeStyle = "rgb(128, 255, 204)";
    drawActions(canvas, canvasCtx, millisActions, false);

    function drawBound(canvasCtx, boundAndType) {
        if (boundAndType === null) {
            return;
        }
        const [bound, boundType] = boundAndType;
        if (bound.width === 0 || bound.height === 0) {
            return;
        }
        const x = (bound.x / width) * canvas.width;
        const y = (bound.y / height) * canvas.height;
        const w = (bound.width / width) * canvas.width;
        const h = (bound.height / height) * canvas.height;

        canvasCtx.strokeStyle = "rgb(152, 233, 32)";
        canvasCtx.beginPath();
        canvasCtx.setLineDash([8]);
        canvasCtx.strokeRect(x, y, w, h);

        if (boundType === "PingPong") {
            canvasCtx.strokeStyle = "rgb(254, 71, 57)";

            canvasCtx.moveTo(0, y);
            canvasCtx.lineTo(x - 5, y);

            canvasCtx.moveTo(0, y + h);
            canvasCtx.lineTo(x - 5, y + h);

            canvasCtx.moveTo(x + w + 5, y);
            canvasCtx.lineTo(canvas.width, y);

            canvasCtx.moveTo(x + w + 5, y + h);
            canvasCtx.lineTo(canvas.width, y + h);

            canvasCtx.moveTo(x, 0);
            canvasCtx.lineTo(x, y);

            canvasCtx.moveTo(x + w, 0);
            canvasCtx.lineTo(x + w, y);

            canvasCtx.moveTo(x, y + h);
            canvasCtx.lineTo(x, canvas.height);

            canvasCtx.moveTo(x + w, y + h);
            canvasCtx.lineTo(x + w, canvas.height);
        }
        if (boundType === "AutoMobbing") {
            canvasCtx.moveTo(x + w / 2, y + 2);
            canvasCtx.lineTo(x + w / 2, y + h - 2);
            
            canvasCtx.moveTo(x + 2, y + h / 2);
            canvasCtx.lineTo(x + w - 2, y + h / 2);
        }
        canvasCtx.stroke();
    }

    function drawActions(canvas, ctx, actions, hasArc) {
        const rectSize = 4;
        const rectHalf = rectSize / 2;
        let lastAction = null;
        let i = 1;

        ctx.font = '12px sans-serif';

        for (const action of actions) {
            const x = (action.x / width) * canvas.width;
            const y = ((height - action.y) / height) * canvas.height;

            ctx.fillRect(x, y, rectSize, rectSize);

            let labelX = x + rectSize / 2;
            let labelY = y + rectSize - 7;
            ctx.fillText(i, labelX, labelY);

            if (hasArc && lastAction !== null) {
                let [fromX, fromY] = lastAction;
                drawArc(ctx, fromX + rectHalf, fromY + rectHalf, x + rectHalf, y + rectHalf);
            }

            lastAction = [x, y];
            i++;
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

#[derive(PartialEq, Clone, Debug)]
struct MinimapState {
    position: Option<(i32, i32)>,
    health: Option<(u32, u32)>,
    state: String,
    normal_action: Option<String>,
    priority_action: Option<String>,
    erda_shower_state: String,
    input_state: String,
    operation: Operation,
    detected_size: Option<(usize, usize)>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum MinimapUpdate {
    Set,
    Create(String),
    Import(Map),
    Delete,
}

#[component]
pub fn MinimapScreen() -> Element {
    let mut map = use_context::<AppState>().map;
    let mut map_preset = use_context::<AppState>().map_preset;
    let mut maps = use_resource(async || query_maps().await.unwrap_or_default());
    let position = use_context::<AppState>().position;
    // Maps queried `maps` to names
    let map_names = use_memo::<Vec<String>>(move || {
        maps()
            .unwrap_or_default()
            .into_iter()
            .map(|map| map.name)
            .collect()
    });
    // Maps currently selected `map` to the index in `maps`
    let map_index = use_memo(move || {
        maps().zip(map()).and_then(|(maps, map)| {
            maps.into_iter()
                .enumerate()
                .find(|(_, data)| map.id == data.id)
                .map(|(i, _)| i)
        })
    });

    // Game state for displaying info
    let state = use_signal::<Option<MinimapState>>(|| None);
    // Handles async operations for map-related
    let coroutine = use_coroutine(move |mut rx: UnboundedReceiver<MinimapUpdate>| async move {
        while let Some(message) = rx.next().await {
            match message {
                MinimapUpdate::Set => {
                    update_map(map_preset(), map()).await;
                }
                MinimapUpdate::Create(name) => {
                    let Some(new_map) = create_map(name).await else {
                        continue;
                    };
                    let Some(new_map) = upsert_map(new_map).await else {
                        continue;
                    };

                    map.set(Some(new_map));
                    map_preset.set(None);
                    update_map(None, map()).await;
                }
                MinimapUpdate::Import(map) => {
                    upsert_map(map).await;
                }
                MinimapUpdate::Delete => {
                    if let Some(current_map) = map()
                        && delete_map(current_map).await
                    {
                        map.set(None);
                        map_preset.set(None);
                    }
                }
            }
        }
    });

    // Sets a map and preset if there is not one
    use_effect(move || {
        if let Some(maps) = maps()
            && !maps.is_empty()
            && map.peek().is_none()
        {
            map.set(maps.into_iter().next());
            map_preset.set(
                map.peek()
                    .as_ref()
                    .expect("has value")
                    .actions
                    .keys()
                    .next()
                    .cloned(),
            );
            coroutine.send(MinimapUpdate::Set);
        }
    });
    // External modification checking
    use_future(move || async move {
        let mut rx = database_event_receiver();
        loop {
            let event = match rx.recv().await {
                Ok(value) => value,
                Err(RecvError::Closed) => break,
                Err(RecvError::Lagged(_)) => continue,
            };
            if matches!(
                event,
                DatabaseEvent::MapUpdated(_) | DatabaseEvent::MapDeleted(_)
            ) {
                maps.restart();
            }
        }
    });

    rsx! {
        div { class: "relative flex flex-col flex-none w-xs xl:w-md z-0",
            div {
                class: "absolute inset-0 bg-no-repeat bg-center w-[130%] -z-1",
                style: "background-image: url({BACKGROUND}); background-size: 100%; background-position: -10px 120px;",
            }
            Canvas {
                state,
                map,
                map_preset,
                position,
            }
            Buttons { state, map }
            Info { state, map }
            div { class: "flex-grow flex items-end px-2",
                div { class: "flex flex-col items-end w-full",
                    ImportExport { map }
                    div { class: "h-10 w-full flex items-center",
                        NamedSelect {
                            class: "w-full",
                            on_create: move |name| {
                                coroutine.send(MinimapUpdate::Create(name));
                            },
                            on_delete: move |_| {
                                coroutine.send(MinimapUpdate::Delete);
                            },
                            delete_disabled: map_names().is_empty(),
                            Select::<usize> {
                                class: "w-full",
                                placeholder: "Create a map...",
                                disabled: map_names().is_empty(),
                                on_selected: move |index| {
                                    let selected: Map = maps
                                        .peek()
                                        .as_ref()
                                        .expect("should already loaded")
                                        .get(index)
                                        .cloned()
                                        .unwrap();
                                    map_preset.set(selected.actions.keys().next().cloned());
                                    map.set(Some(selected));
                                    coroutine.send(MinimapUpdate::Set);
                                },

                                for (i , name) in map_names().into_iter().enumerate() {
                                    SelectOption::<usize> {
                                        value: i,
                                        label: name,
                                        selected: map_index() == Some(i),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Canvas(
    state: Signal<Option<MinimapState>>,
    map: ReadSignal<Option<Map>>,
    map_preset: ReadSignal<Option<String>>,
    position: Signal<(i32, i32)>,
) -> Element {
    let rotation_bound_and_type = use_memo(move || {
        let map = map()?;

        match map.rotation_mode {
            RotationMode::StartToEnd | RotationMode::StartToEndThenReverse => None,
            RotationMode::AutoMobbing => Some((map.rotation_auto_mob_bound, "AutoMobbing")),
            RotationMode::PingPong => Some((map.rotation_ping_pong_bound, "PingPong")),
        }
    });

    use_effect(move || {
        let bound_and_type = rotation_bound_and_type();
        let preset = map_preset();
        let Some(map) = map() else {
            return;
        };
        let actions = preset
            .and_then(|preset| map.actions.get(&preset).cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|action| match action {
                Action::Move(ActionMove {
                    position: Position { x, y, .. },
                    condition,
                    ..
                })
                | Action::Key(ActionKey {
                    position: Some(Position { x, y, .. }),
                    condition,
                    ..
                }) => Some(ActionView {
                    x,
                    y,
                    condition: condition.to_string(),
                }),
                _ => None,
            })
            .collect::<Vec<_>>();

        spawn(async move {
            let canvas = document::eval(MINIMAP_ACTIONS_JS);
            let _ = canvas.send((
                map.width,
                map.height,
                actions,
                bound_and_type,
                map.platforms,
            ));
        });
    });
    // Draw map and update game state
    use_future(move || async move {
        let mut canvas = document::eval(MINIMAP_JS);
        let mut receiver = state_receiver().await;
        loop {
            let Ok(current_state) = receiver.recv().await else {
                continue;
            };
            let destinations = current_state.destinations;
            let quadrant = current_state
                .auto_mob_quadrant
                .map(|quadrant| quadrant.to_string());
            let frame = current_state.frame;
            let portals = current_state.portals;
            let current_state = MinimapState {
                position: current_state.position,
                health: current_state.health,
                state: current_state.state,
                normal_action: current_state.normal_action,
                priority_action: current_state.priority_action,
                erda_shower_state: current_state.erda_shower_state,
                input_state: current_state.input_state,
                operation: current_state.operation,
                detected_size: frame.as_ref().map(|(_, width, height)| (*width, *height)),
            };

            if *position.peek() != current_state.position.unwrap_or_default() {
                position.set(current_state.position.unwrap_or_default());
            }
            state.set(Some(current_state));
            sleep(Duration::from_millis(50)).await;

            let bound = rotation_bound_and_type
                .peek()
                .deref()
                .map(|(bound, _)| bound);
            let Some((frame, width, height)) = frame else {
                continue;
            };
            let Err(error) =
                canvas.send((frame, width, height, destinations, bound, quadrant, portals))
            else {
                continue;
            };
            if matches!(error, EvalError::Finished) {
                // TODO: https://github.com/DioxusLabs/dioxus/issues/2979
                canvas = document::eval(MINIMAP_JS);
            }
        }
    });

    rsx! {
        div { class: "relative h-31 xl:h-38 rounded-2xl bg-secondary-surface",
            canvas {
                class: "absolute inset-0 rounded-2xl w-full h-full",
                id: "canvas-map",
            }
            canvas {
                class: "absolute inset-0 rounded-2xl w-full h-full",
                id: "canvas-map-actions",
            }
        }
    }
}

#[component]
fn Info(state: ReadSignal<Option<MinimapState>>, map: ReadSignal<Option<Map>>) -> Element {
    #[derive(Debug, PartialEq, Clone)]
    struct GameStateInfo {
        position: String,
        health: String,
        state: String,
        normal_action: String,
        priority_action: String,
        erda_shower_state: String,
        input_state: String,
        detected_map_size: String,
        selected_map_size: String,
        run_timer_duration: String,
    }

    let info = use_memo(move || {
        let mut info = GameStateInfo {
            position: "Unknown".to_string(),
            health: "Unknown".to_string(),
            state: "Unknown".to_string(),
            normal_action: "None".to_string(),
            priority_action: "None".to_string(),
            erda_shower_state: "Unknown".to_string(),
            input_state: "Unknown".to_string(),
            detected_map_size: "Unknown".to_string(),
            selected_map_size: "Unknown".to_string(),
            run_timer_duration: "None".to_string(),
        };

        if let Some(map) = map() {
            info.selected_map_size = format!("{}px x {}px", map.width, map.height);
        }

        if let Some(state) = state() {
            info.state = state.state;
            info.erda_shower_state = state.erda_shower_state;
            info.input_state = state.input_state;
            info.run_timer_duration = match state.operation {
                Operation::Halting | Operation::Running => "None".to_string(),
                Operation::TemporaryHalting(duration) => duration_from(duration),
                Operation::RunUntil(instant) => {
                    duration_from(instant.saturating_duration_since(Instant::now()))
                }
            };
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
            if let Some((width, height)) = state.detected_size {
                info.detected_map_size = format!("{width}px x {height}px")
            }
        }

        info
    });

    rsx! {
        div { class: "grid grid-cols-2 items-center justify-center px-4 py-3 gap-1",
            InfoItem { name: "State", value: info().state }
            InfoItem { name: "Position", value: info().position }
            InfoItem { name: "HP", value: info().health }
            InfoItem { name: "Priority action", value: info().priority_action }
            InfoItem { name: "Normal action", value: info().normal_action }
            InfoItem { name: "Erda Shower", value: info().erda_shower_state }
            InfoItem { name: "Detected size", value: info().detected_map_size }
            InfoItem { name: "Selected size", value: info().selected_map_size }
            InfoItem { name: "Run timer", value: info().run_timer_duration }
            InfoItem { name: "Input method", value: info().input_state }
        }
    }
}

#[component]
fn InfoItem(name: String, value: String) -> Element {
    rsx! {
        p { class: "text-sm text-primary-text font-mono", "{name}" }
        p { class: "text-sm text-primary-text text-right font-mono", "{value}" }
    }
}

#[component]
fn Buttons(state: ReadSignal<Option<MinimapState>>, map: ReadSignal<Option<Map>>) -> Element {
    let kind = use_memo(move || {
        state()
            .map(|state| match state.operation {
                Operation::Halting => OperationUpdate::Halt,
                Operation::TemporaryHalting(_) => OperationUpdate::TemporaryHalt,
                Operation::Running | Operation::RunUntil(_) => OperationUpdate::Run,
            })
            .unwrap_or(OperationUpdate::Halt)
    });
    let character = use_context::<AppState>().character;
    let disabled = use_memo(move || map().is_none() || character().is_none());

    let start_stop_text = use_memo(move || {
        if matches!(
            kind(),
            OperationUpdate::Run | OperationUpdate::TemporaryHalt
        ) {
            "Stop"
        } else {
            "Start"
        }
    });
    let suspend_resume_text = use_memo(move || {
        state()
            .map(|state| match state.operation {
                Operation::TemporaryHalting(_) => "Resume",
                Operation::Halting | Operation::Running | Operation::RunUntil(_) => "Suspend",
            })
            .unwrap_or("Suspend")
    });
    let suspend_resume_disabled = use_memo(move || {
        if disabled() {
            return true;
        }
        state()
            .map(|state| {
                !matches!(
                    state.operation,
                    Operation::TemporaryHalting(_) | Operation::RunUntil(_)
                )
            })
            .unwrap_or_default()
    });

    rsx! {
        div { class: "flex h-10 justify-center items-center gap-4",
            Button {
                class: "w-20",
                style: ButtonStyle::Primary,
                disabled: disabled(),
                on_click: move || async move {
                    let kind = match *kind.peek() {
                        OperationUpdate::Halt => OperationUpdate::Run,
                        OperationUpdate::TemporaryHalt | OperationUpdate::Run => {
                            OperationUpdate::Halt
                        }
                    };
                    update_operation(kind).await;
                },
                {start_stop_text()}
            }
            Button {
                class: "w-20",
                style: ButtonStyle::Primary,
                disabled: suspend_resume_disabled(),
                on_click: move || async move {
                    let kind = match *kind.peek() {
                        OperationUpdate::Run => OperationUpdate::TemporaryHalt,
                        OperationUpdate::TemporaryHalt | OperationUpdate::Halt => {
                            OperationUpdate::Run
                        }
                    };
                    update_operation(kind).await;
                },
                {suspend_resume_text()}
            }
            Button {
                class: "w-20",
                style: ButtonStyle::Primary,
                on_click: move |_| async move {
                    redetect_minimap().await;
                },
                "Re-detect"
            }
        }
    }
}

#[component]
fn ImportExport(map: ReadSignal<Option<Map>>) -> Element {
    let coroutine = use_coroutine_handle::<MinimapUpdate>();

    let export_name = use_memo(move || {
        let name = map().map(|map| map.name).unwrap_or_default();
        format!("{name}.json")
    });
    let export_content = move |_| {
        map.peek()
            .as_ref()
            .and_then(|map| serde_json::to_vec_pretty(map).ok())
            .unwrap_or_default()
    };

    let import_map = use_callback(move |file: FileData| async move {
        let Ok(bytes) = file.read_bytes().await else {
            return;
        };
        let Ok(map) = serde_json::from_slice::<'_, Map>(&bytes) else {
            return;
        };

        coroutine.send(MinimapUpdate::Import(map));
    });

    rsx! {
        div { class: "flex gap-3",
            FileInput {
                on_file: move |file| async move {
                    import_map(file).await;
                },
                Button { class: "w-20", style: ButtonStyle::Primary, "Import" }
            }
            FileOutput {
                on_file: export_content,
                download: export_name(),
                disabled: map().is_none(),
                Button {
                    class: "w-20",
                    style: ButtonStyle::Primary,
                    disabled: map().is_none(),

                    "Export"
                }
            }
        }
    }
}

#[inline]
fn duration_from(duration: Duration) -> String {
    let seconds = duration.as_secs() % 60;
    let minutes = (duration.as_secs() / 60) % 60;
    let hours = (duration.as_secs() / 60) / 60;

    format!("{hours:0>2}:{minutes:0>2}:{seconds:0>2}")
}
