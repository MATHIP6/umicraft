// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mode 5: drag the cursor stack over several slots to split it.
//!
//! A drag is three phases sent as separate `Click Window` packets: start
//! (slot −999), one "add" per slot, end (slot −999). The button encodes the
//! phase in its two low bits and the drag mode in the next two
//! (MCP `Container.getDragEvent`, `extractDragMode`).

use umicraft_protocol::packets::play::ItemStack;

use super::super::space::ClickSpace;
use super::super::stack::{count, max_stack, same_item, set_count};
use super::{ClickMode, ClickRequest, OUTSIDE_SLOT};

const PHASE_START: i8 = 0;
const PHASE_ADD: i8 = 1;
const PHASE_END: i8 = 2;

/// The mouse button of a drag, which fixes how the stack is divided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragButton {
    /// Left button: the stack is split evenly.
    Left,
    /// Right button: one item per slot.
    Right,
    /// Middle button (creative): full stacks.
    Middle,
}

impl DragButton {
    fn mode(self) -> i8 {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Middle => 2,
        }
    }
}

fn encode(phase: i8, mode: i8) -> i8 {
    (phase & 3) | ((mode & 3) << 2)
}

/// The `Click Window` requests of a finished drag over `slots`.
#[must_use]
pub fn drag_sequence(slots: &[i16], button: DragButton) -> Vec<ClickRequest> {
    let mode = button.mode();
    let request = |slot, phase| ClickRequest {
        slot,
        button: encode(phase, mode),
        mode: ClickMode::Drag,
    };
    let mut requests = vec![request(OUTSIDE_SLOT, PHASE_START)];
    requests.extend(slots.iter().map(|&slot| request(slot, PHASE_ADD)));
    requests.push(request(OUTSIDE_SLOT, PHASE_END));
    requests
}

/// The drag in progress inside a window (`Container.dragEvent`, `dragMode`,
/// `dragSlots`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DragState {
    event: i8,
    mode: i8,
    slots: Vec<usize>,
}

impl DragState {
    /// Whether a drag has started and not ended.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.event != 0
    }

    /// Cancels the drag.
    pub fn reset(&mut self) {
        self.event = 0;
        self.slots.clear();
    }

    /// Advances the drag with one packet of mode 5.
    pub(super) fn step(
        &mut self,
        space: &mut ClickSpace<'_>,
        cursor: &mut Option<ItemStack>,
        slot: i16,
        button: i8,
        creative: bool,
    ) {
        let previous = self.event;
        self.event = button & 3;
        let out_of_order = (previous != 1 || self.event != 2) && previous != self.event;
        if out_of_order || cursor.is_none() {
            self.reset();
        } else if self.event == PHASE_START {
            self.mode = (button >> 2) & 3;
            if self.mode < 2 || creative {
                self.event = PHASE_ADD;
                self.slots.clear();
            } else {
                self.reset();
            }
        } else if self.event == PHASE_ADD {
            self.add_slot(space, cursor.as_ref(), slot);
        } else if self.event == PHASE_END {
            self.finish(space, cursor);
            self.reset();
        } else {
            self.reset();
        }
    }

    fn add_slot(&mut self, space: &ClickSpace<'_>, held: Option<&ItemStack>, slot: i16) {
        let (Some(held), Ok(index)) = (held, usize::try_from(slot)) else {
            return;
        };
        if can_add(space, index, held)
            && count(held) > self.slots.len() as i32
            && !self.slots.contains(&index)
        {
            self.slots.push(index);
        }
    }

    fn finish(&self, space: &mut ClickSpace<'_>, cursor: &mut Option<ItemStack>) {
        let Some(held) = cursor.clone() else { return };
        if self.slots.is_empty() {
            return;
        }
        let mut remaining = count(&held);
        for &index in &self.slots {
            if !can_add(space, index, &held) || count(&held) < self.slots.len() as i32 {
                continue;
            }
            let existing = space.get(index).map_or(0, count);
            let share = self.share(&held, existing);
            let limit = max_stack(&held).min(space.kind(index).limit(&held));
            let size = share.min(limit);
            remaining -= size - existing;
            let mut placed = held.clone();
            set_count(&mut placed, size);
            space.set(index, Some(placed));
        }
        *cursor = (remaining > 0).then(|| {
            let mut rest = held;
            set_count(&mut rest, remaining);
            rest
        });
    }

    /// `Container.computeStackSize`.
    fn share(&self, held: &ItemStack, existing: i32) -> i32 {
        let base = match self.mode {
            0 => count(held) / self.slots.len() as i32,
            1 => 1,
            _ => max_stack(held),
        };
        base + existing
    }
}

/// The slots of `slots` a drag of `held` can really fill, in order.
///
/// This is the filter vanilla applies while the pointer crosses slots
/// (`GuiContainer.mouseClickMove`): the slot must take the item and there
/// must be an item left for it (`stackSize > dragSplittingSlots.size()`).
pub(crate) fn drag_targets(space: &ClickSpace<'_>, held: &ItemStack, slots: &[i16]) -> Vec<i16> {
    let mut accepted: Vec<i16> = Vec::new();
    for &slot in slots {
        let fits = usize::try_from(slot).is_ok_and(|index| can_add(space, index, held));
        if fits && count(held) > accepted.len() as i32 && !accepted.contains(&slot) {
            accepted.push(slot);
        }
    }
    accepted
}

/// `canAddItemToSlot(.., true)` and `isItemValid`.
fn can_add(space: &ClickSpace<'_>, index: usize, held: &ItemStack) -> bool {
    if index >= space.len() || !space.kind(index).accepts(held) {
        return false;
    }
    match space.get(index) {
        None => true,
        Some(there) => same_item(there, held) && count(there) <= max_stack(held),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::kind::WindowKind;
    use super::super::super::space::fixtures::TestWindow;
    use super::super::super::stack::fixtures::stack;
    use super::super::slot_click;
    use super::*;

    fn run(
        w: &mut TestWindow,
        cursor: &mut Option<ItemStack>,
        slots: &[i16],
        button: DragButton,
        creative: bool,
    ) {
        let mut drag = DragState::default();
        for request in drag_sequence(slots, button) {
            slot_click(&mut w.space(), cursor, &mut drag, request, creative);
        }
        assert!(!drag.is_active());
    }

    #[test]
    fn the_sequence_encodes_phase_and_mode() {
        let sequence = drag_sequence(&[3, 4], DragButton::Right);
        assert_eq!(sequence.len(), 4);
        assert_eq!((sequence[0].slot, sequence[0].button), (OUTSIDE_SLOT, 4));
        assert_eq!((sequence[1].slot, sequence[1].button), (3, 5));
        assert_eq!((sequence[3].slot, sequence[3].button), (OUTSIDE_SLOT, 6));
    }

    #[test]
    fn a_left_drag_splits_the_stack_evenly() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        let mut cursor = Some(stack(1, 10));
        run(&mut w, &mut cursor, &[0, 1, 2], DragButton::Left, false);
        assert_eq!(w.own[0], Some(stack(1, 3)));
        assert_eq!(w.own[2], Some(stack(1, 3)));
        assert_eq!(cursor, Some(stack(1, 1)));
    }

    #[test]
    fn a_right_drag_places_one_per_slot() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        w.own[1] = Some(stack(1, 5));
        let mut cursor = Some(stack(1, 4));
        run(&mut w, &mut cursor, &[0, 1], DragButton::Right, false);
        assert_eq!(w.own[0], Some(stack(1, 1)));
        assert_eq!(w.own[1], Some(stack(1, 6)));
        assert_eq!(cursor, Some(stack(1, 2)));
    }

    #[test]
    fn a_middle_drag_needs_creative() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        let mut cursor = Some(stack(1, 5));
        run(&mut w, &mut cursor, &[0], DragButton::Middle, false);
        assert!(w.own[0].is_none());
        run(&mut w, &mut cursor, &[0], DragButton::Middle, true);
        assert_eq!(w.own[0], Some(stack(1, 64)));
    }

    #[test]
    fn slots_that_refuse_the_item_are_skipped() {
        let mut w = TestWindow::player();
        let mut cursor = Some(stack(1, 4));
        run(&mut w, &mut cursor, &[5, 10, 11], DragButton::Left, false);
        assert!(w.inv.slot(5).is_none());
        assert_eq!(w.inv.slot(10).map(count), Some(2));
        assert_eq!(w.inv.slot(11).map(count), Some(2));
    }

    #[test]
    fn targets_skip_refusing_slots_and_stop_when_the_stack_runs_out() {
        let mut w = TestWindow::player();
        w.inv.set_slot(11, Some(stack(2, 1)));
        let held = stack(1, 2);
        // Slot 5 is armour (refuses), 11 holds another item, 12 repeats.
        let targets = drag_targets(&w.space(), &held, &[5, 11, 10, 12, 10, 13]);
        assert_eq!(targets, vec![10, 12]);
    }

    #[test]
    fn a_drag_over_one_slot_acts_as_a_placement() {
        let mut w = TestWindow::player();
        let mut cursor = Some(stack(1, 4));
        run(&mut w, &mut cursor, &[10], DragButton::Left, false);
        assert_eq!(w.inv.slot(10), Some(&stack(1, 4)));
        assert!(cursor.is_none());
    }
}
