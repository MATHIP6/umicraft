// SPDX-License-Identifier: MIT OR Apache-2.0

//! The 1.8 data-watcher list (the metadata framing).
//!
//! 1.8 does **not** prefix the watcher list with a count. It is a stream of
//! entries, each `[header u8]` with `header = (type id << 5) | data value
//! id`, terminated by `0x7F` (MCP `net/minecraft/entity/DataWatcher.java`,
//! `readWatchedListFromPacketBuffer` / `writeTo`). The value of an entry
//! follows the header and its width depends on the type id:
//!
//! | type id | value | width |
//! |---|---|---|
//! | 0 | byte | 1 |
//! | 1 | short | 2 |
//! | 2 | int | 4 |
//! | 3 | float | 4 |
//! | 4 | string | VarInt byte length + bytes |
//! | 5 | item stack | see [`crate::packets::play::item_stack`] |
//! | 6 | block position | 3 × i32 |
//! | 7 | rotations | 3 × f32 |
//!
//! ADR-0005 §A.4 defers the *typed* metadata model (and with it the
//! `Entity Metadata` packet 0x1C) to its own decision. This module
//! therefore provides only the framing — enough for `Spawn Player` (0x0C),
//! which embeds a watcher list — and keeps every value as raw bytes.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::item_stack::ItemStack;
use crate::packets::play::wire::RecordingReader;
use crate::varint::read_var_u32;

/// Header byte that terminates a 1.8 data-watcher list.
pub const TERMINATOR: u8 = 0x7F;

/// Maximum number of entries accepted in one watcher list.
///
/// Vanilla tracks at most 32 data value ids per entity; the cap only exists
/// so a hostile server cannot make the client allocate without bound.
const MAX_OBJECTS: usize = 256;

/// Maximum byte length a watcher-list string may claim.
const MAX_STRING_BYTES: usize = 32_767 * 4;

/// One entry of a 1.8 data-watcher list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchedObject {
    /// Type id of the value (0–7, see the module documentation).
    pub type_id: u8,
    /// Data value id inside the entity (0–31).
    pub index: u8,
    /// The raw value bytes, exactly as they appeared on the wire.
    pub value: Vec<u8>,
}

/// A decoded 1.8 data-watcher list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    /// The entries, in wire order.
    pub objects: Vec<WatchedObject>,
}

impl Metadata {
    /// Reads a watcher list, including its `0x7F` terminator.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on truncation, [`ProtocolError::Invariant`] on
    /// an unknown type id or an implausible length prefix.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let mut objects = Vec::new();
        loop {
            let mut header = [0u8; 1];
            reader.read_exact(&mut header)?;
            let header = header[0];
            if header == TERMINATOR {
                break;
            }
            if objects.len() >= MAX_OBJECTS {
                return Err(ProtocolError::Invariant(format!(
                    "data watcher list longer than {MAX_OBJECTS} entries"
                )));
            }
            let type_id = header >> 5;
            let index = header & 0x1F;
            let value = read_value(reader, type_id)?;
            objects.push(WatchedObject {
                type_id,
                index,
                value,
            });
        }
        Ok(Self { objects })
    }

    /// Writes a watcher list, including its `0x7F` terminator.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on write failure.
    pub fn encode(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        for object in &self.objects {
            writer.write_all(&[(object.type_id << 5) | (object.index & 0x1F)])?;
            writer.write_all(&object.value)?;
        }
        writer.write_all(&[TERMINATOR])?;
        Ok(())
    }
}

/// Reads the value of one watcher entry, keeping its raw bytes.
fn read_value(reader: &mut impl Read, type_id: u8) -> Result<Vec<u8>, ProtocolError> {
    match type_id {
        0 => read_exact_bytes(reader, 1), // byte
        1 => read_exact_bytes(reader, 2), // short
        2 => read_exact_bytes(reader, 4), // int
        3 => read_exact_bytes(reader, 4), // float
        4 => {
            // String: VarInt byte length, then the UTF-8 bytes.
            let len = read_var_u32(reader)? as usize;
            if len > MAX_STRING_BYTES {
                return Err(ProtocolError::Invariant(format!(
                    "data watcher string of {len} bytes exceeds the packet budget"
                )));
            }
            read_exact_bytes(reader, len)
        }
        5 => {
            // Item stack: variable width, so it is parsed with a recording
            // reader to learn how many bytes it occupied.
            let mut recorder = RecordingReader::new(reader);
            ItemStack::decode(&mut recorder)?;
            Ok(recorder.into_recorded())
        }
        6 => read_exact_bytes(reader, 12), // block position: 3 × i32
        7 => read_exact_bytes(reader, 12), // rotations: 3 × f32
        other => Err(ProtocolError::Invariant(format!(
            "unknown data watcher type id {other}"
        ))),
    }
}

/// Reads exactly `len` bytes into a fresh buffer.
fn read_exact_bytes(reader: &mut impl Read, len: usize) -> Result<Vec<u8>, ProtocolError> {
    let mut bytes = vec![0u8; len];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watcher_list_round_trips() {
        let bytes = [
            0x00, 0x2A, // type 0 (byte), index 0, value 42
            0x23, 0x00, 0x01, // type 1 (short), index 3, value 1
            0x7F, // terminator
        ];
        let metadata = Metadata::decode(&mut &bytes[..]).unwrap();
        assert_eq!(metadata.objects.len(), 2);
        assert_eq!(metadata.objects[0].type_id, 0);
        assert_eq!(metadata.objects[0].index, 0);
        assert_eq!(metadata.objects[0].value, vec![0x2A]);
        assert_eq!(metadata.objects[1].type_id, 1);
        assert_eq!(metadata.objects[1].index, 3);
        assert_eq!(metadata.objects[1].value, vec![0x00, 0x01]);

        let mut buffer = Vec::new();
        metadata.encode(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn an_empty_list_is_just_the_terminator() {
        let metadata = Metadata::decode(&mut &[TERMINATOR][..]).unwrap();
        assert!(metadata.objects.is_empty());
        let mut buffer = Vec::new();
        metadata.encode(&mut buffer).unwrap();
        assert_eq!(buffer, [TERMINATOR]);
    }

    #[test]
    fn an_item_stack_value_is_captured_whole() {
        // Type 5 (item stack), index 0: stone, 64 items, no NBT.
        let bytes = [
            0xA0, // header: type 5 (item stack), index 0
            0x00, 0x01, // item id 1
            0x40, // stack size 64
            0x00, 0x00, // damage 0
            0x00, // no NBT
            0x7F, // terminator
        ];
        let metadata = Metadata::decode(&mut &bytes[..]).unwrap();
        assert_eq!(metadata.objects[0].type_id, 5);
        assert_eq!(
            metadata.objects[0].value,
            vec![0x00, 0x01, 0x40, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn a_truncated_list_is_an_error_not_a_panic() {
        assert!(Metadata::decode(&mut &[][..]).is_err());
        assert!(Metadata::decode(&mut &[0x00][..]).is_err());
        // Unknown type id 8.
        assert!(Metadata::decode(&mut &[0x00, 0x7F][..]).is_err());
    }
}
