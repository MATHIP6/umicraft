// SPDX-License-Identifier: MIT OR Apache-2.0

//! Live preview of a drag split (MCP `GuiContainer.drawScreen`).
//!
//! While the buttons is held over several slots vanilla already shows the
//! stacks the release will produce. The preview replays the drag on a throw-
//! away copy of the window, so it can never disagree with the real click.

use umicraft_protocol::packets::play::ItemStack;

use super::click::{drag_sequence, DragButton};
use super::kind::{resolve_container_slot, SlotTarget};
use super::manager::WindowManager;
use super::stack::to_wire;
use super::view::WindowView;

/// What the window will look like if the drag ends now.
#[derive(Debug, Clone, PartialEq)]
pub struct DragPreview {
    /// The window slots that receive items, with their future stack.
    pub slots: Vec<(usize, ItemStack)>,
    /// What stays on the cursor.
    pub cursor: Option<ItemStack>,
}

/// Previews a drag over `slots`, `None` when fewer than two slots accept the
/// cursor stack (the release is then a plain click).
#[must_use]
pub fn drag_preview(
    view: &WindowView,
    slots: &[i16],
    button: DragButton,
    creative: bool,
) -> Option<DragPreview> {
    let mut window = WindowManager::from_view(view);
    let targets = window.drag_targets(slots);
    if targets.len() < 2 {
        return None;
    }
    for request in drag_sequence(&targets, button) {
        window.click(request, creative);
    }
    let slots = targets
        .iter()
        .filter_map(|&slot| {
            let index = usize::try_from(slot).ok()?;
            Some((index, window.window_slot(index)?.clone()))
        })
        .collect();
    Some(DragPreview {
        slots,
        cursor: window.cursor().cloned(),
    })
}

impl WindowManager {
    /// Rebuilds a window from a render snapshot.
    #[must_use]
    pub fn from_view(view: &WindowView) -> Self {
        let mut window = Self::new();
        for (index, item) in view.player_slots.iter().enumerate() {
            window.apply_set_slot(0, index as i16, to_wire(item.as_ref()));
        }
        if let Some(container) = &view.container {
            window.open_window(
                container.window_id,
                container.kind,
                container.title.clone(),
                container.slots.len() as u8,
            );
            for (index, item) in container.slots.iter().enumerate() {
                window.apply_set_slot(
                    container.window_id as i8,
                    index as i16,
                    to_wire(item.as_ref()),
                );
            }
        }
        window.apply_set_slot(-1, -1, to_wire(view.cursor.as_ref()));
        window
    }

    /// The stack in a slot of the displayed window (the player window when no
    /// container is open).
    #[must_use]
    pub fn window_slot(&self, index: usize) -> Option<&ItemStack> {
        match self.container() {
            None => self.inventory().slot(index),
            Some(container) => {
                match resolve_container_slot(index, container.container_slots.len())? {
                    SlotTarget::Container(i) => container.container_slots.get(i)?.as_ref(),
                    SlotTarget::Player(i) => self.inventory().slot(i),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::kind::{WindowKind, PLAYER_SLOT_COUNT};
    use super::super::stack::fixtures::stack;
    use super::*;

    fn view_with_cursor(cursor: ItemStack) -> WindowView {
        WindowView {
            player_slots: std::array::from_fn::<_, PLAYER_SLOT_COUNT, _>(|_| None),
            container: None,
            cursor: Some(cursor),
            selected_slot: 0,
        }
    }

    #[test]
    fn a_right_drag_previews_one_item_per_slot() {
        let view = view_with_cursor(stack(1, 5));
        let preview = drag_preview(&view, &[10, 11, 12], DragButton::Right, false).unwrap();
        assert_eq!(preview.slots.len(), 3);
        assert!(preview.slots.iter().all(|(_, s)| s.stack_size == 1));
        assert_eq!(preview.cursor, Some(stack(1, 2)));
    }

    #[test]
    fn a_left_drag_previews_an_even_split() {
        let view = view_with_cursor(stack(1, 10));
        let preview = drag_preview(&view, &[10, 11, 12], DragButton::Left, false).unwrap();
        assert!(preview.slots.iter().all(|(_, s)| s.stack_size == 3));
        assert_eq!(preview.cursor, Some(stack(1, 1)));
    }

    #[test]
    fn fewer_than_two_slots_is_no_preview() {
        let view = view_with_cursor(stack(1, 10));
        assert!(drag_preview(&view, &[10], DragButton::Left, false).is_none());
    }

    #[test]
    fn container_slots_are_previewed_by_window_index() {
        let mut window = WindowManager::new();
        window.open_window(2, WindowKind::Chest, String::new(), 27);
        window.apply_set_slot(-1, -1, stack(1, 4));
        let view = window.view();
        let preview = drag_preview(&view, &[0, 1], DragButton::Left, false).unwrap();
        assert_eq!(preview.slots, vec![(0, stack(1, 2)), (1, stack(1, 2))]);
        assert!(preview.cursor.is_none());
    }
}
