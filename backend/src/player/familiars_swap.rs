use std::fmt::Display;

use opencv::core::Rect;
use platforms::windows::KeyKind;

use super::{
    Player, PlayerState,
    actions::on_action,
    timeout::{Timeout, update_with_timeout},
};
use crate::{
    array::Array,
    bridge::MouseAction,
    context::Context,
    database::{FamiliarRarity, SwappableFamiliars},
    detect::{FamiliarLevel, FamiliarRank},
};

/// Number of familiar slots available.
const FAMILIAR_SLOTS: usize = 3;

/// Internal state machine representing the current stage of familiar swapping.
#[derive(Debug, Clone, Copy)]
enum SwappingStage {
    /// Opening the familiar menu.
    OpenMenu(Timeout),
    /// Clicking on the "Setup" tab in the familiar UI.
    OpenSetup(Timeout),
    /// Find the familiar slots.
    FindSlots,
    /// Release the slots.
    FreeSlots(usize, bool),
    /// Release a single slot.
    FreeSlot(Timeout, usize),
    /// Find swappable familiar cards.
    FindCards,
    /// Swapping a card into an empty slot.
    Swapping(Timeout, usize),
    /// Scrolling the list to find more cards.
    Scrolling(Timeout, Option<Rect>),
    /// Saving the familiar setup.
    Saving(Timeout, Option<Rect>),
    Completed,
}

/// Struct for storing familiar swapping data.
#[derive(Debug, Clone, Copy)]
pub struct FamiliarsSwapping {
    /// Current stage of the familiar swapping state machine.
    stage: SwappingStage,
    /// Detected familiar slots with free/occupied status.
    slots: Array<(Rect, bool), 3>,
    /// Detected familiar cards and ranks.
    cards: Array<(Rect, FamiliarRank), 64>,
    /// Indicates which familiar slots are allowed to be swapped.
    swappable_slots: SwappableFamiliars,
    /// Only familiars with these rarities will be considered for swapping.
    swappable_rarities: Array<FamiliarRarity, 2>,
}

impl Display for FamiliarsSwapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.stage {
            SwappingStage::OpenMenu(_) => write!(f, "Opening"),
            SwappingStage::OpenSetup(_) => write!(f, "Opening Setup"),
            SwappingStage::FindSlots => write!(f, "Find Slots"),
            SwappingStage::FreeSlots(_, _) | SwappingStage::FreeSlot(_, _) => {
                write!(f, "Freeing Slots")
            }
            SwappingStage::FindCards => write!(f, "Finding Cards"),
            SwappingStage::Swapping(_, _) => write!(f, "Swapping"),
            SwappingStage::Scrolling(_, _) => write!(f, "Scrolling"),
            SwappingStage::Saving(_, _) => write!(f, "Saving"),
            SwappingStage::Completed => write!(f, "Completed"),
        }
    }
}

impl FamiliarsSwapping {
    pub fn new(
        swappable_slots: SwappableFamiliars,
        swappable_rarities: Array<FamiliarRarity, 2>,
    ) -> Self {
        Self {
            stage: SwappingStage::OpenMenu(Timeout::default()),
            slots: Array::new(),
            cards: Array::new(),
            swappable_slots,
            swappable_rarities,
        }
    }
}

impl FamiliarsSwapping {
    #[inline]
    fn stage(self, stage: SwappingStage) -> FamiliarsSwapping {
        FamiliarsSwapping { stage, ..self }
    }

    #[inline]
    fn stage_open_menu(self, timeout: Timeout) -> FamiliarsSwapping {
        self.stage(SwappingStage::OpenMenu(timeout))
    }

    #[inline]
    fn stage_open_setup(self, timeout: Timeout) -> FamiliarsSwapping {
        self.stage(SwappingStage::OpenSetup(timeout))
    }

    #[inline]
    fn stage_free_slots(self, index: usize, was_freeing: bool) -> FamiliarsSwapping {
        self.stage(SwappingStage::FreeSlots(index, was_freeing))
    }

    #[inline]
    fn stage_free_slot(self, timeout: Timeout, index: usize) -> FamiliarsSwapping {
        self.stage(SwappingStage::FreeSlot(timeout, index))
    }

    #[inline]
    fn stage_swapping(self, timeout: Timeout, index: usize) -> FamiliarsSwapping {
        self.stage(SwappingStage::Swapping(timeout, index))
    }

    #[inline]
    fn stage_scrolling(self, timeout: Timeout, scrollbar: Option<Rect>) -> FamiliarsSwapping {
        self.stage(SwappingStage::Scrolling(timeout, scrollbar))
    }

    #[inline]
    fn stage_saving(self, timeout: Timeout, save: Option<Rect>) -> FamiliarsSwapping {
        self.stage(SwappingStage::Saving(timeout, save))
    }
}

pub fn update_familiars_swapping_context(
    context: &Context,
    state: &mut PlayerState,
    swapping: FamiliarsSwapping,
) -> Player {
    let swapping = match swapping.stage {
        SwappingStage::OpenMenu(timeout) => {
            update_open_menu(context, state.config.familiar_key, swapping, timeout)
        }
        SwappingStage::OpenSetup(timeout) => open_setup(context, swapping, timeout),
        SwappingStage::FindSlots => update_find_slots(context, swapping),
        SwappingStage::FreeSlots(index, was_freeing) => {
            update_free_slots(context, swapping, index, was_freeing)
        }
        SwappingStage::FreeSlot(timeout, index) => {
            update_free_slot(context, swapping, timeout, index)
        }
        SwappingStage::FindCards => update_find_cards(context, swapping),
        SwappingStage::Swapping(timeout, index) => {
            update_swapping(context, swapping, timeout, index)
        }
        SwappingStage::Scrolling(timeout, scrollbar) => {
            update_scrolling(context, swapping, timeout, scrollbar)
        }
        SwappingStage::Saving(timeout, save) => update_saving(context, swapping, timeout, save),
        SwappingStage::Completed => unreachable!(),
    };
    let next = if matches!(swapping.stage, SwappingStage::Completed) {
        let _ = context.keys.send(KeyKind::Esc);
        Player::Idle
    } else {
        Player::FamiliarsSwapping(swapping)
    };

    on_action(
        state,
        |_| Some((next, matches!(next, Player::Idle))),
        || next,
    )
}

fn update_open_menu(
    context: &Context,
    key: KeyKind,
    swapping: FamiliarsSwapping,
    timeout: Timeout,
) -> FamiliarsSwapping {
    if swapping.swappable_rarities.is_empty() {
        return swapping.stage(SwappingStage::Completed);
    }

    update_with_timeout(
        timeout,
        5,
        |timeout| {
            let _ = context.keys.send_mouse(50, 50, MouseAction::MoveOnly);
            if context
                .detector_unwrap()
                .detect_familiar_setup_button()
                .is_ok()
            {
                swapping.stage_open_setup(Timeout::default())
            } else {
                // Try open familiar menu until familiar setup button shows up
                let _ = context.keys.send(key);
                swapping.stage_open_menu(timeout)
            }
        },
        || swapping.stage_open_menu(Timeout::default()),
        |timeout| swapping.stage_open_menu(timeout),
    )
}

fn open_setup(
    context: &Context,
    swapping: FamiliarsSwapping,
    timeout: Timeout,
) -> FamiliarsSwapping {
    update_with_timeout(
        timeout,
        5,
        |timeout| {
            // Try click familiar menu setup button every one second until it becomes
            // undetectable
            if let Ok(bbox) = context.detector_unwrap().detect_familiar_setup_button() {
                let x = bbox.x + bbox.width / 2;
                let y = bbox.y + bbox.height / 2;
                let _ = context.keys.send_mouse(x, y, MouseAction::Click);
            }
            swapping.stage_open_setup(timeout)
        },
        || {
            if context
                .detector_unwrap()
                .detect_familiar_setup_button()
                .is_ok()
            {
                swapping.stage_open_setup(Timeout::default())
            } else {
                // This could also indicate familiar menu already closed. If that is the case,
                // find slots will handle it. And send to mouse rest position for detecting slots.
                let _ = context.keys.send_mouse(50, 50, MouseAction::MoveOnly);
                swapping.stage(SwappingStage::FindSlots)
            }
        },
        |timeout| swapping.stage_open_setup(timeout),
    )
}

fn update_find_slots(context: &Context, mut swapping: FamiliarsSwapping) -> FamiliarsSwapping {
    // Detect familiar slots and whether each slot is free
    if swapping.slots.is_empty() {
        let vec = context.detector_unwrap().detect_familiar_slots();
        if vec.len() == FAMILIAR_SLOTS {
            for pair in vec {
                swapping.slots.push(pair);
            }
        } else {
            // Weird spots with false positives
            return swapping.stage(SwappingStage::Completed);
        }
    }

    if swapping.slots.is_empty() {
        // Still empty, bail and retry as this could indicate the menu closed/overlap
        swapping.stage_open_menu(Timeout::default())
    } else {
        swapping.stage_free_slots(FAMILIAR_SLOTS - 1, false)
    }
}

fn update_free_slots(
    context: &Context,
    swapping: FamiliarsSwapping,
    index: usize,
    was_freeing: bool,
) -> FamiliarsSwapping {
    let (_, is_free) = swapping.slots[index];
    match (is_free, index) {
        (true, index) if index > 0 => swapping.stage_free_slots(index - 1, false),
        (true, 0) => {
            if swapping.slots.iter().any(|slot| slot.1) {
                let _ = context.keys.send_mouse(50, 50, MouseAction::MoveOnly);
                swapping.stage(SwappingStage::FindCards)
            } else {
                swapping.stage(SwappingStage::Completed)
            }
        }
        (false, _) => {
            let mut swapping = swapping;
            let can_free = match swapping.swappable_slots {
                SwappableFamiliars::All => true,
                SwappableFamiliars::Last => index == FAMILIAR_SLOTS - 1,
                SwappableFamiliars::SecondAndLast => {
                    index == FAMILIAR_SLOTS - 1 || index == FAMILIAR_SLOTS - 2
                }
            };
            if !can_free {
                return if swapping.slots.iter().any(|slot| slot.1) {
                    let _ = context.keys.send_mouse(50, 50, MouseAction::MoveOnly);
                    swapping.stage(SwappingStage::FindCards)
                } else {
                    swapping.stage(SwappingStage::Completed)
                };
            }

            if was_freeing {
                // Bail and retry as this could indicate the menu closed/overlap
                swapping.slots = Array::new();
                swapping.stage_open_menu(Timeout::default())
            } else {
                swapping.stage_free_slot(Timeout::default(), index)
            }
        }
        (true, _) => unreachable!(),
    }
}

fn update_free_slot(
    context: &Context,
    swapping: FamiliarsSwapping,
    timeout: Timeout,
    index: usize,
) -> FamiliarsSwapping {
    const FAMILIAR_FREE_SLOTS_TIMEOUT: u32 = 10;
    const FAMILIAR_CHECK_FREE_TICK: u32 = 5;
    const FAMILIAR_CHECK_LVL_5_TICK: u32 = FAMILIAR_FREE_SLOTS_TIMEOUT;

    let detector = context.detector_unwrap();
    let (bbox, _) = swapping.slots[index];
    let bbox_x = bbox.x + bbox.width / 2;
    let bbox_y = bbox.y + bbox.height / 2;

    update_with_timeout(
        timeout,
        FAMILIAR_FREE_SLOTS_TIMEOUT,
        |timeout| {
            // On start, send mouse to rest position for checking free slot
            let _ = context
                .keys
                .send_mouse(bbox_x, bbox.y - 20, MouseAction::MoveOnly);
            swapping.stage_free_slot(timeout, index)
        },
        || swapping.stage_free_slots(index, true),
        |mut timeout| {
            let mut swapping = swapping;
            match timeout.current {
                FAMILIAR_CHECK_FREE_TICK => {
                    if detector.detect_familiar_slot_is_free(bbox) {
                        // If familiar is free, timeout and set flag
                        timeout.current = FAMILIAR_FREE_SLOTS_TIMEOUT;
                        swapping.slots[index].1 = true;
                    } else {
                        // This else could mean the menu is already closed, so the update here
                        // can be wrong. But the below level check should be able to handle this
                        // case.
                        for i in index + 1..FAMILIAR_SLOTS {
                            swapping.slots[i].1 =
                                detector.detect_familiar_slot_is_free(swapping.slots[i].0);
                        }
                        // Otherwise, move mouse to hover over to familiar slot to check level.
                        // After double clicking, the previous slot will move forward so this
                        // account for that too.
                        let _ = context
                            .keys
                            .send_mouse(bbox_x, bbox_y, MouseAction::MoveOnly);
                    }
                }
                FAMILIAR_CHECK_LVL_5_TICK => {
                    match detector.detect_familiar_hover_level() {
                        Ok(FamiliarLevel::Level5) => {
                            // Double click to free
                            let _ = context.keys.send_mouse(bbox_x, bbox_y, MouseAction::Click);
                            let _ = context.keys.send_mouse(bbox_x, bbox_y, MouseAction::Click);
                            // Restart from start to check again
                            timeout = Timeout::default();
                        }
                        Ok(FamiliarLevel::LevelOther) => {
                            return if index > 0 {
                                swapping.stage_free_slots(index - 1, false)
                            } else if swapping.slots.iter().any(|slot| slot.1) {
                                let _ = context.keys.send_mouse(50, 50, MouseAction::MoveOnly);
                                swapping.stage(SwappingStage::FindCards)
                            } else {
                                swapping.stage(SwappingStage::Completed)
                            };
                        }
                        // TODO: recoverable?
                        Err(_) => return swapping.stage(SwappingStage::Completed),
                    }
                }
                _ => (),
            }

            swapping.stage_free_slot(timeout, index)
        },
    )
}

fn update_find_cards(context: &Context, mut swapping: FamiliarsSwapping) -> FamiliarsSwapping {
    if swapping.cards.is_empty() {
        let vec = context.detector_unwrap().detect_familiar_cards();
        if vec.is_empty() {
            return swapping.stage_scrolling(Timeout::default(), None);
        }
        for pair in vec {
            let rarity = match pair.1 {
                FamiliarRank::Rare => FamiliarRarity::Rare,
                FamiliarRank::Epic => FamiliarRarity::Epic,
            };
            if swapping.swappable_rarities.iter().any(|r| *r == rarity) {
                swapping.cards.push(pair);
            }
        }
    }

    if swapping.cards.is_empty() {
        // Try scroll
        swapping.stage_scrolling(Timeout::default(), None)
    } else {
        swapping.stage_swapping(Timeout::default(), 0)
    }
}

fn update_swapping(
    context: &Context,
    swapping: FamiliarsSwapping,
    timeout: Timeout,
    index: usize,
) -> FamiliarsSwapping {
    let detector = context.detector_unwrap();
    let (bbox, _) = swapping.cards[index];
    let x = bbox.x + bbox.width / 2;
    let y = bbox.y + bbox.height / 2;

    update_with_timeout(
        timeout,
        10,
        |timeout| {
            let _ = context.keys.send_mouse(x, y, MouseAction::MoveOnly);
            swapping.stage_swapping(timeout, index)
        },
        || {
            // Check free slot in timeout
            let mut swapping = swapping;
            for i in 0..FAMILIAR_SLOTS {
                swapping.slots[i].1 = detector.detect_familiar_slot_is_free(swapping.slots[i].0);
            }
            if swapping.slots.iter().all(|slot| !slot.1) {
                swapping.stage(SwappingStage::Saving(Timeout::default(), None))
            } else if index + 1 < swapping.cards.len() {
                swapping.stage_swapping(Timeout::default(), index + 1)
            } else {
                let _ = context.keys.send_mouse(50, 50, MouseAction::MoveOnly);
                swapping.stage_scrolling(Timeout::default(), None)
            }
        },
        |timeout| {
            if timeout.current == 5 {
                match detector.detect_familiar_hover_level() {
                    Ok(FamiliarLevel::Level5) => {
                        if index + 1 < swapping.cards.len() {
                            return swapping.stage_swapping(Timeout::default(), index + 1);
                        } else {
                            let _ = context.keys.send_mouse(50, 50, MouseAction::MoveOnly);
                        };
                    }
                    Ok(FamiliarLevel::LevelOther) => {
                        // Double click to select
                        let _ = context.keys.send_mouse(x, y, MouseAction::Click);
                        let _ = context.keys.send_mouse(x, y, MouseAction::Click);
                        let _ = context.keys.send_mouse(50, 50, MouseAction::MoveOnly);
                    }
                    // TODO: recoverable?
                    Err(_) => return swapping.stage(SwappingStage::Completed),
                }
            }
            swapping.stage_swapping(timeout, index)
        },
    )
}

fn update_scrolling(
    context: &Context,
    swapping: FamiliarsSwapping,
    timeout: Timeout,
    mut scrollbar: Option<Rect>,
) -> FamiliarsSwapping {
    let detector = context.detector_unwrap();
    if scrollbar.is_none() {
        if let Ok(bar) = detector.detect_familiar_scrollbar() {
            scrollbar = Some(bar);
        } else {
            // TODO: recoverable?
            return swapping.stage(SwappingStage::Completed);
        }
    }

    let scrollbar = scrollbar.unwrap();
    let x = scrollbar.x + scrollbar.width / 2;
    let y = scrollbar.y + scrollbar.height / 2;

    update_with_timeout(
        timeout,
        5,
        |timeout| {
            let _ = context.keys.send_mouse(x, y, MouseAction::Scroll);
            swapping.stage_scrolling(timeout, Some(scrollbar))
        },
        || {
            let mut swapping = swapping;
            if let Ok(bar) = detector.detect_familiar_scrollbar()
                && (bar.y - scrollbar.y).abs() >= 10
            {
                swapping.cards = Array::new();
                return swapping.stage(SwappingStage::FindCards);
            }
            // TODO: recoverable?
            swapping.stage(SwappingStage::Completed)
        },
        |timeout| {
            if timeout.current == 3 {
                let _ = context.keys.send_mouse(x + 50, y, MouseAction::MoveOnly);
            }
            swapping.stage_scrolling(timeout, Some(scrollbar))
        },
    )
}

fn update_saving(
    context: &Context,
    swapping: FamiliarsSwapping,
    timeout: Timeout,
    mut save: Option<Rect>,
) -> FamiliarsSwapping {
    let detector = context.detector_unwrap();
    if save.is_none() {
        if let Ok(button) = detector.detect_familiar_save_button() {
            save = Some(button);
        } else {
            // TODO: recoverable?
            return swapping.stage(SwappingStage::Completed);
        }
    }

    let save = save.unwrap();
    let x = save.x + save.width / 2;
    let y = save.y + save.height / 2;

    update_with_timeout(
        timeout,
        10,
        |timeout| {
            let _ = context.keys.send_mouse(x, y, MouseAction::Click);
            swapping.stage_saving(timeout, Some(save))
        },
        || {
            if let Ok(button) = detector.detect_ok_button() {
                let x = button.x + button.width / 2;
                let y = button.y + button.height / 2;
                let _ = context.keys.send_mouse(x, y, MouseAction::Click);
            }
            swapping.stage(SwappingStage::Completed)
        },
        |timeout| swapping.stage_saving(timeout, Some(save)),
    )
}
