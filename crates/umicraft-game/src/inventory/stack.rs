// SPDX-License-Identifier: MIT OR Apache-2.0

//! Helpers on the wire [`ItemStack`] shared by the click logic.
//!
//! The model stores a slot as `Option<ItemStack>` and keeps the invariant
//! that a stored stack has a positive size; every helper that shrinks a stack
//! goes through [`normalize`] to restore it.

use umicraft_protocol::packets::play::ItemStack;

use crate::items;

/// The stack limit of an item the registry does not know.
const DEFAULT_MAX_STACK: i32 = 64;

/// `Item.getItemStackLimit` of the stack's item.
#[must_use]
pub fn max_stack(stack: &ItemStack) -> i32 {
    items::lookup(i32::from(stack.item_id))
        .map_or(DEFAULT_MAX_STACK, |def| i32::from(def.max_stack))
}

/// `ItemStack.isStackable`: more than one per stack and not a damaged tool.
#[must_use]
pub fn is_stackable(stack: &ItemStack) -> bool {
    let damageable = items::lookup(i32::from(stack.item_id)).is_some_and(|def| def.is_damageable());
    max_stack(stack) > 1 && (!damageable || stack.damage == 0)
}

/// Whether two stacks hold the same item: id, damage and NBT
/// (`isItemEqual` and `areItemStackTagsEqual`).
#[must_use]
pub fn same_item(a: &ItemStack, b: &ItemStack) -> bool {
    a.item_id == b.item_id && a.damage == b.damage && a.nbt == b.nbt
}

/// The size of a stack as a wide integer.
#[must_use]
pub fn count(stack: &ItemStack) -> i32 {
    i32::from(stack.stack_size)
}

/// A copy of `stack` with another size, saturated to the wire range.
#[must_use]
pub fn with_count(stack: &ItemStack, size: i32) -> ItemStack {
    ItemStack {
        stack_size: size.clamp(0, i32::from(i8::MAX)) as i8,
        ..stack.clone()
    }
}

/// Resizes the stack in place, saturated to the wire range.
pub fn set_count(stack: &mut ItemStack, size: i32) {
    stack.stack_size = size.clamp(0, i32::from(i8::MAX)) as i8;
}

/// Adds `delta` (possibly negative) to the size of the stack.
pub fn add_count(stack: &mut ItemStack, delta: i32) {
    let size = count(stack) + delta;
    set_count(stack, size);
}

/// Removes up to `amount` items and returns them (`ItemStack.splitStack`).
pub fn split(stack: &mut ItemStack, amount: i32) -> ItemStack {
    let taken = amount.clamp(0, count(stack));
    add_count(stack, -taken);
    with_count(stack, taken)
}

/// `None` for an empty stack, the stack otherwise.
#[must_use]
pub fn normalize(stack: Option<ItemStack>) -> Option<ItemStack> {
    stack.filter(|s| s.item_id >= 0 && s.stack_size > 0)
}

/// Converts a wire stack to the slot representation (`item_id == -1` is empty).
#[must_use]
pub fn from_wire(stack: ItemStack) -> Option<ItemStack> {
    normalize(Some(stack))
}

/// Converts a slot to the wire form used by `Click Window`.
#[must_use]
pub fn to_wire(stack: Option<&ItemStack>) -> ItemStack {
    stack.cloned().unwrap_or(ItemStack::EMPTY)
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// A stack of `count` items with the given id.
    pub fn stack(item_id: i16, count: i8) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: count,
            damage: 0,
            nbt: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::stack;
    use super::*;

    #[test]
    fn split_removes_up_to_the_available_amount() {
        let mut s = stack(1, 10);
        assert_eq!(split(&mut s, 4).stack_size, 4);
        assert_eq!(s.stack_size, 6);
        assert_eq!(split(&mut s, 99).stack_size, 6);
        assert_eq!(s.stack_size, 0);
    }

    #[test]
    fn same_item_compares_damage_and_nbt() {
        let a = stack(5, 1);
        let mut b = stack(5, 9);
        assert!(same_item(&a, &b));
        b.damage = 2;
        assert!(!same_item(&a, &b));
        b.damage = 0;
        b.nbt = Some(vec![1]);
        assert!(!same_item(&a, &b));
    }

    #[test]
    fn tools_with_damage_do_not_stack() {
        assert!(is_stackable(&stack(1, 1)));
        assert!(!is_stackable(&stack(276, 1)));
    }

    #[test]
    fn normalize_drops_empty_stacks() {
        assert_eq!(normalize(Some(stack(1, 0))), None);
        assert_eq!(normalize(Some(ItemStack::EMPTY)), None);
        assert!(normalize(Some(stack(1, 1))).is_some());
    }
}
