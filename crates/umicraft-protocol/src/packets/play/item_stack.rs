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
use crate::packets::codec::{read_i32, read_u8};
use crate::packets::play::wire::{read_i16, read_i8, skip_bytes, write_i16, RecordingReader};

/// Maximum nesting depth accepted while skipping an NBT compound.
///
/// Vanilla allows 512; item stacks never nest deeper than a handful of
/// levels, and the limit keeps a hostile server from exhausting the stack.
const MAX_NBT_DEPTH: u32 = 32;

/// Maximum number of bytes a single NBT length prefix may claim.
///
/// The protocol caps a whole packet at 2 MiB, so anything larger is bogus.
const MAX_NBT_BYTES: usize = 2 * 1024 * 1024;

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
        let nbt = read_nbt(reader)?;
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

/// Reads the optional NBT compound of an item stack.
///
/// 1.8 writes a single `0x00` byte when the stack has no NBT; otherwise a
/// plain (uncompressed) NBT compound follows — tag type `10`, the compound
/// name, the entries and the terminating `0x00`.
fn read_nbt(reader: &mut impl Read) -> Result<Option<Vec<u8>>, ProtocolError> {
    let mut recorder = RecordingReader::new(reader);
    let mut tag = [0u8; 1];
    recorder.read_exact(&mut tag)?;
    if tag[0] == 0 {
        return Ok(None);
    }
    if tag[0] != 10 {
        return Err(ProtocolError::Invariant(format!(
            "item stack NBT starts with tag type {} instead of a compound",
            tag[0]
        )));
    }
    skip_nbt_name(&mut recorder)?;
    skip_nbt_payload(&mut recorder, 10, 0)?;
    Ok(Some(recorder.into_recorded()))
}

/// Skips the name of an NBT entry (a 16-bit length plus that many bytes).
fn skip_nbt_name(reader: &mut impl Read) -> Result<(), ProtocolError> {
    let len = read_i16(reader)?;
    if len < 0 {
        return Err(ProtocolError::Invariant(format!(
            "negative NBT name length {len}"
        )));
    }
    skip_bytes(reader, len as usize)
}

/// Skips the payload of an NBT tag of the given type.
///
/// The tag type byte and the entry name have already been consumed; the
/// caller is positioned at the payload.
fn skip_nbt_payload(reader: &mut impl Read, tag_type: u8, depth: u32) -> Result<(), ProtocolError> {
    if depth > MAX_NBT_DEPTH {
        return Err(ProtocolError::Invariant(format!(
            "NBT nested deeper than {MAX_NBT_DEPTH} levels"
        )));
    }
    match tag_type {
        0 => {}                      // TAG_End carries no payload
        1 => skip_bytes(reader, 1)?, // TAG_Byte
        2 => skip_bytes(reader, 2)?, // TAG_Short
        3 => skip_bytes(reader, 4)?, // TAG_Int
        4 => skip_bytes(reader, 8)?, // TAG_Long
        5 => skip_bytes(reader, 4)?, // TAG_Float
        6 => skip_bytes(reader, 8)?, // TAG_Double
        7 => {
            // TAG_Byte_Array: int length, then that many bytes.
            let len = read_i32(reader)?;
            skip_bytes(reader, checked_len(len)?)?;
        }
        8 => {
            // TAG_String: short length, then that many bytes.
            let len = read_i16(reader)?;
            if len < 0 {
                return Err(ProtocolError::Invariant(format!(
                    "negative NBT string length {len}"
                )));
            }
            skip_bytes(reader, len as usize)?;
        }
        9 => {
            // TAG_List: element type, int count, then the elements.
            let element_type = read_u8(reader)?;
            let count = read_i32(reader)?;
            if count < 0 {
                return Err(ProtocolError::Invariant(format!(
                    "negative NBT list length {count}"
                )));
            }
            for _ in 0..count {
                skip_nbt_payload(reader, element_type, depth + 1)?;
            }
        }
        10 => {
            // TAG_Compound: entries until the terminating TAG_End.
            loop {
                let mut tag = [0u8; 1];
                reader.read_exact(&mut tag)?;
                if tag[0] == 0 {
                    break;
                }
                skip_nbt_name(reader)?;
                skip_nbt_payload(reader, tag[0], depth + 1)?;
            }
        }
        11 => {
            // TAG_Int_Array: int length, then that many 32-bit values.
            let len = read_i32(reader)?;
            let len = checked_len(len)?;
            skip_bytes(
                reader,
                len.checked_mul(4).ok_or_else(|| {
                    ProtocolError::Invariant("NBT int array length overflows".to_string())
                })?,
            )?;
        }
        other => {
            return Err(ProtocolError::Invariant(format!(
                "unknown NBT tag type {other}"
            )))
        }
    }
    Ok(())
}

/// Converts a length prefix into a `usize`, rejecting negative and
/// implausible values.
fn checked_len(len: i32) -> Result<usize, ProtocolError> {
    if len < 0 {
        return Err(ProtocolError::Invariant(format!(
            "negative NBT length {len}"
        )));
    }
    let len = len as usize;
    if len > MAX_NBT_BYTES {
        return Err(ProtocolError::Invariant(format!(
            "NBT length {len} exceeds the packet budget of {MAX_NBT_BYTES} bytes"
        )));
    }
    Ok(len)
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
