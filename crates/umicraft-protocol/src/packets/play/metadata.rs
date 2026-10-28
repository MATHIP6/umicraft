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

    /// Finds a watched object by index.
    #[must_use]
    pub fn get(&self, index: u8) -> Option<&WatchedObject> {
        self.objects.iter().find(|obj| obj.index == index)
    }

    /// Reads a byte value at the given index.
    #[must_use]
    pub fn get_byte(&self, index: u8) -> Option<u8> {
        self.get(index)
            .filter(|obj| obj.type_id == 0 && obj.value.len() == 1)
            .map(|obj| obj.value[0])
    }

    /// Reads an i16 short value at the given index.
    #[must_use]
    pub fn get_short(&self, index: u8) -> Option<i16> {
        self.get(index)
            .filter(|obj| obj.type_id == 1 && obj.value.len() == 2)
            .map(|obj| i16::from_be_bytes([obj.value[0], obj.value[1]]))
    }

    /// Reads an i32 int value at the given index.
    #[must_use]
    pub fn get_int(&self, index: u8) -> Option<i32> {
        self.get(index)
            .filter(|obj| obj.type_id == 2 && obj.value.len() == 4)
            .map(|obj| i32::from_be_bytes([obj.value[0], obj.value[1], obj.value[2], obj.value[3]]))
    }

    /// Reads an f32 float value at the given index.
    #[must_use]
    pub fn get_float(&self, index: u8) -> Option<f32> {
        self.get(index)
            .filter(|obj| obj.type_id == 3 && obj.value.len() == 4)
            .map(|obj| f32::from_be_bytes([obj.value[0], obj.value[1], obj.value[2], obj.value[3]]))
    }

    /// Reads an item stack value (type id 5) at the given index.
    ///
    /// `None` when the entry is missing, has another type, or does not decode.
    #[must_use]
    pub fn get_item_stack(&self, index: u8) -> Option<ItemStack> {
        self.get(index)
            .filter(|obj| obj.type_id == 5)
            .and_then(|obj| ItemStack::decode(&mut obj.value.as_slice()).ok())
    }

    /// Reads a string value at the given index.
    ///
    /// The raw `value` includes the VarInt length prefix; this method skips it
    /// and returns only the UTF-8 payload.
    #[must_use]
    pub fn get_string(&self, index: u8) -> Option<String> {
        self.get(index)
            .filter(|obj| obj.type_id == 4)
            .and_then(|obj| {
                let mut cursor = obj.value.as_slice();
                let len = read_var_u32(&mut cursor).ok()? as usize;
                if cursor.len() < len {
                    return None;
                }
                String::from_utf8(cursor[..len].to_vec()).ok()
            })
    }

    /// Decodes the standard 1.8 data watcher fields into a typed structure.
    #[must_use]
    pub fn to_typed(&self) -> TypedMetadata {
        let flags = self.get_byte(0).unwrap_or(0);
        let air = self.get_short(1).unwrap_or(300);
        let custom_name = self.get_string(2).unwrap_or_default();
        let custom_name_visible = self.get_byte(3).unwrap_or(0) != 0;
        let health = self.get_float(6).unwrap_or(20.0);
        let skin_parts = self.get_byte(10).unwrap_or(0x7F);
        let is_baby = self.get_byte(12).unwrap_or(0) == 1;

        TypedMetadata {
            flags,
            air,
            custom_name,
            custom_name_visible,
            health,
            skin_parts,
            is_baby,
        }
    }
}

/// Standard typed 1.8 entity metadata decoded from standard data watchers.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedMetadata {
    /// Entity status flags (index 0).
    pub flags: u8,
    /// Air ticks remaining (index 1).
    pub air: i16,
    /// Custom name (index 2).
    pub custom_name: String,
    /// Custom name visibility (index 3).
    pub custom_name_visible: bool,
    /// Health (index 6).
    pub health: f32,
    /// Player skin parts (index 10).
    pub skin_parts: u8,
    /// Whether ageable mob is a baby (index 12).
    pub is_baby: bool,
}

impl Default for TypedMetadata {
    fn default() -> Self {
        Self {
            flags: 0,
            air: 300,
            custom_name: String::new(),
            custom_name_visible: false,
            health: 20.0,
            skin_parts: 0x7F,
            is_baby: false,
        }
    }
}

impl TypedMetadata {
    /// Returns true if the entity is burning.
    #[must_use]
    pub fn on_fire(&self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Returns true if the entity is crouching / sneaking.
    #[must_use]
    pub fn is_sneaking(&self) -> bool {
        self.flags & 0x02 != 0
    }

    /// Returns true if the entity is sprinting.
    #[must_use]
    pub fn is_sprinting(&self) -> bool {
        self.flags & 0x08 != 0
    }

    /// Returns true if the entity is eating / drawing a bow.
    #[must_use]
    pub fn is_eating(&self) -> bool {
        self.flags & 0x10 != 0
    }

    /// Returns true if the entity is invisible.
    #[must_use]
    pub fn is_invisible(&self) -> bool {
        self.flags & 0x20 != 0
    }

    /// Returns true if the player's cape is enabled.
    #[must_use]
    pub fn cape_enabled(&self) -> bool {
        self.skin_parts & 0x01 != 0
    }

    /// Returns true if the player's jacket is enabled.
    #[must_use]
    pub fn jacket_enabled(&self) -> bool {
        self.skin_parts & 0x02 != 0
    }

    /// Returns true if the player's left sleeve is enabled.
    #[must_use]
    pub fn left_sleeve_enabled(&self) -> bool {
        self.skin_parts & 0x04 != 0
    }

    /// Returns true if the player's right sleeve is enabled.
    #[must_use]
    pub fn right_sleeve_enabled(&self) -> bool {
        self.skin_parts & 0x08 != 0
    }

    /// Returns true if the player's left pants leg is enabled.
    #[must_use]
    pub fn left_pants_enabled(&self) -> bool {
        self.skin_parts & 0x10 != 0
    }

    /// Returns true if the player's right pants leg is enabled.
    #[must_use]
    pub fn right_pants_enabled(&self) -> bool {
        self.skin_parts & 0x20 != 0
    }

    /// Returns true if the player's hat / helmet layer is enabled.
    #[must_use]
    pub fn hat_enabled(&self) -> bool {
        self.skin_parts & 0x40 != 0
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
            // Use a recording reader so the VarInt prefix is kept in `value`
            // and the roundtrip encode reproduces the original wire bytes.
            let mut recorder = RecordingReader::new(reader);
            let len = read_var_u32(&mut recorder)? as usize;
            if len > MAX_STRING_BYTES {
                return Err(ProtocolError::Invariant(format!(
                    "data watcher string of {len} bytes exceeds the packet budget"
                )));
            }
            let mut bytes = vec![0u8; len];
            recorder.read_exact(&mut bytes)?;
            Ok(recorder.into_recorded())
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
    fn an_item_stack_is_read_with_and_without_nbt() {
        let bytes = [
            0xA8, 0x01, 0x16, 0x02, 0x00, 0x03, 0x00, // index 8: id 278, 2 items, damage 3
            0xAA, 0x00, 0x05, 0x01, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, // index 10 with NBT
            0x7F,
        ];
        let metadata = Metadata::decode(&mut &bytes[..]).unwrap();
        let plain = metadata.get_item_stack(8).unwrap();
        assert_eq!((plain.item_id, plain.stack_size, plain.damage), (278, 2, 3));
        assert_eq!(plain.nbt, None);
        let tagged = metadata.get_item_stack(10).unwrap();
        assert_eq!(tagged.item_id, 5);
        assert_eq!(tagged.nbt, Some(vec![0x0A, 0x00, 0x00, 0x00]));
    }

    #[test]
    fn a_wrong_type_or_missing_index_is_no_item_stack() {
        let bytes = [0x08, 0x01, 0x7F]; // byte at index 8
        let metadata = Metadata::decode(&mut &bytes[..]).unwrap();
        assert_eq!(metadata.get_item_stack(8), None);
        assert_eq!(metadata.get_item_stack(9), None);
    }

    #[test]
    fn a_corrupt_item_stack_value_is_none() {
        let metadata = Metadata {
            objects: vec![WatchedObject {
                type_id: 5,
                index: 8,
                value: vec![0x00],
            }],
        };
        assert_eq!(metadata.get_item_stack(8), None);
    }

    #[test]
    fn a_string_value_round_trips() {
        // Type 4 (string), index 2: "Hi" encoded as VarInt(2) + 0x48 0x69.
        let bytes = [
            0x82, // header: type 4 (string), index 2
            0x02, 0x48, 0x69, // VarInt length 2 + "Hi"
            0x7F, // terminator
        ];
        let metadata = Metadata::decode(&mut &bytes[..]).unwrap();
        assert_eq!(metadata.objects.len(), 1);
        assert_eq!(metadata.objects[0].type_id, 4);
        assert_eq!(metadata.objects[0].index, 2);
        assert_eq!(metadata.objects[0].value, vec![0x02, 0x48, 0x69]);

        let mut buffer = Vec::new();
        metadata.encode(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);

        assert_eq!(metadata.get_string(2), Some("Hi".to_string()),);
    }

    #[test]
    fn a_truncated_list_is_an_error_not_a_panic() {
        assert!(Metadata::decode(&mut &[][..]).is_err());
        assert!(Metadata::decode(&mut &[0x00][..]).is_err());
        // Unknown type id 8.
        assert!(Metadata::decode(&mut &[0x00, 0x7F][..]).is_err());
    }
}
