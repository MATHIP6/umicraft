// SPDX-License-Identifier: MIT OR Apache-2.0

//! Local prediction of a window click (MCP `Container.slotClick`).
//!
//! [`slot_click`] applies one [`ClickRequest`] to a [`ClickSpace`] and the
//! cursor exactly as the vanilla client does before it sends `Click Window`;
//! the stack it returns is the "clicked item" field of that packet.
//!
//! Each click mode lives in its own file: [`normal`] (left/right, middle,
//! drop), [`swap`] (number keys), [`double`], [`drag`] and [`transfer`]
//! (shift-click).

mod double;
pub mod drag;
mod normal;
mod swap;
pub mod transfer;

use umicraft_protocol::packets::play::ItemStack;

use super::space::ClickSpace;
pub(super) use drag::drag_targets;
pub use drag::{drag_sequence, DragButton, DragState};

/// The slot id that means "outside the window".
pub const OUTSIDE_SLOT: i16 = -999;

/// The click mode of `Click Window` (MCP `GuiContainer.handleMouseClick`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum ClickMode {
    /// Left or right click.
    Normal = 0,
    /// Shift + left or right click.
    Shift = 1,
    /// A number key, the button being the hotbar index.
    NumberKey = 2,
    /// Middle click (creative only).
    Middle = 3,
    /// Drop key (Q), the button 1 meaning the whole stack.
    Drop = 4,
    /// A drag split; the button encodes the phase.
    Drag = 5,
    /// Double click: collect matching items.
    Double = 6,
}

/// One click, as the wire packet describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickRequest {
    /// The window slot, or [`OUTSIDE_SLOT`].
    pub slot: i16,
    /// The button; its meaning depends on the mode.
    pub button: i8,
    /// The mode.
    pub mode: ClickMode,
}

impl ClickRequest {
    /// A left click.
    #[must_use]
    pub fn left(slot: i16) -> Self {
        Self::normal(slot, 0)
    }

    /// A right click.
    #[must_use]
    pub fn right(slot: i16) -> Self {
        Self::normal(slot, 1)
    }

    /// A shift-click with the given mouse button.
    #[must_use]
    pub fn shift(slot: i16, button: i8) -> Self {
        Self {
            slot,
            button,
            mode: ClickMode::Shift,
        }
    }

    /// A number key press over a slot (`key` is 0–8).
    #[must_use]
    pub fn number_key(slot: i16, key: u8) -> Self {
        Self {
            slot,
            button: key as i8,
            mode: ClickMode::NumberKey,
        }
    }

    /// A middle click.
    #[must_use]
    pub fn middle(slot: i16) -> Self {
        Self {
            slot,
            button: 2,
            mode: ClickMode::Middle,
        }
    }

    /// The drop key over a slot; `whole_stack` is Ctrl + Q.
    #[must_use]
    pub fn drop_key(slot: i16, whole_stack: bool) -> Self {
        Self {
            slot,
            button: i8::from(whole_stack),
            mode: ClickMode::Drop,
        }
    }

    /// A double click on a slot.
    #[must_use]
    pub fn double(slot: i16) -> Self {
        Self {
            slot,
            button: 0,
            mode: ClickMode::Double,
        }
    }

    /// A click outside the window, dropping the cursor stack (`right` drops
    /// a single item).
    #[must_use]
    pub fn outside(right: bool) -> Self {
        Self::normal(OUTSIDE_SLOT, i8::from(right))
    }

    fn normal(slot: i16, button: i8) -> Self {
        Self {
            slot,
            button,
            mode: ClickMode::Normal,
        }
    }
}

/// Applies a click to the window and the cursor.
///
/// `creative` enables the middle-click clone and the creative drag mode.
/// Returns the "clicked item" of the packet: a copy of the stack the click
/// acted on, `None` for the modes that report nothing.
pub fn slot_click(
    space: &mut ClickSpace<'_>,
    cursor: &mut Option<ItemStack>,
    drag: &mut DragState,
    request: ClickRequest,
    creative: bool,
) -> Option<ItemStack> {
    let slot = usize::try_from(request.slot)
        .ok()
        .filter(|&s| s < space.len());
    let button = request.button;
    if request.mode == ClickMode::Drag {
        drag.step(space, cursor, request.slot, button, creative);
        return None;
    }
    if drag.is_active() {
        drag.reset();
        return None;
    }
    match request.mode {
        ClickMode::Normal | ClickMode::Shift if matches!(button, 0 | 1) => {
            if request.slot == OUTSIDE_SLOT {
                normal::drop_cursor(cursor, button);
                None
            } else if request.mode == ClickMode::Shift {
                transfer::shift_click(space, slot?)
            } else {
                normal::click(space, cursor, slot?, button)
            }
        }
        ClickMode::NumberKey if (0..9).contains(&button) => {
            swap::number_key(space, slot?, usize::try_from(button).ok()?);
            None
        }
        ClickMode::Middle if creative && cursor.is_none() => {
            normal::clone_stack(space, cursor, slot?);
            None
        }
        ClickMode::Drop if cursor.is_none() => {
            normal::drop_from_slot(space, slot?, button);
            None
        }
        ClickMode::Double => {
            double::collect(space, cursor, slot?, button);
            None
        }
        _ => None,
    }
}
