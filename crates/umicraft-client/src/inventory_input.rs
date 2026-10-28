// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mouse and keyboard handling of the inventory and container screens.
//!
//! [`InventoryInput`] is a pure state machine: the window feeds it pointer,
//! button and key events together with the current [`ScreenLayout`] and it
//! answers with the [`InventoryAction`]s to send. It ports the click rules of
//! MCP `GuiContainer` (`mouseClicked`, `mouseClickMove`, `mouseReleased`,
//! `keyTyped`, `checkHotbarKeys`):
//!
//! - with an empty cursor a click acts on the press; with an item on the
//!   cursor it waits for the release, because moving over further slots in
//!   between turns it into a drag split;
//! - a second left click on the same slot within 250 ms while the cursor
//!   holds an item collects matching items (mode 6) instead of clicking;
//! - number keys and Q act on the hovered slot.
//!
//! What the machine cannot know (whether a slot accepts the dragged item, how
//! many items are left to share) is decided by the game thread, see
//! [`crate::inventory_events`].

use std::time::{Duration, Instant};

use umicraft_game::inventory::click::{ClickRequest, DragButton, OUTSIDE_SLOT};
use umicraft_ui::ScreenLayout;

/// A pointer position in physical pixels.
pub type Point = (f32, f32);

/// Longest gap between two clicks that still make a double click
/// (`GuiContainer.mouseClicked`: `i - lastClickTime < 250L`).
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(250);

/// A mouse button the inventory reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// Primary button.
    Left,
    /// Secondary button.
    Right,
    /// Wheel button (creative clone).
    Middle,
}

impl Button {
    fn drag_button(self) -> DragButton {
        match self {
            Self::Left => DragButton::Left,
            Self::Right => DragButton::Right,
            Self::Middle => DragButton::Middle,
        }
    }
}

/// A key the inventory reacts to while a slot is hovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryKey {
    /// A digit key, 1 to 9: swap the slot with that hotbar slot.
    Number(u8),
    /// The drop key (Q); Ctrl drops the whole stack.
    Drop,
}

/// What the game knows and the machine needs with every event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    /// Whether the cursor holds an item right now.
    pub cursor_has_item: bool,
    /// Whether the player is in creative mode (unlocks the middle button).
    pub creative: bool,
}

/// What the player asked the inventory to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryAction {
    /// One `Click Window`.
    Click(ClickRequest),
    /// A finished drag split over `slots`, in the order they were crossed.
    Drag {
        /// The window slots the pointer crossed while the button was down.
        slots: Vec<i16>,
        /// The button that was held, which fixes how the stack is divided.
        button: DragButton,
        /// The click to send instead when none of the slots can take the
        /// item (vanilla only drags over slots that accept it, otherwise the
        /// release is a plain click).
        fallback: Option<ClickRequest>,
    },
}

/// The button press in progress (`GuiContainer` fields `dragSplitting`,
/// `ignoreMouseUp` and `doubleClick`).
#[derive(Debug)]
struct Press {
    button: Button,
    /// The slots crossed so far; `Some` when the press may become a drag.
    drag: Option<Vec<i16>>,
    /// The press already acted, so the release must not.
    ignore_release: bool,
    /// The press completes a double click.
    double: bool,
}

/// The previous click (`lastClickSlot`, `lastClickTime`, `lastClickButton`).
#[derive(Debug, Clone, Copy)]
struct LastClick {
    slot: Option<usize>,
    button: Button,
    at: Instant,
}

/// The state machine; see the [module documentation](self).
#[derive(Debug, Default)]
pub struct InventoryInput {
    pointer: Option<Point>,
    shift: bool,
    ctrl: bool,
    press: Option<Press>,
    last_click: Option<LastClick>,
}

impl InventoryInput {
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

    /// The drag split in progress: the slots crossed so far and the button,
    /// once at least two slots were crossed (vanilla then shows the preview).
    #[must_use]
    pub fn drag_in_progress(&self) -> Option<(&[i16], DragButton)> {
        let press = self.press.as_ref()?;
        let slots = press.drag.as_deref().filter(|slots| slots.len() > 1)?;
        Some((slots, press.button.drag_button()))
    }

    /// Forgets the press and the last click, for when the screen closes.
    pub fn reset(&mut self) {
        self.press = None;
        self.last_click = None;
    }

    /// The pointer moved; while a drag split is under way the slot below it
    /// joins the drag (`mouseClickMove`).
    pub fn pointer_moved(&mut self, layout: &ScreenLayout, point: Point, context: Context) {
        self.pointer = Some(point);
        if !context.cursor_has_item {
            return;
        }
        let Some(slots) = self.press.as_mut().and_then(|p| p.drag.as_mut()) else {
            return;
        };
        if let Some(slot) = layout.slot_at(point).and_then(slot_id) {
            if !slots.contains(&slot) {
                slots.push(slot);
            }
        }
    }

    /// A button went down at the last pointer position (`mouseClicked`).
    pub fn button_pressed(
        &mut self,
        layout: &ScreenLayout,
        button: Button,
        context: Context,
        now: Instant,
    ) -> Option<InventoryAction> {
        let point = self.pointer?;
        // MCP only clones in creative: the server ignores mode 3 otherwise.
        if button == Button::Middle && !context.creative {
            return None;
        }
        let slot = layout.slot_at(point);
        let double = self.last_click.is_some_and(|last| {
            last.slot == slot
                && last.button == button
                && now.duration_since(last.at) < DOUBLE_CLICK_WINDOW
        });
        self.last_click = Some(LastClick {
            slot,
            button,
            at: now,
        });
        let mut press = Press {
            button,
            drag: None,
            ignore_release: false,
            double,
        };
        let mut action = None;
        if let Some(target) = target_at(layout, point) {
            if context.cursor_has_item {
                press.drag = Some(Vec::new());
            } else {
                press.ignore_release = true;
                action = self.click_for(button, target).map(InventoryAction::Click);
            }
        }
        self.press = Some(press);
        action
    }

    /// A button went up at the last pointer position (`mouseReleased`).
    pub fn button_released(
        &mut self,
        layout: &ScreenLayout,
        button: Button,
        context: Context,
    ) -> Option<InventoryAction> {
        let point = self.pointer?;
        let press = self.press.take();
        if let Some(action) = self.double_click(layout, point, button, press.as_ref(), context) {
            return Some(action);
        }
        if let Some(press) = &press {
            // Another button ended the drag: cancel it (`dragSplittingButton`).
            if press.drag.is_some() && press.button != button {
                return None;
            }
            if press.ignore_release {
                return None;
            }
        }
        let plain = target_at(layout, point)
            .filter(|_| context.cursor_has_item)
            .and_then(|target| self.click_for(button, target));
        let slots = press.and_then(|p| p.drag).unwrap_or_default();
        if !context.cursor_has_item {
            self.last_click = None;
        }
        if slots.is_empty() {
            plain.map(InventoryAction::Click)
        } else {
            Some(InventoryAction::Drag {
                slots,
                button: button.drag_button(),
                fallback: plain,
            })
        }
    }

    /// A key went down while the screen is open (`keyTyped`).
    pub fn key_pressed(
        &mut self,
        layout: &ScreenLayout,
        key: InventoryKey,
        context: Context,
    ) -> Option<InventoryAction> {
        let slot = slot_id(layout.slot_at(self.pointer?)?)?;
        let request = match key {
            // `checkHotbarKeys` only swaps while the cursor is empty.
            InventoryKey::Number(n) if (1..=9).contains(&n) && !context.cursor_has_item => {
                ClickRequest::number_key(slot, n - 1)
            }
            InventoryKey::Drop => ClickRequest::drop_key(slot, self.ctrl),
            InventoryKey::Number(_) => return None,
        };
        Some(InventoryAction::Click(request))
    }

    /// The collect click of a double click, if the release completes one.
    ///
    /// Vanilla also shift-clicks every matching slot of the inventory when
    /// Shift is held; that needs the slot contents, so with Shift the release
    /// is handled as a normal one.
    fn double_click(
        &mut self,
        layout: &ScreenLayout,
        point: Point,
        button: Button,
        press: Option<&Press>,
        context: Context,
    ) -> Option<InventoryAction> {
        let completes = press.is_some_and(|p| p.double)
            && button == Button::Left
            && context.cursor_has_item
            && !self.shift;
        let slot = slot_id(layout.slot_at(point)?).filter(|_| completes)?;
        self.last_click = None;
        Some(InventoryAction::Click(ClickRequest::double(slot)))
    }

    /// The click a button makes on `target` (a slot or [`OUTSIDE_SLOT`]).
    fn click_for(&self, button: Button, target: i16) -> Option<ClickRequest> {
        let outside = target == OUTSIDE_SLOT;
        match button {
            Button::Middle => (!outside).then(|| ClickRequest::middle(target)),
            Button::Left | Button::Right if outside => {
                Some(ClickRequest::outside(button == Button::Right))
            }
            Button::Left | Button::Right if self.shift => Some(ClickRequest::shift(
                target,
                i8::from(button == Button::Right),
            )),
            Button::Left => Some(ClickRequest::left(target)),
            Button::Right => Some(ClickRequest::right(target)),
        }
    }
}

/// The wire slot id of a window slot index.
fn slot_id(index: usize) -> Option<i16> {
    i16::try_from(index).ok()
}

/// The slot under `point`, [`OUTSIDE_SLOT`] outside the background and `None`
/// on the background between slots.
fn target_at(layout: &ScreenLayout, point: Point) -> Option<i16> {
    if !layout.point_inside(point) {
        return Some(OUTSIDE_SLOT);
    }
    layout.slot_at(point).and_then(slot_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_ui::{Rect, ScreenKind};

    const EMPTY: Context = Context {
        cursor_has_item: false,
        creative: false,
    };
    const HOLDING: Context = Context {
        cursor_has_item: true,
        creative: false,
    };
    const CREATIVE: Context = Context {
        cursor_has_item: false,
        creative: true,
    };
    const OUTSIDE: Point = (1.0, 1.0);

    fn layout() -> ScreenLayout {
        ScreenLayout::new(Rect::new(0.0, 0.0, 854.0, 480.0), ScreenKind::Player)
    }

    fn centre(layout: &ScreenLayout, slot: usize) -> Point {
        let rect = layout
            .slots
            .iter()
            .find(|(index, _)| *index == slot)
            .map(|(_, rect)| *rect)
            .expect("the slot exists");
        (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
    }

    fn at(input: &mut InventoryInput, layout: &ScreenLayout, slot: usize, context: Context) {
        input.pointer_moved(layout, centre(layout, slot), context);
    }

    fn click(request: ClickRequest) -> Option<InventoryAction> {
        Some(InventoryAction::Click(request))
    }

    #[test]
    fn an_empty_cursor_clicks_on_the_press_only() {
        let (layout, now) = (layout(), Instant::now());
        let mut input = InventoryInput::new();
        at(&mut input, &layout, 10, EMPTY);
        assert_eq!(
            input.button_pressed(&layout, Button::Left, EMPTY, now),
            click(ClickRequest::left(10))
        );
        assert_eq!(input.button_released(&layout, Button::Left, EMPTY), None);
        assert_eq!(
            input.button_pressed(&layout, Button::Right, EMPTY, now + Duration::from_secs(1)),
            click(ClickRequest::right(10))
        );
    }

    #[test]
    fn a_held_item_clicks_on_the_release() {
        let (layout, now) = (layout(), Instant::now());
        let mut input = InventoryInput::new();
        at(&mut input, &layout, 10, HOLDING);
        assert_eq!(
            input.button_pressed(&layout, Button::Right, HOLDING, now),
            None
        );
        assert_eq!(
            input.button_released(&layout, Button::Right, HOLDING),
            click(ClickRequest::right(10))
        );
    }

    #[test]
    fn shift_turns_a_click_into_a_shift_click() {
        let (layout, now) = (layout(), Instant::now());
        let mut input = InventoryInput::new();
        input.set_modifiers(true, false);
        at(&mut input, &layout, 12, EMPTY);
        assert_eq!(
            input.button_pressed(&layout, Button::Right, EMPTY, now),
            click(ClickRequest::shift(12, 1))
        );
    }

    #[test]
    fn a_press_outside_drops_the_cursor_stack_on_release() {
        let (layout, now) = (layout(), Instant::now());
        let mut input = InventoryInput::new();
        input.pointer_moved(&layout, OUTSIDE, HOLDING);
        assert_eq!(
            input.button_pressed(&layout, Button::Right, HOLDING, now),
            None
        );
        assert_eq!(
            input.button_released(&layout, Button::Right, HOLDING),
            click(ClickRequest::outside(true))
        );
    }

    #[test]
    fn dragging_over_three_slots_makes_a_drag() {
        let (layout, now) = (layout(), Instant::now());
        let mut input = InventoryInput::new();
        at(&mut input, &layout, 10, HOLDING);
        input.button_pressed(&layout, Button::Left, HOLDING, now);
        for slot in [10, 11, 12, 11] {
            at(&mut input, &layout, slot, HOLDING);
        }
        assert_eq!(
            input.button_released(&layout, Button::Left, HOLDING),
            Some(InventoryAction::Drag {
                slots: vec![10, 11, 12],
                button: DragButton::Left,
                fallback: Some(ClickRequest::left(11)),
            })
        );
    }

    #[test]
    fn a_drag_over_no_further_slot_is_a_plain_click() {
        let (layout, now) = (layout(), Instant::now());
        let mut input = InventoryInput::new();
        at(&mut input, &layout, 10, HOLDING);
        input.button_pressed(&layout, Button::Left, HOLDING, now);
        assert_eq!(
            input.button_released(&layout, Button::Left, HOLDING),
            click(ClickRequest::left(10))
        );
    }

    #[test]
    fn another_button_cancels_the_drag() {
        let (layout, now) = (layout(), Instant::now());
        let mut input = InventoryInput::new();
        at(&mut input, &layout, 10, HOLDING);
        input.button_pressed(&layout, Button::Left, HOLDING, now);
        at(&mut input, &layout, 11, HOLDING);
        assert_eq!(input.button_released(&layout, Button::Right, HOLDING), None);
    }

    #[test]
    fn a_second_left_click_within_250_ms_is_a_double_click() {
        let (layout, now) = (layout(), Instant::now());
        let mut input = InventoryInput::new();
        at(&mut input, &layout, 10, EMPTY);
        input.button_pressed(&layout, Button::Left, EMPTY, now);
        input.button_released(&layout, Button::Left, EMPTY);
        // The first click picked an item up.
        at(&mut input, &layout, 10, HOLDING);
        let later = now + Duration::from_millis(200);
        assert_eq!(
            input.button_pressed(&layout, Button::Left, HOLDING, later),
            None
        );
        assert_eq!(
            input.button_released(&layout, Button::Left, HOLDING),
            click(ClickRequest::double(10))
        );
    }

    #[test]
    fn a_slow_second_click_is_a_normal_click() {
        let (layout, now) = (layout(), Instant::now());
        let mut input = InventoryInput::new();
        at(&mut input, &layout, 10, EMPTY);
        input.button_pressed(&layout, Button::Left, EMPTY, now);
        input.button_released(&layout, Button::Left, EMPTY);
        at(&mut input, &layout, 10, HOLDING);
        let later = now + Duration::from_millis(250);
        input.button_pressed(&layout, Button::Left, HOLDING, later);
        assert_eq!(
            input.button_released(&layout, Button::Left, HOLDING),
            click(ClickRequest::left(10))
        );
    }

    #[test]
    fn a_double_click_on_another_slot_is_a_normal_click() {
        let (layout, now) = (layout(), Instant::now());
        let mut input = InventoryInput::new();
        at(&mut input, &layout, 10, EMPTY);
        input.button_pressed(&layout, Button::Left, EMPTY, now);
        input.button_released(&layout, Button::Left, EMPTY);
        at(&mut input, &layout, 11, HOLDING);
        input.button_pressed(
            &layout,
            Button::Left,
            HOLDING,
            now + Duration::from_millis(50),
        );
        assert_eq!(
            input.button_released(&layout, Button::Left, HOLDING),
            click(ClickRequest::left(11))
        );
    }

    #[test]
    fn the_middle_button_clones_in_creative_only() {
        let (layout, now) = (layout(), Instant::now());
        let mut input = InventoryInput::new();
        at(&mut input, &layout, 10, EMPTY);
        assert_eq!(
            input.button_pressed(&layout, Button::Middle, EMPTY, now),
            None
        );
        assert_eq!(
            input.button_pressed(&layout, Button::Middle, CREATIVE, now),
            click(ClickRequest::middle(10))
        );
    }

    #[test]
    fn number_keys_swap_with_the_hotbar_while_the_cursor_is_empty() {
        let layout = layout();
        let mut input = InventoryInput::new();
        at(&mut input, &layout, 12, EMPTY);
        assert_eq!(
            input.key_pressed(&layout, InventoryKey::Number(3), EMPTY),
            click(ClickRequest::number_key(12, 2))
        );
        assert_eq!(
            input.key_pressed(&layout, InventoryKey::Number(3), HOLDING),
            None
        );
        assert_eq!(
            input.key_pressed(&layout, InventoryKey::Number(0), EMPTY),
            None
        );
    }

    #[test]
    fn q_drops_one_and_ctrl_q_the_whole_stack() {
        let layout = layout();
        let mut input = InventoryInput::new();
        at(&mut input, &layout, 12, EMPTY);
        assert_eq!(
            input.key_pressed(&layout, InventoryKey::Drop, EMPTY),
            click(ClickRequest::drop_key(12, false))
        );
        input.set_modifiers(false, true);
        assert_eq!(
            input.key_pressed(&layout, InventoryKey::Drop, EMPTY),
            click(ClickRequest::drop_key(12, true))
        );
    }

    #[test]
    fn keys_need_a_hovered_slot() {
        let layout = layout();
        let mut input = InventoryInput::new();
        input.pointer_moved(&layout, OUTSIDE, EMPTY);
        assert_eq!(input.key_pressed(&layout, InventoryKey::Drop, EMPTY), None);
    }
}
