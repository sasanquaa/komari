use std::{mem::discriminant, ops::Range};

use backend::{Action, ActionCondition, ActionKey};
use dioxus::{html::FileData, prelude::*};

use crate::{
    actions::{
        ActionsContext, ActionsUpdate,
        list::{ActionsList, ItemClickEvent, ItemMoveEvent},
        popup::PopupActionsInputContent,
    },
    components::{
        button::{Button, ButtonStyle},
        file::{FileInput, FileOutput},
        popup::PopupContext,
        section::Section,
    },
};

#[derive(Clone, Copy, PartialEq)]
enum PopupContent {
    None,
    Add(Action),
    Edit { action: Action, index: usize },
}

/// Finds the linked action index range where `action_index` is a non-linked action.
fn find_linked_action_range(actions: &[Action], action_index: usize) -> Option<Range<usize>> {
    if action_index + 1 >= actions.len() {
        return None;
    }
    let start = action_index + 1;
    if !matches!(actions[start].condition(), ActionCondition::Linked) {
        return None;
    }

    let mut end = start + 1;
    while end < actions.len() {
        if !matches!(actions[end].condition(), ActionCondition::Linked) {
            break;
        }
        end += 1;
    }

    Some(start..end)
}

/// Finds the last linked action index of the last action matching `condition`.
fn find_last_linked_action_index(actions: &[Action], condition: ActionCondition) -> Option<usize> {
    let condition_filter = discriminant(&condition);
    let (mut last_index, _) = actions
        .iter()
        .enumerate()
        .rev()
        .find(|(_, action)| condition_filter == discriminant(&action.condition()))?;

    if let Some(range) = find_linked_action_range(actions, last_index) {
        last_index += range.count();
    }

    Some(last_index)
}

/// Filters `actions` to find action with condition matching `condition` including linked
/// action(s) of that matching action.
///
/// Returns a [`Vec<(Action, usize)>`] where [`usize`] is the index of the action inside the
/// original `actions`.
pub fn filter_actions(actions: Vec<Action>, condition: ActionCondition) -> Vec<(Action, usize)> {
    let condition_filter = discriminant(&condition);
    let mut filtered = Vec::with_capacity(actions.len());
    let mut i = 0;
    while i < actions.len() {
        let action = actions[i];
        if condition_filter != discriminant(&action.condition()) {
            i += 1;
            continue;
        }

        filtered.push((action, i));
        if let Some(range) = find_linked_action_range(&actions, i) {
            filtered.extend(actions[range.clone()].iter().copied().zip(range.clone()));
            i += range.count();
        }
        i += 1;
    }

    filtered
}

#[component]
pub fn SectionActions(actions: Memo<Vec<Action>>, disabled: bool) -> Element {
    let coroutine = use_coroutine_handle::<ActionsUpdate>();
    let map = use_context::<ActionsContext>().map;

    let export_name = use_memo(move || format!("{}.json", map().name));
    let export_content = move |_| serde_json::to_vec_pretty(&*actions.peek()).unwrap_or_default();

    let import_actions = move |file: FileData| async move {
        let mut actions = actions();

        let Ok(bytes) = file.read_bytes().await else {
            return;
        };
        let Ok(import_actions) = serde_json::from_slice::<'_, Vec<Action>>(&bytes) else {
            return;
        };

        let mut i = 0;
        while i < import_actions.len() {
            let action = import_actions[i];
            if matches!(action.condition(), ActionCondition::Linked) {
                // Malformed
                i += 1;
                continue;
            }

            actions.push(action);
            if let Some(range) = find_linked_action_range(&import_actions, i) {
                actions.extend(import_actions[range.clone()].iter().copied());
                i += range.count();
            }
            i += 1;
        }

        coroutine.send(ActionsUpdate::Update(actions));
    };

    let add_action = move |action: Action, condition: ActionCondition| {
        let mut actions = actions();
        let index = if matches!(action.condition(), ActionCondition::Linked) {
            find_last_linked_action_index(&actions, condition)
                .map(|index| index + 1)
                .unwrap_or(actions.len())
        } else {
            actions.len()
        };

        actions.insert(index, action);
        coroutine.send(ActionsUpdate::Update(actions));
    };

    let edit_action = move |new_action: Action, index: usize| {
        let mut actions = actions();
        let Some(action) = actions.get_mut(index) else {
            return;
        };

        *action = new_action;
        coroutine.send(ActionsUpdate::Update(actions));
    };

    let delete_action = move |index: usize| {
        let mut actions = actions();
        let Some(condition) = actions.get(index).map(|action| action.condition()) else {
            return;
        };

        // Replaces the first linked action to this `action` condition
        // TODO: Maybe replace find_linked_action_range with a simple lookahead
        if !matches!(condition, ActionCondition::Linked)
            && find_linked_action_range(&actions, index).is_some()
        {
            actions[index + 1] = actions[index + 1].with_condition(condition);
        }
        actions.remove(index);
        coroutine.send(ActionsUpdate::Update(actions));
    };

    let move_action = move |event: ItemMoveEvent| {
        let ItemMoveEvent {
            from_index,
            from_condition,
            to_index_local,
            to_index,
            to_condition,
        } = event;
        let mut actions = actions();
        let action = actions.remove(from_index);

        let insert_index = if from_condition == to_condition || from_index >= to_index {
            to_index
        } else {
            to_index - 1
        };
        let insert_index = insert_index.min(actions.len());
        let action_ref = actions.insert_mut(insert_index, action);
        debug!(target: "ui/actions", "move action from {from_index} to {insert_index}");

        if from_condition != to_condition || to_index_local == 0 {
            *action_ref = action_ref.with_condition(to_condition);
        }

        coroutine.send(ActionsUpdate::Update(actions));
    };

    let mut popup_content = use_signal(|| PopupContent::None);
    let mut popup_open = use_signal(|| false);

    let mut handle_add_action_click = move |condition: ActionCondition| {
        let action = Action::Key(ActionKey::default()).with_condition(condition);
        let content = PopupContent::Add(action);
        popup_content.set(content);
    };

    let handle_edit_action_click = move |event: ItemClickEvent| {
        popup_content.set(PopupContent::Edit {
            action: event.action,
            index: event.index,
        });
    };

    rsx! {
        PopupContext {
            open: popup_open,
            on_open: move |open: bool| {
                popup_open.set(open);
            },
            Section { title: "Normal actions",
                ActionsList {
                    on_add_click: move |_| {
                        handle_add_action_click(ActionCondition::Any);
                    },
                    on_item_click: handle_edit_action_click,
                    on_item_move: move_action,
                    on_item_delete: delete_action,
                    condition: ActionCondition::Any,
                    disabled,
                    actions: actions(),
                }
            }
            Section { title: "Erda Shower off cooldown priority actions",
                ActionsList {
                    on_add_click: move |_| {
                        handle_add_action_click(ActionCondition::ErdaShowerOffCooldown);
                    },
                    on_item_click: handle_edit_action_click,
                    on_item_move: move_action,
                    on_item_delete: delete_action,
                    condition: ActionCondition::ErdaShowerOffCooldown,
                    disabled,
                    actions: actions(),
                }
            }
            Section { title: "Every milliseconds priority actions",
                ActionsList {
                    on_add_click: move |_| {
                        handle_add_action_click(ActionCondition::EveryMillis(0));
                    },
                    on_item_click: handle_edit_action_click,
                    on_item_move: move_action,
                    on_item_delete: delete_action,
                    condition: ActionCondition::EveryMillis(0),
                    disabled,
                    actions: actions(),
                }
            }
            Section { title: "Import/export actions",
                div { class: "flex gap-2",
                    FileInput {
                        class: "flex-grow",
                        on_file: move |file| async move {
                            import_actions(file).await;
                        },
                        disabled,
                        Button {
                            class: "w-full",
                            style: ButtonStyle::Primary,
                            disabled,
                            "Import"
                        }
                    }
                    FileOutput {
                        class: "flex-grow",
                        on_file: export_content,
                        download: export_name(),
                        disabled,
                        Button {
                            class: "w-full",
                            style: ButtonStyle::Primary,
                            disabled,
                            "Export"
                        }
                    }
                }
            }

            match popup_content() {
                #[allow(clippy::double_parens)]
                PopupContent::None => rsx! {},
                PopupContent::Add(action) => rsx! {
                    PopupActionsInputContent {
                        modifying: false,
                        linkable: !filter_actions(actions(), action.condition()).is_empty(),
                        on_cancel: move |_| {
                            popup_open.set(false);
                            popup_content.set(PopupContent::None);
                        },
                        on_copy: None,
                        on_value: move |(action, condition)| {
                            add_action(action, condition);
                            popup_open.set(false);
                            popup_content.set(PopupContent::None);
                        },
                        value: action,
                    }
                },
                PopupContent::Edit { action, index } => rsx! {
                    PopupActionsInputContent {
                        modifying: true,
                        linkable: filter_actions(actions(), action.condition())
                            .into_iter()
                            .next()
                            .map(|first| first.1 != index)
                            .unwrap_or_default(),
                        on_copy: move |_| {
                            popup_content.set(PopupContent::Add(action));
                        },
                        on_cancel: move |_| {
                            popup_open.set(false);
                            popup_content.set(PopupContent::None);
                        },
                        on_value: move |(action, _)| {
                            edit_action(action, index);
                            popup_open.set(false);
                            popup_content.set(PopupContent::None);
                        },
                        value: action,
                    }
                },
            }
        }
    }
}
