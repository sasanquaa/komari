use std::fmt::Display;

use log::debug;
use opencv::core::{Point, Rect};
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

const MAX_RETRY: u32 = 4;

/// Internal state machine representing the current stage of familiar swapping.
#[derive(Debug, Clone, Copy)]
enum SwappingStage {
    /// Opening the familiar menu.
    OpenMenu(Timeout, u32),
    /// Clicking on the "Setup" tab in the familiar UI.
    OpenSetup(Timeout, u32),
    /// Find the familiar slots.
    FindSlots,
    /// Check if slot is free or occupied to release the slot.
    FreeSlots(usize, bool),
    /// Try releasing a single slot.
    FreeSlot(Timeout, usize),
    /// Find swappable familiar cards.
    FindCards,
    /// Swapping a card into an empty slot.
    Swapping(Timeout, usize),
    /// Scrolling the familiar cards list to find more cards.
    Scrolling(Timeout, Option<Rect>, u32),
    /// Saving the familiar setup.
    Saving(Timeout, u32),
    Completing(Timeout, bool),
}

/// Struct for storing familiar swapping data.
#[derive(Debug, Clone, Copy)]
pub struct FamiliarsSwapping {
    /// Current stage of the familiar swapping state machine.
    stage: SwappingStage,
    /// Detected familiar slots with free/occupied status.
    slots: Array<(Rect, bool), 3>,
    /// Detected familiar cards.
    cards: Array<Rect, 64>,
    /// Indicates which familiar slots are allowed to be swapped.
    swappable_slots: SwappableFamiliars,
    /// Only familiars with these rarities will be considered for swapping.
    swappable_rarities: Array<FamiliarRarity, 2>,
    /// Mouse rest point for other operations.
    mouse_rest: Point,
}

impl Display for FamiliarsSwapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.stage {
            SwappingStage::OpenMenu(_, _) => write!(f, "Opening"),
            SwappingStage::OpenSetup(_, _) => write!(f, "Opening Setup"),
            SwappingStage::FindSlots => write!(f, "Find Slots"),
            SwappingStage::FreeSlots(_, _) | SwappingStage::FreeSlot(_, _) => {
                write!(f, "Freeing Slots")
            }
            SwappingStage::FindCards => write!(f, "Finding Cards"),
            SwappingStage::Swapping(_, _) => write!(f, "Swapping"),
            SwappingStage::Scrolling(_, _, _) => write!(f, "Scrolling"),
            SwappingStage::Saving(_, _) => write!(f, "Saving"),
            SwappingStage::Completing(_, _) => write!(f, "Completing"),
        }
    }
}

impl FamiliarsSwapping {
    pub fn new(
        swappable_slots: SwappableFamiliars,
        swappable_rarities: Array<FamiliarRarity, 2>,
    ) -> Self {
        Self {
            stage: SwappingStage::OpenMenu(Timeout::default(), 0),
            slots: Array::new(),
            cards: Array::new(),
            swappable_slots,
            swappable_rarities,
            mouse_rest: Point::new(50, 50),
        }
    }
}

impl FamiliarsSwapping {
    #[inline]
    fn stage(self, stage: SwappingStage) -> FamiliarsSwapping {
        FamiliarsSwapping { stage, ..self }
    }

    #[inline]
    fn stage_open_menu(self, timeout: Timeout, retry_count: u32) -> FamiliarsSwapping {
        self.stage(SwappingStage::OpenMenu(timeout, retry_count))
    }

    #[inline]
    fn stage_open_setup(self, timeout: Timeout, retry_count: u32) -> FamiliarsSwapping {
        self.stage(SwappingStage::OpenSetup(timeout, retry_count))
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
    fn stage_scrolling(
        self,
        timeout: Timeout,
        scrollbar: Option<Rect>,
        scrolled_count: u32,
    ) -> FamiliarsSwapping {
        self.stage(SwappingStage::Scrolling(timeout, scrollbar, scrolled_count))
    }

    #[inline]
    fn stage_saving(self, timeout: Timeout, retry_count: u32) -> FamiliarsSwapping {
        self.stage(SwappingStage::Saving(timeout, retry_count))
    }

    #[inline]
    fn stage_completing(self, timeout: Timeout, completed: bool) -> FamiliarsSwapping {
        self.stage(SwappingStage::Completing(timeout, completed))
    }
}

/// Updates [`Player::FamiliarsSwapping`] contextual state.
///
/// Note: This state does not use any [`Task`], so all detections are blocking. But this should be
/// acceptable for this state.
pub fn update_familiars_swapping_context(
    context: &Context,
    state: &mut PlayerState,
    swapping: FamiliarsSwapping,
) -> Player {
    let swapping = if swapping.swappable_rarities.is_empty() {
        swapping.stage_completing(Timeout::default(), true)
    } else {
        match swapping.stage {
            SwappingStage::OpenMenu(timeout, retry_count) => update_open_menu(
                context,
                state.config.familiar_key,
                swapping,
                timeout,
                retry_count,
            ),
            SwappingStage::OpenSetup(timeout, retry_count) => {
                open_setup(context, swapping, timeout, retry_count)
            }
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
            SwappingStage::Scrolling(timeout, scrollbar, retry_count) => {
                update_scrolling(context, swapping, timeout, scrollbar, retry_count)
            }
            SwappingStage::Saving(timeout, retry_count) => {
                update_saving(context, swapping, timeout, retry_count)
            }
            SwappingStage::Completing(timeout, completed) => {
                update_completing(context, swapping, timeout, completed)
            }
        }
    };
    let next = if matches!(swapping.stage, SwappingStage::Completing(_, true)) {
        Player::Idle
    } else {
        Player::FamiliarsSwapping(swapping)
    };

    on_action(
        state,
        |_| Some((next, matches!(next, Player::Idle))),
        || Player::Idle, // Force cancel if it is not initiated from an action
    )
}

fn update_open_menu(
    context: &Context,
    key: KeyKind,
    swapping: FamiliarsSwapping,
    timeout: Timeout,
    retry_count: u32,
) -> FamiliarsSwapping {
    update_with_timeout(
        timeout,
        10,
        |timeout| {
            let rest = swapping.mouse_rest;
            let _ = context.keys.send_mouse(rest.x, rest.y, MouseAction::Move);
            if context.detector_unwrap().detect_familiar_menu_opened() {
                swapping.stage_open_setup(Timeout::default(), 0)
            } else if retry_count + 1 < MAX_RETRY {
                let _ = context.keys.send(key);
                swapping.stage_open_menu(timeout, retry_count + 1)
            } else {
                swapping.stage_completing(Timeout::default(), false)
            }
        },
        || swapping.stage_open_menu(Timeout::default(), retry_count),
        |timeout| swapping.stage_open_menu(timeout, retry_count),
    )
}

fn open_setup(
    context: &Context,
    swapping: FamiliarsSwapping,
    timeout: Timeout,
    retry_count: u32,
) -> FamiliarsSwapping {
    const OPEN_SETUP_TIMEOUT: u32 = 10;

    update_with_timeout(
        timeout,
        OPEN_SETUP_TIMEOUT,
        |timeout| {
            let mut swapping = swapping;

            // Try click familiar menu setup button every one second until it becomes
            // undetectable
            if let Ok(bbox) = context.detector_unwrap().detect_familiar_setup_button() {
                let (x, y) = bbox_click_point(bbox);
                let _ = context.keys.send_mouse(x, y, MouseAction::Click);
                swapping.mouse_rest = Point::new(bbox.x, bbox.y - 100);
            }

            swapping.stage_open_setup(timeout, retry_count)
        },
        || {
            if context
                .detector_unwrap()
                .detect_familiar_setup_button()
                .is_ok()
            {
                if retry_count + 1 < MAX_RETRY {
                    swapping.stage_open_setup(Timeout::default(), retry_count + 1)
                } else {
                    swapping.stage_completing(Timeout::default(), false)
                }
            } else {
                // This could also indicate familiar menu already closed. If that is the case,
                // find slots will handle it. And send to mouse rest position for detecting slots.
                let rest = swapping.mouse_rest;
                let _ = context.keys.send_mouse(rest.x, rest.y, MouseAction::Move);
                swapping.stage(SwappingStage::FindSlots)
            }
        },
        |timeout| swapping.stage_open_setup(timeout, retry_count),
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
            debug!(target: "player", "familiar slots is not 3, aborting...");
            // Weird spots with false positives
            return swapping.stage_completing(Timeout::default(), false);
        }
    }

    if swapping.slots.is_empty() {
        // Still empty, bail and retry as this could indicate the menu closed/overlap
        swapping.stage_open_menu(Timeout::default(), 0)
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
    #[inline]
    fn find_cards_or_complete(context: &Context, swapping: FamiliarsSwapping) -> FamiliarsSwapping {
        if swapping.slots.iter().any(|slot| slot.1) {
            let rest = swapping.mouse_rest;
            let _ = context.keys.send_mouse(rest.x, rest.y, MouseAction::Move);
            swapping.stage(SwappingStage::FindCards)
        } else {
            swapping.stage_completing(Timeout::default(), false)
        }
    }

    let (_, is_free) = swapping.slots[index];
    match (is_free, index) {
        (true, index) if index > 0 => swapping.stage_free_slots(index - 1, false),
        (true, 0) => find_cards_or_complete(context, swapping),
        (false, _) => {
            let can_free = match swapping.swappable_slots {
                SwappableFamiliars::All => true,
                SwappableFamiliars::Last => index == FAMILIAR_SLOTS - 1,
                SwappableFamiliars::SecondAndLast => {
                    index == FAMILIAR_SLOTS - 1 || index == FAMILIAR_SLOTS - 2
                }
            };
            if !can_free {
                return find_cards_or_complete(context, swapping);
            }

            if was_freeing {
                // Bail and retry as this could indicate the menu closed/overlap
                FamiliarsSwapping {
                    slots: Array::new(),
                    ..swapping.stage_open_menu(Timeout::default(), 0)
                }
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
    const FAMILIAR_CHECK_FREE_TICK: u32 = FAMILIAR_FREE_SLOTS_TIMEOUT;
    const FAMILIAR_CHECK_LVL_5_TICK: u32 = 5;

    update_with_timeout(
        timeout,
        FAMILIAR_FREE_SLOTS_TIMEOUT,
        |timeout| {
            // On start, move mouse to hover over the familiar slot to check level
            let bbox = swapping.slots[index].0;
            let x = bbox.x + bbox.width / 2;
            let _ = context.keys.send_mouse(x, bbox.y + 20, MouseAction::Move);
            swapping.stage_free_slot(timeout, index)
        },
        || swapping.stage_free_slots(index, true),
        |mut timeout| {
            let mut swapping = swapping;
            let bbox = swapping.slots[index].0;
            let (x, y) = bbox_click_point(bbox);
            let detector = context.detector_unwrap();

            match timeout.current {
                FAMILIAR_CHECK_LVL_5_TICK => {
                    match detector.detect_familiar_hover_level() {
                        Ok(FamiliarLevel::Level5) => {
                            // Double click to free
                            let _ = context.keys.send_mouse(x, y, MouseAction::Click);
                            let _ = context.keys.send_mouse(x, y, MouseAction::Click);
                            // Move mouse to rest position to check if it has been truely freed
                            let _ = context.keys.send_mouse(x, bbox.y - 20, MouseAction::Move);
                        }
                        Ok(FamiliarLevel::LevelOther) => {
                            return if index > 0 {
                                // If current slot is already non-level-5, check next slot
                                swapping.stage_free_slots(index - 1, false)
                            } else if swapping.slots.iter().any(|slot| slot.1) {
                                // If there is no more slot to check and any of them is free,
                                // starts finding cards for swapping
                                let rest = swapping.mouse_rest;
                                let _ = context.keys.send_mouse(rest.x, rest.y, MouseAction::Move);
                                swapping.stage(SwappingStage::FindCards)
                            } else {
                                // All of the slots are occupied and non-level-5
                                swapping.stage_completing(Timeout::default(), false)
                            };
                        }
                        // Could mean UI being closed
                        Err(_) => return swapping.stage_free_slots(index, true),
                    }
                }
                FAMILIAR_CHECK_FREE_TICK => {
                    if detector.detect_familiar_slot_is_free(bbox) {
                        // If familiar is free, timeout and set flag
                        timeout.current = FAMILIAR_FREE_SLOTS_TIMEOUT;
                        swapping.slots[index].1 = true;
                    } else {
                        // After double clicking, previous slots will move forward so this loop
                        // updates previous slot free status. But this else could also mean the menu
                        // is already closed, so the update here can be wrong. However, resetting
                        // the timeout below will account for this case because of familiar level
                        // detection.
                        for i in index + 1..FAMILIAR_SLOTS {
                            swapping.slots[i].1 =
                                detector.detect_familiar_slot_is_free(swapping.slots[i].0);
                        }
                        timeout = Timeout::default()
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
            return swapping.stage_scrolling(Timeout::default(), None, 0);
        }
        for pair in vec {
            let rarity = match pair.1 {
                FamiliarRank::Rare => FamiliarRarity::Rare,
                FamiliarRank::Epic => FamiliarRarity::Epic,
            };
            if swapping.swappable_rarities.iter().any(|r| *r == rarity) {
                swapping.cards.push(pair.0);
            }
        }
    }

    if swapping.cards.is_empty() {
        // Try scroll
        swapping.stage_scrolling(Timeout::default(), None, 0)
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
    const SWAPPING_TIMEOUT: u32 = 10;
    const SWAPPING_DETECT_LEVEL_TICK: u32 = 5;

    update_with_timeout(
        timeout,
        SWAPPING_TIMEOUT,
        |timeout| {
            let (x, y) = bbox_click_point(swapping.cards[index]);
            let _ = context.keys.send_mouse(x, y, MouseAction::Move);
            swapping.stage_swapping(timeout, index)
        },
        || {
            // Check free slot in timeout
            let mut swapping = swapping;
            for i in 0..FAMILIAR_SLOTS {
                swapping.slots[i].1 = context
                    .detector_unwrap()
                    .detect_familiar_slot_is_free(swapping.slots[i].0);
            }

            if swapping.slots.iter().all(|slot| !slot.1) {
                // Save if all slots are occupied. Could also mean UI is already closed.
                swapping.stage(SwappingStage::Saving(Timeout::default(), 0))
            } else if index + 1 < swapping.cards.len() {
                // At least one slot is free and there are more cards. Could mean double click
                // failed or familiar already level 5, advances either way.
                swapping.stage_swapping(Timeout::default(), index + 1)
            } else {
                // Try scroll for more cards
                let rest = swapping.mouse_rest;
                let _ = context.keys.send_mouse(rest.x, rest.y, MouseAction::Move);
                swapping.stage_scrolling(Timeout::default(), None, 0)
            }
        },
        |timeout| {
            let rest = swapping.mouse_rest;

            if timeout.current == SWAPPING_DETECT_LEVEL_TICK {
                match context.detector_unwrap().detect_familiar_hover_level() {
                    Ok(FamiliarLevel::Level5) => {
                        // Move to rest position and wait for timeout
                        let _ = context.keys.send_mouse(rest.x, rest.y, MouseAction::Move);
                    }
                    Ok(FamiliarLevel::LevelOther) => {
                        // Double click to select and then move to rest point
                        let bbox = swapping.cards[index];
                        let (x, y) = bbox_click_point(bbox);
                        let _ = context.keys.send_mouse(x, y, MouseAction::Click);
                        let _ = context.keys.send_mouse(x, y, MouseAction::Click);
                        let _ = context.keys.send_mouse(rest.x, rest.y, MouseAction::Move);
                    }
                    Err(_) => {
                        // Recoverable in an edge case where the mouse overlap with the level
                        if !context.detector_unwrap().detect_familiar_menu_opened() {
                            return swapping.stage_completing(Timeout::default(), false);
                        }
                    }
                }
            }

            swapping.stage_swapping(timeout, index)
        },
    )
}

#[inline]
fn update_scrolling(
    context: &Context,
    swapping: FamiliarsSwapping,
    timeout: Timeout,
    scrollbar: Option<Rect>,
    retry_count: u32,
) -> FamiliarsSwapping {
    /// Timeout for scrolling familiar cards list.
    const SCROLLING_TIMEOUT: u32 = 10;

    /// Tick to move the mouse beside scrollbar at.
    const SCROLLING_REST_TICK: u32 = 5;

    /// Y distance difference indicating the scrollbar has scrolled.
    const SCROLLBAR_SCROLLED_THRESHOLD: i32 = 10;

    update_with_timeout(
        timeout,
        SCROLLING_TIMEOUT,
        |timeout| {
            let Ok(scrollbar) = context.detector_unwrap().detect_familiar_scrollbar() else {
                // TODO: recoverable?
                return swapping.stage_completing(Timeout::default(), false);
            };

            let (x, y) = bbox_click_point(scrollbar);
            let _ = context.keys.send_mouse(x, y, MouseAction::Scroll);

            swapping.stage_scrolling(timeout, Some(scrollbar), retry_count)
        },
        || {
            if let Ok(bar) = context.detector_unwrap().detect_familiar_scrollbar() {
                if (bar.y - scrollbar.unwrap().y).abs() >= SCROLLBAR_SCROLLED_THRESHOLD {
                    return FamiliarsSwapping {
                        cards: Array::new(), // Reset cards array
                        ..swapping.stage(SwappingStage::FindCards)
                    };
                } else if retry_count + 1 < MAX_RETRY {
                    // Try again because scrolling might have failed. This could also indicate
                    // the list is empty.
                    return swapping.stage_scrolling(
                        Timeout::default(),
                        Some(bar),
                        retry_count + 1,
                    );
                }
            }

            swapping.stage_completing(Timeout::default(), false)
        },
        |timeout| {
            if timeout.current == SCROLLING_REST_TICK {
                let (x, y) = bbox_click_point(scrollbar.unwrap());
                let _ = context.keys.send_mouse(x + 70, y, MouseAction::Move);
            }

            swapping.stage_scrolling(timeout, scrollbar, retry_count)
        },
    )
}

#[inline]
fn update_saving(
    context: &Context,
    swapping: FamiliarsSwapping,
    timeout: Timeout,
    retry_count: u32,
) -> FamiliarsSwapping {
    /// Timeout for saving familiars setup.
    const SAVING_TIMEOUT: u32 = 30;
    const PRESS_OK_AT: u32 = 15;

    update_with_timeout(
        timeout,
        SAVING_TIMEOUT,
        |timeout| {
            let Ok(button) = context.detector_unwrap().detect_familiar_save_button() else {
                // TODO: recoverable?
                return swapping.stage_completing(Timeout::default(), false);
            };

            let (x, y) = bbox_click_point(button);
            let _ = context.keys.send_mouse(x, y, MouseAction::Click);

            swapping.stage_saving(timeout, retry_count)
        },
        || {
            if context.detector_unwrap().detect_familiar_menu_opened()
                && retry_count + 1 < MAX_RETRY
            {
                swapping.stage_saving(Timeout::default(), retry_count + 1)
            } else {
                swapping.stage_completing(Timeout::default(), false)
            }
        },
        |timeout| {
            if timeout.current == PRESS_OK_AT
                && let Ok(button) = context.detector_unwrap().detect_esc_confirm_button()
            {
                let (x, y) = bbox_click_point(button);
                let _ = context.keys.send_mouse(x, y, MouseAction::Click);
                let _ = context.keys.send_mouse(
                    swapping.mouse_rest.x,
                    swapping.mouse_rest.y,
                    MouseAction::Move,
                );
                let _ = context.keys.send(KeyKind::Esc);
            }
            swapping.stage_saving(timeout, retry_count)
        },
    )
}

#[inline]
fn update_completing(
    context: &Context,
    swapping: FamiliarsSwapping,
    timeout: Timeout,
    completed: bool,
) -> FamiliarsSwapping {
    update_with_timeout(
        timeout,
        10,
        |timeout| {
            let has_menu = context.detector_unwrap().detect_familiar_menu_opened();
            if has_menu {
                let _ = context.keys.send(KeyKind::Esc);
            }
            swapping.stage_completing(timeout, !has_menu)
        },
        || swapping.stage_completing(Timeout::default(), completed),
        |timeout| swapping.stage_completing(timeout, completed),
    )
}

#[inline]
fn bbox_click_point(bbox: Rect) -> (i32, i32) {
    let x = bbox.x + bbox.width / 2;
    let y = bbox.y + bbox.height / 2;
    (x, y)
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use anyhow::Ok;

    use super::*;
    use crate::{array::Array, bridge::MockKeySender, detect::MockDetector};

    #[test]
    fn update_free_slots_advance_index_if_already_free() {
        let context = Context::new(None, None);
        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.slots.push((bbox, false));
        swapping.slots.push((bbox, true)); // Index 1 already free

        let result = update_free_slots(&context, swapping, 1, false);
        assert_matches!(result.stage, SwappingStage::FreeSlots(0, false));
    }

    #[test]
    fn update_free_slots_move_to_find_cards() {
        let mut keys = MockKeySender::default();
        keys.expect_send_mouse().once().returning(|_, _, _| Ok(()));
        let context = Context::new(Some(keys), None);

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.slots.push((bbox, true));

        let result = update_free_slots(&context, swapping, 0, false);
        // At least 1 slot is free and index is 0 so move to FindCards
        assert_matches!(result.stage, SwappingStage::FindCards);
    }

    #[test]
    fn update_free_slots_can_free() {
        let context = Context::new(None, None);
        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.slots.push((bbox, false));
        // Second slot not free but can free because of SwappableFamiliars::All
        swapping.slots.push((bbox, false));

        let result = update_free_slots(&context, swapping, 1, false);
        assert_matches!(result.stage, SwappingStage::FreeSlot(_, 1));
    }

    #[test]
    fn update_free_slots_cannot_free() {
        let context = Context::new(None, None);
        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::Last, Array::new());
        let bbox = Default::default();
        swapping.slots.push((bbox, false));
        // Second slot not free but also cannot free because of SwappableFamiliars::Last
        swapping.slots.push((bbox, false));

        let result = update_free_slots(&context, swapping, 1, false);
        // Completing because there is no free slot to swap
        assert_matches!(result.stage, SwappingStage::Completing(_, _));
    }

    #[test]
    fn update_free_slot_detect_level_5_and_click() {
        let mut keys = MockKeySender::default();
        keys.expect_send_mouse()
            .times(3)
            .returning(|_, _, _| Ok(()));
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_hover_level()
            .once()
            .returning(|| Ok(FamiliarLevel::Level5));
        let context = Context::new(Some(keys), Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.slots.push((bbox, false));

        let timeout = Timeout {
            current: 4, // One tick before detection
            started: true,
            ..Default::default()
        };
        let result = update_free_slot(&context, swapping, timeout, 0);
        assert_matches!(result.stage, SwappingStage::FreeSlot(_, 0));
    }

    #[test]
    fn update_free_slot_detect_free_and_set_flag() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_slot_is_free()
            .once()
            .returning(|_| true);
        let context = Context::new(None, Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.slots.push((bbox, false));

        let timeout = Timeout {
            current: 9, // One tick before detection
            started: true,
            ..Default::default()
        };
        let result = update_free_slot(&context, swapping, timeout, 0);
        assert!(result.slots[0].1);
        assert_matches!(
            result.stage,
            SwappingStage::FreeSlot(Timeout { current: 10, .. }, 0)
        );
    }

    #[test]
    fn update_swapping_detect_level_5_and_move_to_rest() {
        let mut keys = MockKeySender::default();
        keys.expect_send_mouse().once().returning(|_, _, _| Ok(()));
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_hover_level()
            .once()
            .returning(|| Ok(FamiliarLevel::Level5));
        let context = Context::new(Some(keys), Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.cards.push(bbox);

        let timeout = Timeout {
            current: 4,
            started: true,
            ..Default::default()
        };
        update_swapping(&context, swapping, timeout, 0);
    }

    #[test]
    fn update_swapping_detect_level_other_double_click_and_move_to_rest() {
        let mut keys = MockKeySender::default();
        keys.expect_send_mouse()
            .times(3)
            .returning(|_, _, _| Ok(()));
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_hover_level()
            .once()
            .returning(|| Ok(FamiliarLevel::LevelOther));
        let context = Context::new(Some(keys), Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.cards.push(bbox);

        let timeout = Timeout {
            current: 4,
            started: true,
            ..Default::default()
        };
        update_swapping(&context, swapping, timeout, 0);
    }

    #[test]
    fn update_swapping_timeout_advance_to_next_card_if_slot_and_card_available() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_slot_is_free()
            .times(FAMILIAR_SLOTS)
            .returning(|_| true);
        let context = Context::new(None, Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.cards.push(bbox);
        swapping.cards.push(bbox);
        for _ in 0..FAMILIAR_SLOTS {
            swapping.slots.push((bbox, true));
        }

        let timeout = Timeout {
            current: 10,
            started: true,
            ..Default::default()
        };

        let result = update_swapping(&context, swapping, timeout, 0);
        assert_matches!(result.stage, SwappingStage::Swapping(_, 1));
    }

    #[test]
    fn update_swapping_timeout_advance_to_scroll_if_slot_available_and_card_unavailable() {
        let mut keys = MockKeySender::default();
        keys.expect_send_mouse().once().returning(|_, _, _| Ok(()));
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_slot_is_free()
            .times(FAMILIAR_SLOTS)
            .returning(|_| true);
        let context = Context::new(Some(keys), Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.cards.push(bbox);
        for _ in 0..FAMILIAR_SLOTS {
            swapping.slots.push((bbox, true));
        }

        let timeout = Timeout {
            current: 10,
            started: true,
            ..Default::default()
        };

        let result = update_swapping(&context, swapping, timeout, 0);
        assert_matches!(result.stage, SwappingStage::Scrolling(_, None, 0));
    }

    #[test]
    fn update_saving_detect_and_click_save_button() {
        let mut keys = MockKeySender::default();
        keys.expect_send_mouse().once().returning(|_, _, _| Ok(()));

        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_save_button()
            .once()
            .returning(|| Ok(Default::default()));

        let context = Context::new(Some(keys), Some(detector));
        let swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());

        let timeout = Timeout::default();
        let result = update_saving(&context, swapping, timeout, 0);

        assert_matches!(result.stage, SwappingStage::Saving(_, 0));
    }

    #[test]
    fn update_saving_press_ok_button() {
        let mut keys = MockKeySender::default();
        keys.expect_send_mouse()
            .times(2) // one for OK button click, one for mouse move
            .returning(|_, _, _| Ok(()));

        let mut detector = MockDetector::default();
        detector
            .expect_detect_esc_confirm_button()
            .once()
            .returning(|| Ok(Default::default()));

        let context = Context::new(Some(keys), Some(detector));
        let swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());

        let timeout = Timeout {
            current: 14, // PRESS_OK_AT
            started: true,
            ..Default::default()
        };

        let result = update_saving(&context, swapping, timeout, 0);
        assert_matches!(result.stage, SwappingStage::Saving(_, 0));
    }

    // TODO: more tests
}
