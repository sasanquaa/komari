use dioxus::{document::EvalError, prelude::*};
use serde::Deserialize;

use crate::components::use_user_id_or_unique;

#[derive(PartialEq, Clone, Deserialize, Debug)]
enum ListEvent {
    Move {
        from: usize,
        to: usize,
        to_list_id: String,
    },
}

#[derive(PartialEq, Clone)]
pub struct MoveEvent {
    pub from: usize,
    pub to: usize,
    pub to_list_id: String,
}

#[derive(PartialEq, Props, Clone)]
pub struct ListProps {
    id: ReadSignal<Option<String>>,
    #[props(default)]
    on_move: Callback<MoveEvent>,
    #[props(default)]
    class: String,
    #[props(default)]
    group: Option<String>,
    children: Element,
}

#[component]
pub fn List(props: ListProps) -> Element
where
{
    let class = props.class;
    let on_move = props.on_move;
    let group = props.group;
    let id = use_user_id_or_unique(props.id);

    use_effect(move || {
        let script = format!(
            r#"
            function rollback(item) {{
                const parent = item._rollbackParent;
                const prev = item._rollbackPrev;
                const next = item._rollbackNext;

                if (parent === null) {{
                    return;
                }}

                if (next !== null && next.parentNode === parent) {{
                    parent.insertBefore(item, next);
                }} else if (prev !== null && prev.parentNode === parent) {{
                    parent.insertBefore(item, prev.nextSibling);
                }} else {{
                    parent.appendChild(item);
                }}
            }}

            const element = document.getElementById("{id}");
            const sortable = Sortable.get(element);
            if (sortable !== undefined) {{
                return;
            }}

            Sortable.create(element, {{
                group: await dioxus.recv(),
                animation: 150,
                forceFallback: true,
                onStart(e) {{
                    const item = e.item;
                    item._rollbackParent = e.from;
                    item._rollbackPrev = item.previousSibling;
                    item._rollbackNext = item.nextSibling;
                }},
                onEnd: async (e) => {{
                    rollback(e.item);

                    const from = e.oldIndex;
                    const fromListId = e.from.id;

                    const to = e.newIndex;
                    const toListId = e.to.id;

                    if (from === to && fromListId === toListId) {{
                        return;
                    }}

                    const event = {{
                        "Move": {{
                            from,
                            to,
                            to_list_id: toListId
                        }}
                    }};
                    await dioxus.send(event);
                }}
            }});
            "#
        );
        let mut eval = document::eval(script.as_str());
        let _ = eval.send(group.clone());

        spawn(async move {
            loop {
                let result = eval.recv::<ListEvent>().await;
                debug!(target: "ui/list"," event received {result:?}");
                match result {
                    Ok(ListEvent::Move {
                        from,
                        to,
                        to_list_id,
                    }) => {
                        on_move(MoveEvent {
                            from,
                            to,
                            to_list_id,
                        });
                    }

                    Err(EvalError::Finished) => {
                        eval = document::eval(script.as_str());
                    }
                    Err(_) => break,
                }
            }
        });
    });

    rsx! {
        div { id, class, {props.children} }
    }
}

#[derive(PartialEq, Props, Clone)]
pub struct ListItemProps {
    #[props(default)]
    on_click: Callback,
    #[props(default)]
    class: String,
    children: Element,
}

#[component]
pub fn ListItem(props: ListItemProps) -> Element {
    let class = props.class;
    let on_click = props.on_click;

    rsx! {
        div {
            class,
            onclick: move |e| {
                e.stop_propagation();
                on_click(());
            },
            {props.children}
        }
    }
}
