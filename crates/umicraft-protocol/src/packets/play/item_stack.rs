// SPDX-License-Identifier: MIT OR Apache-2.0

//! The 1.8 item stack (`PacketBuffer.readItemStackFromBuffer`).
//!
//! Layout (MCP `net/minecraft/network/PacketBuffer.java`,
//! `readItemStackFromBuffer` / `writeItemStackToBuffer`):
//!
//! | field | width | notes |
//! |---|---|---|
//! | item id | i16 | `-1` means "no item"; the rest of the fields are absent |
//! | stack size | i8 | number of items |
//! | damage / metadata | i16 | tool durability, dye colour, … |
//! | NBT | optional | `0x00` when the stack has no NBT, otherwise a plain (uncompressed) NBT compound |
//!
//! The NBT compound is captured verbatim: M3 needs the numeric item id, the
//! stack size and the damage value for the HUD, but the raw bytes have to
//! survive a decode/encode round trip so that a client answering
//! `Player Block Placement` sends back the very stack the server described.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::nbt::read_optional_nbt;
use crate::packets::play::wire::{read_i16, read_i8, write_i16};

/// An item stack as 1.8 puts it on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemStack {
    /// Numeric item id; `-1` marks an empty stack.
    pub item_id: i16,
    /// Number of items in the stack.
    pub stack_size: i8,
    /// Item metadata (the damage value for tools).
    pub damage: i16,
    /// Raw NBT compound bytes (tag type, name and payload), `None` when the
    /// stack carries no NBT.
    pub nbt: Option<Vec<u8>>,
}

impl ItemStack {
    /// The empty stack (item id `-1`), the wire form of "nothing".
    pub const EMPTY: Self = Self {
        item_id: -1,
        stack_size: 0,
        damage: 0,
        nbt: None,
    };

    /// Reads an item stack.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on truncation, [`ProtocolError::Invariant`] when
    /// the NBT payload is malformed.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let item_id = read_i16(reader)?;
        if item_id < 0 {
            // Vanilla writes -1 and stops there: no count, no damage, no NBT.
            return Ok(Self {
                item_id,
                stack_size: 0,
                damage: 0,
                nbt: None,
            });
        }
        let stack_size = read_i8(reader)?;
        let damage = read_i16(reader)?;
        let nbt = read_optional_nbt(reader)?;
        Ok(Self {
            item_id,
            stack_size,
            damage,
            nbt,
        })
    }

    /// Writes an item stack.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on write failure.
    pub fn encode(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_i16(writer, self.item_id)?;
        if self.item_id < 0 {
            return Ok(());
        }
        writer.write_all(&self.stack_size.to_be_bytes())?;
        write_i16(writer, self.damage)?;
        match &self.nbt {
            Some(bytes) => writer.write_all(bytes)?,
            None => writer.write_all(&[0x00])?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_stack_is_just_the_item_id() {
        let bytes = [0xFF, 0xFF];
        let stack = ItemStack::decode(&mut &bytes[..]).unwrap();
        assert_eq!(
            stack,
            ItemStack {
                item_id: -1,
                stack_size: 0,
                damage: 0,
                nbt: None,
            }
        );

        let mut buffer = Vec::new();
        stack.encode(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn a_plain_stack_round_trips() {
        // Item 1 (stone), 64 items, damage 0, no NBT.
        let bytes = [0x00, 0x01, 0x40, 0x00, 0x00, 0x00];
        let stack = ItemStack::decode(&mut &bytes[..]).unwrap();
        assert_eq!(stack.item_id, 1);
        assert_eq!(stack.stack_size, 64);
        assert_eq!(stack.damage, 0);
        assert_eq!(stack.nbt, None);

        let mut buffer = Vec::new();
        stack.encode(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn nbt_is_captured_verbatim() {
        // A compound named "" holding one short entry named "id" with value 1.
        let bytes = [
            0x00, 0x01, // item id 1
            0x01, // stack size 1
            0x00, 0x00, // damage 0
            0x0A, // TAG_Compound
            0x00, 0x00, // empty name
            0x02, // TAG_Short
            0x00, 0x02, // name length 2
            0x69, 0x64, // name "id"
            0x00, 0x01, // value 1
            0x00, // TAG_End
        ];
        let stack = ItemStack::decode(&mut &bytes[..]).unwrap();
        assert_eq!(stack.nbt.as_deref(), Some(&bytes[5..][..]));

        let mut buffer = Vec::new();
        stack.encode(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn a_truncated_stack_is_an_error_not_a_panic() {
        assert!(ItemStack::decode(&mut &[][..]).is_err());
        assert!(ItemStack::decode(&mut &[0x00, 0x01][..]).is_err());
        // A compound that never terminates.
        let endless = [0x00, 0x01, 0x01, 0x00, 0x00, 0x0A, 0x00, 0x00];
        assert!(ItemStack::decode(&mut &endless[..]).is_err());
    }
}
