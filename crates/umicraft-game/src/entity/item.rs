// SPDX-License-Identifier: MIT OR Apache-2.0

//! The item stack an entity carries: held, dropped or framed.

use std::sync::Arc;

use umicraft_protocol::packets::play::ItemStack;
/// An item stack as an entity carries it: held, dropped or framed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityItem {
    /// Numeric item id (always non-negative).
    pub item_id: i16,
    /// Item metadata (damage value, dye colour, ...).
    pub damage: i16,
    /// Number of items in the stack.
    pub count: u8,
    /// Raw NBT compound bytes, shared so snapshots stay cheap to clone.
    pub nbt: Option<Arc<[u8]>>,
}

impl EntityItem {
    /// Converts a wire stack; an empty stack (item id below zero) is `None`.
    ///
    /// A negative stack size is clamped to zero.
    #[must_use]
    pub fn from_stack(stack: &ItemStack) -> Option<Self> {
        (stack.item_id >= 0).then(|| Self {
            item_id: stack.item_id,
            damage: stack.damage,
            count: u8::try_from(stack.stack_size).unwrap_or(0),
            nbt: stack.nbt.as_deref().map(Arc::from),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_wire_stack_is_no_item() {
        assert_eq!(EntityItem::from_stack(&ItemStack::EMPTY), None);
    }

    #[test]
    fn a_wire_stack_converts_with_its_nbt() {
        let stack = ItemStack {
            item_id: 276,
            stack_size: 3,
            damage: 5,
            nbt: Some(vec![10, 0, 0, 0]),
        };
        let item = EntityItem::from_stack(&stack).expect("not empty");
        assert_eq!((item.item_id, item.damage, item.count), (276, 5, 3));
        assert_eq!(item.nbt.as_deref(), Some(&[10u8, 0, 0, 0][..]));
    }
}
