// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `Creative Inventory Action` a screen transition asks the client to send.

use umicraft_protocol::packets::play::serverbound::CreativeInventoryAction;
use umicraft_protocol::packets::play::ItemStack;

use crate::inventory::stack::to_wire;
use crate::inventory::PLAYER_SLOT_COUNT;

/// The slot value that makes the server spawn the stack as a dropped item
/// (`PlayerControllerMP.sendPacketDropItem`).
pub const DROP_SLOT: i16 = -1;

/// One `Creative Inventory Action` (MCP `C10PacketCreativeInventoryAction`).
///
/// The creative screen applies each change to the local
/// [`WindowManager`](crate::inventory::WindowManager) itself, so the caller
/// only has to send these packets in order.
#[derive(Debug, Clone, PartialEq)]
pub struct CreativeAction {
    /// The player-window slot to set (36 to 44 is the hotbar), or
    /// [`DROP_SLOT`] to drop `stack` into the world.
    pub slot: i16,
    /// The new content of the slot; [`ItemStack::EMPTY`] clears it.
    pub stack: ItemStack,
}

impl CreativeAction {
    /// Sets a player-window slot (`PlayerControllerMP.sendSlotPacket`).
    #[must_use]
    pub fn set_slot(slot: usize, stack: Option<&ItemStack>) -> Self {
        Self {
            slot: slot as i16,
            stack: to_wire(stack),
        }
    }

    /// Drops a stack into the world.
    #[must_use]
    pub fn drop_stack(stack: ItemStack) -> Self {
        Self {
            slot: DROP_SLOT,
            stack,
        }
    }

    /// Whether the action drops an item instead of setting a slot.
    #[must_use]
    pub fn is_drop(&self) -> bool {
        self.slot == DROP_SLOT
    }
}

impl From<CreativeAction> for CreativeInventoryAction {
    fn from(action: CreativeAction) -> Self {
        Self {
            slot: action.slot,
            item: action.stack,
        }
    }
}

/// The slots of the player window that differ between two snapshots, as
/// `set slot` actions; `always` is reported even when unchanged (vanilla
/// resends the clicked hotbar slot).
///
/// Vanilla gets the same list from `CreativeCrafting`, which forwards every
/// slot `Container.detectAndSendChanges` finds changed.
pub(super) fn slot_changes(
    before: &[Option<ItemStack>; PLAYER_SLOT_COUNT],
    after: &[Option<ItemStack>; PLAYER_SLOT_COUNT],
    always: Option<usize>,
) -> Vec<CreativeAction> {
    (0..PLAYER_SLOT_COUNT)
        .filter(|&slot| before[slot] != after[slot] || always == Some(slot))
        .map(|slot| CreativeAction::set_slot(slot, after[slot].as_ref()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::stack::fixtures::stack;

    fn empty() -> [Option<ItemStack>; PLAYER_SLOT_COUNT] {
        std::array::from_fn(|_| None)
    }

    #[test]
    fn only_changed_slots_are_reported_in_slot_order() {
        let before = empty();
        let mut after = empty();
        after[40] = Some(stack(1, 2));
        after[9] = Some(stack(3, 1));
        let actions = slot_changes(&before, &after, None);
        let slots: Vec<_> = actions.iter().map(|a| a.slot).collect();
        assert_eq!(slots, [9, 40]);
        assert_eq!(actions[1].stack, stack(1, 2));
    }

    #[test]
    fn the_forced_slot_is_reported_even_when_unchanged() {
        let state = empty();
        let actions = slot_changes(&state, &state, Some(37));
        assert_eq!(actions, vec![CreativeAction::set_slot(37, None)]);
        assert_eq!(actions[0].stack, ItemStack::EMPTY);
    }

    #[test]
    fn a_drop_uses_the_drop_slot_and_converts_to_the_packet() {
        let action = CreativeAction::drop_stack(stack(276, 1));
        assert!(action.is_drop());
        let packet = CreativeInventoryAction::from(action);
        assert_eq!((packet.slot, packet.item.item_id), (-1, 276));
    }
}
