// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mouse and keyboard handling of the creative inventory.
//!
//! [`CreativeInput`] is a pure state machine like
//! [`InventoryInput`](crate::inventory_input::InventoryInput), but its output
//! is different: the creative window sends no `Click Window`, so the machine
//! answers with [`CreativeEvent`]s that the game thread feeds to
//! `CreativeScreen` (see [`crate::creative_events`]). It ports the input
//! rules of MCP `GuiContainerCreative` over `GuiContainer`:
//!
//! - a left press on a tab button is swallowed and the tab is selected when
//!   the button is *released* over it;
//! - a left press on the scrollbar track starts a handle drag (only when the
//!   tab scrolls), a release ends it;
//! - with an empty cursor a click acts on the press; with an item on the
//!   cursor it waits for the release, because crossing further slots in
//!   between turns it into a drag split;
//! - a second left click on the same slot within 250 ms while the cursor holds
//!   an item collects matching items;
//! - number keys and Q act on the hovered slot, except on the search tab where
//!   they type into the search box (T opens the search tab elsewhere, E closes
//!   the window elsewhere).
//!
//! All positions are scaled pixels relative to the window's top-left corner,
//! as [`CreativeLayout::local`](crate::creative_layout::CreativeLayout::local)
//! produces them.

use std::time::{Duration, Instant};

use umicraft_game::inventory::creative::scroll::{
    SCROLLBAR_HEIGHT, SCROLLBAR_WIDTH, SCROLLBAR_X, SCROLLBAR_Y,
};
use umicraft_game::inventory::creative::tabs::{WINDOW_HEIGHT, WINDOW_WIDTH};
use umicraft_game::inventory::creative::{slot_at, CreativeSlot, CreativeTab, MouseButton};

/// A position in scaled pixels relative to the window's top-left corner.
pub type Local = (i32, i32);

/// Longest gap between two clicks that still make a double click
/// (`GuiContainer.mouseClicked`: `i - lastClickTime < 250L`).
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(250);

/// What the machine needs to know about the window with every event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    /// The selected tab, which decides the slots that exist.
    pub tab: CreativeTab,
    /// Whether the cursor holds an item right now.
    pub cursor_has_item: bool,
    /// Whether the scrollbar is usable.
    pub needs_scrollbar: bool,
}

/// A press-and-release on a slot, or outside the window when `target` is
/// `None`; the argument list of `CreativeScreen::click`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotClick {
    /// The slot, `None` outside the window.
    pub target: Option<CreativeSlot>,
    /// The button.
    pub button: MouseButton,
    /// Whether Shift was held.
    pub shift: bool,
}

/// What the player asked the creative window to do; the game thread applies
/// it to its `CreativeScreen`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreativeEvent {
    /// The window opened: re-select the remembered tab.
    Open,
    /// A click.
    Click(SlotClick),
    /// A finished drag split.
    Drag {
        /// The button that was held.
        button: MouseButton,
        /// The slots the pointer crossed, in order.
        slots: Vec<CreativeSlot>,
        /// The click to make instead when fewer than two of the slots can
        /// take part (vanilla only splits over two or more slots).
        fallback: Option<SlotClick>,
    },
    /// A double click on a slot.
    DoubleClick(CreativeSlot),
    /// A mouse wheel step, positive when scrolling up.
    Wheel(i32),
    /// A left press on the scrollbar track.
    ScrollPress(Local),
    /// The pointer moved while the handle is dragged (window-relative `y`).
    ScrollDrag(i32),
    /// The left button went up after a handle drag.
    ScrollRelease,
    /// A tab button was released over.
    SelectTab(CreativeTab),
    /// The chat key outside the search tab.
    OpenSearch,
    /// A digit key; `typed` is the character to type instead on the search
    /// tab when the digit is not a shortcut.
    Number {
        /// The hovered slot.
        hovered: Option<CreativeSlot>,
        /// The hotbar slot, 0 to 8.
        key: usize,
        /// The character the key produces.
        typed: Option<char>,
    },
    /// The drop key.
    Drop {
        /// The hovered slot.
        hovered: Option<CreativeSlot>,
        /// Ctrl was held.
        whole: bool,
    },
    /// A character typed into the search box.
    Char(char),
    /// Backspace in the search box.
    Backspace,
}

/// The role of a key in the creative window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    /// Escape: closes the window.
    Escape,
    /// Backspace: edits the search text.
    Backspace,
    /// The inventory key (E): closes the window outside the search tab.
    Inventory,
    /// The chat key (T): opens the search tab.
    Chat,
    /// The drop key (Q).
    Drop,
    /// A digit key, 1 to 9.
    Digit(u8),
    /// Any other key; only its text matters.
    Other,
}

/// A key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// What the key is.
    pub kind: KeyKind,
    /// The character it types, if any.
    pub text: Option<char>,
}

/// What a key press leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Nothing happens.
    Ignored,
    /// The window closes.
    Close,
    /// An event for the game thread.
    Send(CreativeEvent),
}

/// The button press in progress (`GuiContainer` fields `dragSplitting`,
/// `ignoreMouseUp` and `doubleClick`).
#[derive(Debug)]
struct Press {
    button: MouseButton,
    /// The slots crossed so far; `Some` when the press may become a drag.
    drag: Option<Vec<CreativeSlot>>,
    /// The press already acted, so the release must not.
    ignore_release: bool,
    /// The press completes a double click.
    double: bool,
}

/// The previous click (`lastClickSlot`, `lastClickTime`, `lastClickButton`).
#[derive(Debug, Clone, Copy)]
struct LastClick {
    slot: Option<CreativeSlot>,
    button: MouseButton,
    at: Instant,
}

/// The state machine; see the [module documentation](self).
#[derive(Debug, Default)]
pub struct CreativeInput {
    pointer: Option<Local>,
    shift: bool,
    ctrl: bool,
    press: Option<Press>,
    last_click: Option<LastClick>,
    scrolling: bool,
}

impl CreativeInput {
    /// A machine with no pointer position and no modifier held.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the Shift and Ctrl state.
    pub fn set_modifiers(&mut self, shift: bool, ctrl: bool) {
        self.shift = shift;
        self.ctrl = ctrl;
    }

    /// Forgets the press, the last click and the scrollbar drag, for when the
    /// window closes.
    pub fn reset(&mut self) {
        self.press = None;
        self.last_click = None;
        self.scrolling = false;
    }

    /// The slot under the pointer.
    #[must_use]
    pub fn hovered(&self, tab: CreativeTab) -> Option<CreativeSlot> {
        let (x, y) = self.pointer?;
        slot_at(tab, x, y)
    }

    /// The pointer moved; the handle follows while it is dragged and the slot
    /// below the pointer joins a drag split (`mouseClickMove`).
    pub fn pointer_moved(&mut self, point: Local, context: Context) -> Option<CreativeEvent> {
        self.pointer = Some(point);
        if self.scrolling {
            return Some(CreativeEvent::ScrollDrag(point.1));
        }
        if !context.cursor_has_item {
            return None;
        }
        let slots = self.press.as_mut().and_then(|p| p.drag.as_mut())?;
        if let Some(slot) = slot_at(context.tab, point.0, point.1) {
            if !slots.contains(&slot) {
                slots.push(slot);
            }
        }
        None
    }

    /// A mouse wheel step (`steps` positive when scrolling up).
    #[must_use]
    pub fn wheel(&self, steps: i32, context: Context) -> Option<CreativeEvent> {
        (steps != 0 && context.needs_scrollbar).then(|| CreativeEvent::Wheel(steps.signum()))
    }

    /// A button went down at the last pointer position (`mouseClicked`).
    pub fn button_pressed(
        &mut self,
        button: MouseButton,
        context: Context,
        now: Instant,
    ) -> Option<CreativeEvent> {
        let point = self.pointer?;
        self.press = None;
        if button == MouseButton::Left {
            if CreativeTab::at(point.0, point.1).is_some() {
                // Swallowed: the tab is chosen on the release.
                return None;
            }
            if context.needs_scrollbar && on_track(point) {
                self.scrolling = true;
                return Some(CreativeEvent::ScrollPress(point));
            }
        }
        let target = target_at(point, context.tab)?;
        let double = self.is_double(target, button, now);
        self.last_click = Some(LastClick {
            slot: target,
            button,
            at: now,
        });
        let mut press = Press {
            button,
            drag: None,
            ignore_release: false,
            double,
        };
        let mut event = None;
        if context.cursor_has_item {
            press.drag = Some(Vec::new());
        } else {
            press.ignore_release = true;
            event = self.click_for(button, target).map(CreativeEvent::Click);
        }
        self.press = Some(press);
        event
    }

    /// A button went up at the last pointer position (`mouseReleased`).
    pub fn button_released(
        &mut self,
        button: MouseButton,
        context: Context,
    ) -> Option<CreativeEvent> {
        let point = self.pointer?;
        let press = self.press.take();
        if button == MouseButton::Left {
            if std::mem::take(&mut self.scrolling) {
                return Some(CreativeEvent::ScrollRelease);
            }
            if let Some(tab) = CreativeTab::at(point.0, point.1) {
                return Some(CreativeEvent::SelectTab(tab));
            }
        }
        let press = press?;
        if let Some(slot) = self.double_click_slot(&press, point, button, context) {
            self.last_click = None;
            return Some(CreativeEvent::DoubleClick(slot));
        }
        // Another button ended the drag: cancel it (`dragSplittingButton`).
        if (press.drag.is_some() && press.button != button) || press.ignore_release {
            return None;
        }
        let plain = target_at(point, context.tab)
            .filter(|_| context.cursor_has_item)
            .and_then(|target| self.click_for(button, target));
        if !context.cursor_has_item {
            self.last_click = None;
        }
        match press.drag.filter(|slots| !slots.is_empty()) {
            Some(slots) => Some(CreativeEvent::Drag {
                button,
                slots,
                fallback: plain,
            }),
            None => plain.map(CreativeEvent::Click),
        }
    }

    /// A key went down while the window is open (`keyTyped`).
    pub fn key_pressed(&mut self, key: Key, context: Context) -> KeyOutcome {
        match key.kind {
            KeyKind::Escape => return KeyOutcome::Close,
            KeyKind::Backspace => return KeyOutcome::Send(CreativeEvent::Backspace),
            _ => {}
        }
        let hovered = self.hovered(context.tab);
        let number = |digit: u8, typed| {
            usize::from(digit)
                .checked_sub(1)
                .map(|key| CreativeEvent::Number {
                    hovered,
                    key,
                    typed,
                })
        };
        let event = if context.tab == CreativeTab::Search {
            // The search box has the focus: every printable key types, and the
            // hotbar keys only work over a slot (`checkHotbarKeys` first).
            match key.kind {
                KeyKind::Digit(digit) => number(digit, key.text),
                _ => key.text.map(CreativeEvent::Char),
            }
        } else {
            match key.kind {
                KeyKind::Inventory => return KeyOutcome::Close,
                KeyKind::Chat => Some(CreativeEvent::OpenSearch),
                KeyKind::Digit(digit) => number(digit, None),
                KeyKind::Drop => Some(CreativeEvent::Drop {
                    hovered,
                    whole: self.ctrl,
                }),
                _ => None,
            }
        };
        event.map_or(KeyOutcome::Ignored, KeyOutcome::Send)
    }

    /// Whether a press on `target` completes a double click.
    fn is_double(&self, target: Option<CreativeSlot>, button: MouseButton, now: Instant) -> bool {
        self.last_click.is_some_and(|last| {
            last.slot == target
                && last.button == button
                && now.duration_since(last.at) < DOUBLE_CLICK_WINDOW
        })
    }

    /// The slot to collect onto, if the release completes a double click.
    ///
    /// Vanilla also shift-clicks every matching slot when Shift is held; that
    /// needs the slot contents, so with Shift the release is a normal one.
    fn double_click_slot(
        &self,
        press: &Press,
        point: Local,
        button: MouseButton,
        context: Context,
    ) -> Option<CreativeSlot> {
        let completes =
            press.double && button == MouseButton::Left && context.cursor_has_item && !self.shift;
        slot_at(context.tab, point.0, point.1).filter(|_| completes)
    }

    /// The click a button makes on `target`.
    fn click_for(&self, button: MouseButton, target: Option<CreativeSlot>) -> Option<SlotClick> {
        // Cloning needs a slot; the middle button outside does nothing.
        if button == MouseButton::Middle && target.is_none() {
            return None;
        }
        Some(SlotClick {
            target,
            button,
            shift: self.shift,
        })
    }
}

/// Whether a point lies on the scrollbar track.
fn on_track((x, y): Local) -> bool {
    (SCROLLBAR_X..SCROLLBAR_X + SCROLLBAR_WIDTH).contains(&x)
        && (SCROLLBAR_Y..SCROLLBAR_Y + SCROLLBAR_HEIGHT).contains(&y)
}

/// What a press at `point` aims at: `Some(Some(slot))` a slot, `Some(None)`
/// outside the window and `None` the background between slots.
fn target_at((x, y): Local, tab: CreativeTab) -> Option<Option<CreativeSlot>> {
    let inside = (0..WINDOW_WIDTH).contains(&x) && (0..WINDOW_HEIGHT).contains(&y);
    if inside {
        slot_at(tab, x, y).map(Some)
    } else {
        Some(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEMS: Context = Context {
        tab: CreativeTab::BuildingBlocks,
        cursor_has_item: false,
        needs_scrollbar: true,
    };
    const HOLDING: Context = Context {
        cursor_has_item: true,
        ..ITEMS
    };
    const SEARCH: Context = Context {
        tab: CreativeTab::Search,
        ..ITEMS
    };
    const LEFT: MouseButton = MouseButton::Left;

    fn centre(slot: CreativeSlot) -> Local {
        let (x, y) = slot.position().expect("the slot is on screen");
        (x + 8, y + 8)
    }

    fn click(target: Option<CreativeSlot>, button: MouseButton) -> Option<CreativeEvent> {
        Some(CreativeEvent::Click(SlotClick {
            target,
            button,
            shift: false,
        }))
    }

    fn key(kind: KeyKind, text: Option<char>) -> Key {
        Key { kind, text }
    }

    #[test]
    fn an_empty_cursor_clicks_on_the_press_only() {
        let mut input = CreativeInput::new();
        let cell = CreativeSlot::Grid(3);
        input.pointer_moved(centre(cell), ITEMS);
        let now = Instant::now();
        assert_eq!(
            input.button_pressed(LEFT, ITEMS, now),
            click(Some(cell), LEFT)
        );
        assert_eq!(input.button_released(LEFT, ITEMS), None);
    }

    #[test]
    fn a_held_item_clicks_on_the_release() {
        let mut input = CreativeInput::new();
        let hotbar = CreativeSlot::Hotbar(2);
        input.pointer_moved(centre(hotbar), HOLDING);
        assert_eq!(input.button_pressed(LEFT, HOLDING, Instant::now()), None);
        assert_eq!(
            input.button_released(LEFT, HOLDING),
            click(Some(hotbar), LEFT)
        );
    }

    #[test]
    fn crossing_slots_with_a_held_item_makes_a_drag() {
        let mut input = CreativeInput::new();
        let (a, b) = (CreativeSlot::Hotbar(0), CreativeSlot::Hotbar(1));
        input.pointer_moved(centre(a), HOLDING);
        input.button_pressed(MouseButton::Right, HOLDING, Instant::now());
        input.pointer_moved(centre(a), HOLDING);
        input.pointer_moved((-10, 60), HOLDING);
        input.pointer_moved(centre(b), HOLDING);
        assert_eq!(
            input.button_released(MouseButton::Right, HOLDING),
            Some(CreativeEvent::Drag {
                button: MouseButton::Right,
                slots: vec![a, b],
                fallback: Some(SlotClick {
                    target: Some(b),
                    button: MouseButton::Right,
                    shift: false,
                }),
            })
        );
    }

    #[test]
    fn the_second_left_click_within_the_window_collects() {
        let mut input = CreativeInput::new();
        let slot = CreativeSlot::Player(40);
        let context = Context {
            tab: CreativeTab::Inventory,
            ..HOLDING
        };
        input.pointer_moved(centre(slot), context);
        let t0 = Instant::now();
        input.button_pressed(LEFT, context, t0);
        input.button_released(LEFT, context);
        input.button_pressed(LEFT, context, t0 + Duration::from_millis(100));
        assert_eq!(
            input.button_released(LEFT, context),
            Some(CreativeEvent::DoubleClick(slot))
        );
    }

    #[test]
    fn a_slow_second_click_is_a_normal_click() {
        let mut input = CreativeInput::new();
        let slot = CreativeSlot::Hotbar(4);
        input.pointer_moved(centre(slot), HOLDING);
        let t0 = Instant::now();
        input.button_pressed(LEFT, HOLDING, t0);
        input.button_released(LEFT, HOLDING);
        input.button_pressed(LEFT, HOLDING, t0 + Duration::from_millis(300));
        assert_eq!(
            input.button_released(LEFT, HOLDING),
            click(Some(slot), LEFT)
        );
    }

    #[test]
    fn a_click_outside_the_window_targets_nothing_and_inside_the_gaps_is_ignored() {
        let mut input = CreativeInput::new();
        input.pointer_moved((-20, 50), ITEMS);
        assert_eq!(
            input.button_pressed(LEFT, ITEMS, Instant::now()),
            click(None, LEFT)
        );
        input.pointer_moved((4, 4), ITEMS);
        assert_eq!(input.button_pressed(LEFT, ITEMS, Instant::now()), None);
        assert_eq!(input.button_released(LEFT, ITEMS), None);
    }

    #[test]
    fn a_tab_is_selected_on_the_release_not_the_press() {
        let mut input = CreativeInput::new();
        input.pointer_moved((40, -10), ITEMS);
        assert_eq!(input.button_pressed(LEFT, ITEMS, Instant::now()), None);
        assert_eq!(
            input.button_released(LEFT, ITEMS),
            Some(CreativeEvent::SelectTab(CreativeTab::Decorations))
        );
    }

    #[test]
    fn the_scrollbar_handle_follows_the_pointer_until_release() {
        let mut input = CreativeInput::new();
        input.pointer_moved((180, 30), ITEMS);
        assert_eq!(
            input.button_pressed(LEFT, ITEMS, Instant::now()),
            Some(CreativeEvent::ScrollPress((180, 30)))
        );
        assert_eq!(
            input.pointer_moved((300, 90), ITEMS),
            Some(CreativeEvent::ScrollDrag(90))
        );
        assert_eq!(
            input.button_released(LEFT, ITEMS),
            Some(CreativeEvent::ScrollRelease)
        );
        assert_eq!(input.pointer_moved((300, 95), ITEMS), None);
    }

    #[test]
    fn a_tab_without_scrolling_ignores_the_track_and_the_wheel() {
        let still = Context {
            needs_scrollbar: false,
            ..ITEMS
        };
        let mut input = CreativeInput::new();
        input.pointer_moved((180, 30), still);
        assert_eq!(input.button_pressed(LEFT, still, Instant::now()), None);
        assert_eq!(input.wheel(3, still), None);
        assert_eq!(input.wheel(-3, ITEMS), Some(CreativeEvent::Wheel(-1)));
    }

    #[test]
    fn digits_and_q_act_on_the_hovered_slot_in_an_item_tab() {
        let mut input = CreativeInput::new();
        let cell = CreativeSlot::Grid(0);
        input.pointer_moved(centre(cell), ITEMS);
        input.set_modifiers(false, true);
        assert_eq!(
            input.key_pressed(key(KeyKind::Digit(3), Some('3')), ITEMS),
            KeyOutcome::Send(CreativeEvent::Number {
                hovered: Some(cell),
                key: 2,
                typed: None,
            })
        );
        assert_eq!(
            input.key_pressed(key(KeyKind::Drop, Some('q')), ITEMS),
            KeyOutcome::Send(CreativeEvent::Drop {
                hovered: Some(cell),
                whole: true,
            })
        );
    }

    #[test]
    fn e_closes_outside_the_search_tab_and_types_inside_it() {
        let mut input = CreativeInput::new();
        let e = key(KeyKind::Inventory, Some('e'));
        assert_eq!(input.key_pressed(e, ITEMS), KeyOutcome::Close);
        assert_eq!(
            input.key_pressed(e, SEARCH),
            KeyOutcome::Send(CreativeEvent::Char('e'))
        );
        assert_eq!(
            input.key_pressed(key(KeyKind::Escape, None), SEARCH),
            KeyOutcome::Close
        );
    }

    #[test]
    fn the_chat_key_opens_the_search_tab_and_q_types_there() {
        let mut input = CreativeInput::new();
        let t = key(KeyKind::Chat, Some('t'));
        assert_eq!(
            input.key_pressed(t, ITEMS),
            KeyOutcome::Send(CreativeEvent::OpenSearch)
        );
        assert_eq!(
            input.key_pressed(key(KeyKind::Drop, Some('q')), SEARCH),
            KeyOutcome::Send(CreativeEvent::Char('q'))
        );
        assert_eq!(
            input.key_pressed(key(KeyKind::Backspace, None), SEARCH),
            KeyOutcome::Send(CreativeEvent::Backspace)
        );
    }

    #[test]
    fn a_digit_on_the_search_tab_carries_its_character() {
        let mut input = CreativeInput::new();
        assert_eq!(
            input.key_pressed(key(KeyKind::Digit(5), Some('5')), SEARCH),
            KeyOutcome::Send(CreativeEvent::Number {
                hovered: None,
                key: 4,
                typed: Some('5'),
            })
        );
    }
}
