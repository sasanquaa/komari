use std::fmt::Display;

use log::{debug, info};
use opencv::core::{Point, Rect};

use super::{
    Player,
    timeout::{Lifecycle, Timeout, next_timeout_lifecycle},
};
use crate::{
    array::Array,
    bridge::{KeyKind, MouseKind},
    detect::{FamiliarLevel, FamiliarRank},
    ecs::Resources,
    models::{FamiliarRarity, SwappableFamiliars},
    player::{PlayerEntity, next_action},
};

/// Number of familiar slots available.
const FAMILIAR_SLOTS: usize = 3;

/// Internal state machine representing the current state of familiar swapping.
#[derive(Debug, Clone, Copy)]
enum State {
    /// Opening the familiar menu.
    OpenMenu(Timeout),
    /// Find the familiar slots.
    FindSlots,
    /// Check if slot is free or occupied to release the slot.
    FreeSlots(usize, bool),
    /// Try releasing a single slot.
    FreeSlot(Timeout, usize),
    /// Find swappable familiar cards.
    FindCards(Timeout),
    /// Swapping a card into an empty slot.
    Swapping(Timeout, usize),
    /// Scrolling the familiar cards list to find more cards.
    Scrolling(Timeout, Option<Rect>, u32),
    /// Saving the familiar setup.
    Saving(Timeout),
    Completing(Timeout, bool),
}

#[derive(Debug, Clone, Copy)]
struct FamiliarSlot {
    bbox: Rect,
    is_free: bool,
}

/// Struct for storing familiar swapping data.
#[derive(Debug, Clone, Copy)]
pub struct FamiliarsSwapping {
    /// Current state of the familiar swapping state machine.
    state: State,
    /// Detected familiar slots with free/occupied status.
    slots: Array<FamiliarSlot, 3>,
    /// Detected familiar cards.
    cards: Array<Rect, 64>,
    /// Indicates which familiar slots are allowed to be swapped.
    swappable_slots: SwappableFamiliars,
    /// Only familiars with these rarities will be considered for swapping.
    swappable_rarities: Array<FamiliarRarity, 2>,
    /// Mouse rest point for other operations.
    mouse_rest: Point,
    /// Whether swapping is successful.
    success: bool,
}

impl Display for FamiliarsSwapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.state {
            State::OpenMenu(_) => write!(f, "OpenMenu"),
            State::FindSlots => write!(f, "FindSlots"),
            State::FreeSlots(_, _) | State::FreeSlot(_, _) => {
                write!(f, "FreeSlots")
            }
            State::FindCards(_) => write!(f, "FindCards"),
            State::Swapping(_, _) => write!(f, "Swapping"),
            State::Scrolling(_, _, _) => write!(f, "Scrolling"),
            State::Saving(_) => write!(f, "Saving"),
            State::Completing(_, _) => write!(f, "Completing"),
        }
    }
}

impl FamiliarsSwapping {
    pub fn new(
        swappable_slots: SwappableFamiliars,
        swappable_rarities: Array<FamiliarRarity, 2>,
    ) -> Self {
        Self {
            state: State::OpenMenu(Timeout::default()),
            slots: Array::new(),
            cards: Array::new(),
            swappable_slots,
            swappable_rarities,
            mouse_rest: Point::new(50, 50),
            success: false,
        }
    }
}

/// Updates [`Player::FamiliarsSwapping`] contextual state.
///
/// Note: This state does not use any [`Task`], so all detections are blocking. But this should be
/// acceptable for this state.
pub fn update_familiars_swapping_state(resources: &Resources, player: &mut PlayerEntity) {
    let Player::FamiliarsSwapping(mut swapping) = player.state else {
        panic!("state is not familiars swapping")
    };
    let familiar_key = match player.context.config.familiar_key {
        Some(val) => val,
        None => {
            info!(target:"backend/player","aborted familiars swapping because familiar menu key is not set");
            player.context.clear_action_completed();
            player.state = Player::Idle;
            return;
        }
    };

    match swapping.state {
        State::OpenMenu(_) => update_open_menu(resources, &mut swapping, familiar_key),
        State::FindSlots => update_find_slots(resources, &mut swapping),
        State::FreeSlots(_, _) => update_free_slots(resources, &mut swapping),
        State::FreeSlot(_, _) => update_free_slot(resources, &mut swapping),
        State::FindCards(_) => update_find_cards(resources, &mut swapping),
        State::Swapping(_, _) => update_swapping(resources, &mut swapping),
        State::Scrolling(_, _, _) => update_scrolling(resources, &mut swapping),
        State::Saving(_) => update_saving(resources, &mut swapping),
        State::Completing(timeout, completed) => {
            update_completing(resources, &mut swapping, timeout, completed)
        }
    }

    let next = if matches!(swapping.state, State::Completing(_, true)) {
        Player::Idle
    } else {
        Player::FamiliarsSwapping(swapping)
    };

    match next_action(&player.context) {
        Some(_) => {
            let is_terminal = matches!(next, Player::Idle);
            if is_terminal {
                player.context.clear_action_completed();
                if swapping.success {
                    player.context.clear_familiars_swap_fail_count();
                } else {
                    player.context.track_familiars_swap_fail_count();
                }
            }

            player.state = next;
        }
        None => player.state = Player::Idle, // Force cancel if not from action
    }
}

fn update_open_menu(resources: &Resources, swapping: &mut FamiliarsSwapping, key: KeyKind) {
    let State::OpenMenu(timeout) = swapping.state else {
        panic!("familiars swapping state is not opening menu");
    };

    match next_timeout_lifecycle(timeout, 120) {
        Lifecycle::Started(timeout) => {
            resources.input.send_mouse(
                swapping.mouse_rest.x,
                swapping.mouse_rest.y,
                MouseKind::Move,
            );
            swapping.state = State::OpenMenu(timeout);
        }
        Lifecycle::Ended => {
            if resources.detector().detect_familiar_menu_opened() {
                swapping.state = State::FindSlots;
                return;
            }

            swapping.success = true;
            swapping.state = State::Completing(Timeout::default(), false);
        }
        Lifecycle::Updated(timeout) => {
            if timeout.current == 60 {
                resources.input.send_key(key);
            }

            swapping.state = State::OpenMenu(timeout);
        }
    }
}

fn update_find_slots(resources: &Resources, swapping: &mut FamiliarsSwapping) {
    // Detect familiar slots and whether each slot is free
    if swapping.slots.is_empty() {
        let vec = resources.detector().detect_familiar_slots();
        if vec.len() == FAMILIAR_SLOTS {
            for pair in vec {
                swapping.slots.push(FamiliarSlot {
                    bbox: pair.0,
                    is_free: pair.1,
                });
            }
        } else {
            debug!(target: "backend/player", "familiar slots is not 3 but was {}, aborting...", vec.len());
            debug!(target: "backend/player", "detected familiar slots were {vec:?}");
            // Weird spots with false positives
            swapping.success = true;
            swapping.state = State::Completing(Timeout::default(), false);
            return;
        }
    }

    swapping.state = State::FreeSlots(FAMILIAR_SLOTS - 1, false);
}

fn update_free_slots(resources: &Resources, swapping: &mut FamiliarsSwapping) {
    #[inline]
    fn find_cards_or_complete(resources: &Resources, swapping: &mut FamiliarsSwapping) {
        if swapping.slots.iter().any(|slot| slot.is_free) {
            if let Ok(bbox) = resources.detector().detect_familiar_level_button() {
                let (x, y) = bbox_click_point(bbox);
                resources.input.send_mouse(x, y, MouseKind::Click);
            } else {
                let rest = swapping.mouse_rest;
                resources.input.send_mouse(rest.x, rest.y, MouseKind::Move);
            }
            swapping.state = State::FindCards(Timeout::default());
            return;
        }

        // No slot is free so move to completing.
        swapping.success = true;
        swapping.state = State::Completing(Timeout::default(), false);
    }

    let State::FreeSlots(index, was_freeing) = swapping.state else {
        panic!("familiars swapping state is not freeing slots")
    };
    let is_free = swapping.slots[index].is_free;
    match (is_free, index) {
        (true, index) if index > 0 => swapping.state = State::FreeSlots(index - 1, false),
        (true, 0) => find_cards_or_complete(resources, swapping),
        (false, _) => {
            let can_free = match swapping.swappable_slots {
                SwappableFamiliars::All => true,
                SwappableFamiliars::Last => index == FAMILIAR_SLOTS - 1,
                SwappableFamiliars::SecondAndLast => {
                    index == FAMILIAR_SLOTS - 1 || index == FAMILIAR_SLOTS - 2
                }
            };
            if !can_free {
                return find_cards_or_complete(resources, swapping);
            }

            // Bail and retry as this could indicate the menu closed/overlap
            if was_freeing {
                swapping.slots = Array::new();
                swapping.state = State::OpenMenu(Timeout::default());
                return;
            }

            swapping.state = State::FreeSlot(Timeout::default(), index);
        }
        (true, _) => unreachable!(),
    }
}

fn update_free_slot(resources: &Resources, swapping: &mut FamiliarsSwapping) {
    const FAMILIAR_FREE_SLOTS_TIMEOUT: u32 = 10;
    const FAMILIAR_CHECK_FREE_TICK: u32 = FAMILIAR_FREE_SLOTS_TIMEOUT;
    const FAMILIAR_CHECK_LVL_5_TICK: u32 = 5;

    let State::FreeSlot(timeout, index) = swapping.state else {
        panic!("familiars swapping state is not freeing slot")
    };

    match next_timeout_lifecycle(timeout, FAMILIAR_FREE_SLOTS_TIMEOUT) {
        Lifecycle::Started(timeout) => {
            let bbox = swapping.slots[index].bbox;
            let x = bbox.x + bbox.width / 2;
            resources.input.send_mouse(x, bbox.y + 20, MouseKind::Move);
            swapping.state = State::FreeSlot(timeout, index);
        }
        Lifecycle::Ended => swapping.state = State::FreeSlots(index, true),
        Lifecycle::Updated(mut timeout) => {
            let bbox = swapping.slots[index].bbox;
            let (x, y) = bbox_click_point(bbox);
            let detector = resources.detector();

            match timeout.current {
                FAMILIAR_CHECK_LVL_5_TICK => {
                    match detector.detect_familiar_hover_level() {
                        Ok(FamiliarLevel::Level5) => {
                            // Double click to free
                            resources.input.send_mouse(x, y, MouseKind::Click);
                            resources.input.send_mouse(x, y, MouseKind::Click);
                            // Move mouse to rest position to check if it has been truely freed
                            resources.input.send_mouse(x, bbox.y - 20, MouseKind::Move);
                        }
                        Ok(FamiliarLevel::LevelOther) => {
                            // If current slot is already non-level-5, check next slot
                            if index > 0 {
                                swapping.state = State::FreeSlots(index - 1, false);
                                return;
                            }

                            // If there is no more slot to check and any of them is free,
                            // starts finding cards for swapping
                            if swapping.slots.iter().any(|slot| slot.is_free) {
                                resources.input.send_mouse(
                                    swapping.mouse_rest.x,
                                    swapping.mouse_rest.y,
                                    MouseKind::Move,
                                );
                                swapping.state = State::FindCards(Timeout::default());
                                return;
                            }

                            // All of the slots are occupied and non-level-5
                            swapping.success = true;
                            swapping.state = State::Completing(Timeout::default(), false);
                            return;
                        }
                        // Could mean UI being closed
                        Err(_) => swapping.state = State::FreeSlots(index, true),
                    }
                }
                FAMILIAR_CHECK_FREE_TICK => {
                    if detector.detect_familiar_slot_is_free(bbox) {
                        // If familiar is free, timeout and set flag
                        timeout.current = FAMILIAR_FREE_SLOTS_TIMEOUT;
                        swapping.slots[index].is_free = true;
                    } else {
                        // After double clicking, previous slots will move forward so this loop
                        // updates previous slot free status. But this else could also mean the menu
                        // is already closed, so the update here can be wrong. However, resetting
                        // the timeout below will account for this case because of familiar level
                        // detection.
                        for i in index + 1..FAMILIAR_SLOTS {
                            swapping.slots[i].is_free =
                                detector.detect_familiar_slot_is_free(swapping.slots[i].bbox);
                        }
                        timeout = Timeout::default()
                    }
                }
                _ => (),
            }

            swapping.state = State::FreeSlot(timeout, index);
        }
    }
}

fn update_find_cards(resources: &Resources, swapping: &mut FamiliarsSwapping) {
    let State::FindCards(timeout) = swapping.state else {
        panic!("familiars swapping state is not finding cards");
    };

    // Timeout for ensuring sorting takes effect
    match next_timeout_lifecycle(timeout, 5) {
        Lifecycle::Ended => {
            if swapping.cards.is_empty() {
                let vec = resources.detector().detect_familiar_cards();
                if vec.is_empty() {
                    swapping.state = State::Scrolling(Timeout::default(), None, 0);
                    return;
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

            swapping.state = if swapping.cards.is_empty() {
                State::Scrolling(Timeout::default(), None, 0)
            } else {
                State::Swapping(Timeout::default(), 0)
            };
        }
        Lifecycle::Started(timeout) | Lifecycle::Updated(timeout) => {
            swapping.state = State::FindCards(timeout);
        }
    }
}

fn update_swapping(resources: &Resources, swapping: &mut FamiliarsSwapping) {
    /// Checks only for the first fixed number of familiar cards.
    const MAX_CHECK_COUNT: usize = 10;
    const SWAPPING_TIMEOUT: u32 = 10;
    const SWAPPING_DETECT_LEVEL_TICK: u32 = 5;

    let State::Swapping(timeout, index) = swapping.state else {
        panic!("familiars swapping state is not swapping")
    };

    match next_timeout_lifecycle(timeout, SWAPPING_TIMEOUT) {
        Lifecycle::Started(timeout) => {
            let (x, y) = bbox_click_point(swapping.cards[index]);
            resources.input.send_mouse(x, y, MouseKind::Move);
            swapping.state = State::Swapping(timeout, index);
        }
        Lifecycle::Ended => {
            // Check free slot in timeout
            for i in 0..FAMILIAR_SLOTS {
                swapping.slots[i].is_free = resources
                    .detector()
                    .detect_familiar_slot_is_free(swapping.slots[i].bbox);
            }

            // Save if all slots are occupied. Could also mean UI is already closed.
            if swapping.slots.iter().all(|slot| !slot.is_free) {
                swapping.state = State::Saving(Timeout::default());
                return;
            }

            // At least one slot is free and there are more cards. Could mean double click
            // failed or familiar already level 5, advances either way.
            swapping.state = if index + 1 < swapping.cards.len().min(MAX_CHECK_COUNT) {
                State::Swapping(Timeout::default(), index + 1)
            } else {
                State::Completing(Timeout::default(), false)
            };
        }
        Lifecycle::Updated(timeout) => {
            if timeout.current == SWAPPING_DETECT_LEVEL_TICK {
                let rest = swapping.mouse_rest;

                match resources.detector().detect_familiar_hover_level() {
                    Ok(FamiliarLevel::Level5) => {
                        // Move to rest position and wait for timeout
                        resources.input.send_mouse(rest.x, rest.y, MouseKind::Move);
                    }
                    Ok(FamiliarLevel::LevelOther) => {
                        // Click to select and then move to rest point
                        let bbox = swapping.cards[index];
                        let (x, y) = bbox_click_point(bbox);
                        resources.input.send_mouse(x, y, MouseKind::Click);
                        resources.input.send_mouse(rest.x, rest.y, MouseKind::Move);
                    }
                    Err(_) => {
                        // Recoverable in an edge case where the mouse overlap with the level
                        if !resources.detector().detect_familiar_menu_opened() {
                            swapping.state = State::Completing(Timeout::default(), false);
                            return;
                        }
                    }
                }
            }

            swapping.state = State::Swapping(timeout, index);
        }
    }
}

#[inline]
fn update_scrolling(resources: &Resources, swapping: &mut FamiliarsSwapping) {
    const MAX_RETRY: u32 = 3;

    /// Timeout for scrolling familiar cards list.
    const SCROLLING_TIMEOUT: u32 = 10;

    /// Tick to move the mouse beside scrollbar at.
    const SCROLLING_REST_TICK: u32 = 5;

    /// Y distance difference indicating the scrollbar has scrolled.
    const SCROLLBAR_SCROLLED_THRESHOLD: i32 = 10;

    let State::Scrolling(timeout, scrollbar, retry_count) = swapping.state else {
        panic!("familiars swapping state is not scrolling")
    };

    match next_timeout_lifecycle(timeout, SCROLLING_TIMEOUT) {
        Lifecycle::Started(timeout) => {
            // TODO: recoverable?
            let scrollbar = match resources.detector().detect_familiar_scrollbar() {
                Ok(val) => val,
                Err(_) => {
                    swapping.state = State::Completing(Timeout::default(), false);
                    return;
                }
            };

            let (x, y) = bbox_click_point(scrollbar);
            resources.input.send_mouse(x, y, MouseKind::Scroll);
            swapping.state = State::Scrolling(timeout, Some(scrollbar), retry_count);
        }
        Lifecycle::Ended => {
            let current_scrollbar = match resources.detector().detect_familiar_scrollbar() {
                Ok(val) => val,
                Err(_) => {
                    swapping.state = State::Completing(Timeout::default(), false);
                    return;
                }
            };

            if (current_scrollbar.y - scrollbar.unwrap().y).abs() >= SCROLLBAR_SCROLLED_THRESHOLD {
                swapping.cards = Array::new();
                swapping.state = State::FindCards(Timeout::default());
                return;
            }

            // Try again because scrolling might have failed. This could also indicate
            // the list is empty.
            swapping.state = if retry_count < MAX_RETRY {
                State::Scrolling(Timeout::default(), Some(current_scrollbar), retry_count + 1)
            } else {
                State::Completing(Timeout::default(), false)
            };
        }
        Lifecycle::Updated(timeout) => {
            if timeout.current == SCROLLING_REST_TICK {
                let (x, y) = bbox_click_point(scrollbar.unwrap());
                resources.input.send_mouse(x + 70, y, MouseKind::Move);
            }

            swapping.state = State::Scrolling(timeout, scrollbar, retry_count);
        }
    }
}

#[inline]
fn update_saving(resources: &Resources, swapping: &mut FamiliarsSwapping) {
    let State::Saving(timeout) = swapping.state else {
        panic!("familiars swapping state is not saving")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) => {
            let button = match resources.detector().detect_familiar_save_button() {
                Ok(val) => val,
                Err(_) => {
                    swapping.state = State::Completing(Timeout::default(), false);
                    return;
                }
            };

            let (x, y) = bbox_click_point(button);
            resources.input.send_mouse(x, y, MouseKind::Click);
            swapping.state = State::Saving(timeout);
        }
        Lifecycle::Ended => {
            swapping.success = true;
            swapping.state = State::Completing(Timeout::default(), false);
        }
        Lifecycle::Updated(timeout) => swapping.state = State::Saving(timeout),
    }
}

#[inline]
fn update_completing(
    resources: &Resources,
    swapping: &mut FamiliarsSwapping,
    timeout: Timeout,
    completed: bool,
) {
    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) | Lifecycle::Updated(timeout) => {
            swapping.state = State::Completing(timeout, completed);
        }
        Lifecycle::Ended => {
            if resources.detector().detect_familiar_menu_opened() {
                resources.input.send_key(KeyKind::Esc);
            }
            swapping.state = State::Completing(Timeout::default(), true);
        }
    }
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

    use mockall::predicate::{eq, function};

    use super::*;
    use crate::{array::Array, bridge::MockInput, detect::MockDetector};

    #[test]
    fn update_free_slots_advance_index_if_already_free() {
        let resources = Resources::new(None, None);
        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.slots.push(FamiliarSlot {
            bbox,
            is_free: false,
        });
        swapping.slots.push(FamiliarSlot {
            bbox,
            is_free: true,
        }); // Index 1 already free
        swapping.state = State::FreeSlots(1, false);

        update_free_slots(&resources, &mut swapping);

        assert_matches!(swapping.state, State::FreeSlots(0, false));
    }

    #[test]
    fn update_free_slots_move_to_find_cards() {
        let bbox = Rect::new(10, 10, 10, 10);
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_level_button()
            .once()
            .returning(move || Ok(bbox));
        let mut keys = MockInput::default();
        keys.expect_send_mouse()
            .with(
                eq(15),
                eq(15),
                function(|action| matches!(action, MouseKind::Click)),
            )
            .once();
        let resources = Resources::new(Some(keys), Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox_default = Default::default();
        swapping.slots.push(FamiliarSlot {
            bbox: bbox_default,
            is_free: true,
        });
        swapping.state = State::FreeSlots(0, false);

        update_free_slots(&resources, &mut swapping);

        assert_matches!(swapping.state, State::FindCards(_));
    }

    #[test]
    fn update_free_slots_can_free() {
        let resources = Resources::new(None, None);
        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.slots.push(FamiliarSlot {
            bbox,
            is_free: false,
        });
        // Second slot not free but can free because of SwappableFamiliars::All
        swapping.slots.push(FamiliarSlot {
            bbox,
            is_free: false,
        });
        swapping.state = State::FreeSlots(1, false);

        update_free_slots(&resources, &mut swapping);

        assert_matches!(swapping.state, State::FreeSlot(_, 1));
    }

    #[test]
    fn update_free_slots_cannot_free() {
        let resources = Resources::new(None, None);
        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::Last, Array::new());
        let bbox = Default::default();
        swapping.slots.push(FamiliarSlot {
            bbox,
            is_free: false,
        });
        // Second slot not free but also cannot free because of SwappableFamiliars::Last
        swapping.slots.push(FamiliarSlot {
            bbox,
            is_free: false,
        });
        swapping.state = State::FreeSlots(1, false);

        update_free_slots(&resources, &mut swapping);

        // Completing because there is no free slot to swap
        assert_matches!(swapping.state, State::Completing(_, _));
    }

    #[test]
    fn update_free_slot_detect_level_5_and_click() {
        let mut keys = MockInput::default();
        // When Level5 is detected the code will: Move (on start), Click, Click, Move to rest (3 mouse calls)
        keys.expect_send_mouse().times(3);
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_hover_level()
            .once()
            .returning(|| Ok(FamiliarLevel::Level5));
        let resources = Resources::new(Some(keys), Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.slots.push(FamiliarSlot {
            bbox,
            is_free: false,
        });
        swapping.state = State::FreeSlot(
            Timeout {
                current: 4, // One tick before detection in your code (FAMILIAR_CHECK_LVL_5_TICK)
                started: true,
                ..Default::default()
            },
            0,
        );

        update_free_slot(&resources, &mut swapping);

        // Should still be in FreeSlot (updated)
        assert_matches!(swapping.state, State::FreeSlot(_, 0));
    }

    #[test]
    fn update_free_slot_detect_free_and_set_flag() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_slot_is_free()
            .once()
            .returning(|_| true);
        let resources = Resources::new(None, Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.slots.push(FamiliarSlot {
            bbox,
            is_free: false,
        });
        swapping.state = State::FreeSlot(
            Timeout {
                current: 9, // One tick before detection (FAMILIAR_CHECK_FREE_TICK)
                started: true,
                ..Default::default()
            },
            0,
        );

        update_free_slot(&resources, &mut swapping);

        // After setting the free flag the code resets timeout.current = FAMILIAR_FREE_SLOTS_TIMEOUT (10)
        assert!(swapping.slots[0].is_free);
        assert_matches!(
            swapping.state,
            State::FreeSlot(Timeout { current: 10, .. }, 0)
        );
    }

    #[test]
    fn update_swapping_detect_level_5_and_move_to_rest() {
        let mut keys = MockInput::default();
        // Move on lifecycle start to hover above card
        keys.expect_send_mouse().once();
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_hover_level()
            .once()
            .returning(|| Ok(FamiliarLevel::Level5));
        let resources = Resources::new(Some(keys), Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.cards.push(bbox);
        swapping.state = State::Swapping(
            Timeout {
                current: 4,
                started: true,
                ..Default::default()
            },
            0,
        );

        update_swapping(&resources, &mut swapping);

        // No explicit state assertion here — function should have processed the Level5 branch and remain swapping
        assert_matches!(swapping.state, State::Swapping(_, 0));
    }

    #[test]
    fn update_swapping_detect_level_other_double_click_and_move_to_rest() {
        let mut keys = MockInput::default();
        // Move (start), Click, Move to rest = total 2 send_mouse calls in Updated branch
        keys.expect_send_mouse().times(2);
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_hover_level()
            .once()
            .returning(|| Ok(FamiliarLevel::LevelOther));
        let resources = Resources::new(Some(keys), Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.cards.push(bbox);
        swapping.state = State::Swapping(
            Timeout {
                current: 4,
                started: true,
                ..Default::default()
            },
            0,
        );

        update_swapping(&resources, &mut swapping);

        assert_matches!(swapping.state, State::Swapping(_, 0));
    }

    #[test]
    fn update_swapping_timeout_advance_to_next_card_if_slot_and_card_available() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_slot_is_free()
            .times(FAMILIAR_SLOTS)
            .returning(|_| true);
        let resources = Resources::new(None, Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.cards.push(bbox);
        swapping.cards.push(bbox);
        for _ in 0..FAMILIAR_SLOTS {
            swapping.slots.push(FamiliarSlot {
                bbox,
                is_free: true,
            });
        }
        swapping.state = State::Swapping(
            Timeout {
                current: 10,
                started: true,
                ..Default::default()
            },
            0,
        );

        update_swapping(&resources, &mut swapping);

        assert_matches!(swapping.state, State::Swapping(_, 1));
    }

    #[test]
    fn update_swapping_timeout_completing_if_max_check_exceeded() {
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_slot_is_free()
            .times(FAMILIAR_SLOTS)
            .returning(|_| true);
        let resources = Resources::new(None, Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        let bbox = Default::default();
        swapping.cards.push(bbox);
        for _ in 0..FAMILIAR_SLOTS {
            swapping.slots.push(FamiliarSlot {
                bbox,
                is_free: true,
            });
        }
        swapping.state = State::Swapping(
            Timeout {
                current: 10,
                started: true,
                ..Default::default()
            },
            0,
        );

        update_swapping(&resources, &mut swapping);

        assert_matches!(swapping.state, State::Completing(_, false));
    }

    #[test]
    fn update_saving_detect_and_click_save_button() {
        let mut keys = MockInput::default();
        keys.expect_send_mouse().once();
        let mut detector = MockDetector::default();
        detector
            .expect_detect_familiar_save_button()
            .once()
            .returning(|| Ok(Default::default()));
        let resources = Resources::new(Some(keys), Some(detector));

        let mut swapping = FamiliarsSwapping::new(SwappableFamiliars::All, Array::new());
        swapping.state = State::Saving(Timeout::default());

        update_saving(&resources, &mut swapping);

        assert_matches!(swapping.state, State::Saving(_));
    }

    // TODO: more tests
}
