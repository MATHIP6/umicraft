// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Maps` (0x34, clientbound, play): map item data — decorations and an
//! optional rectangle of colour columns.
//!
//! Layout (MCP `S34PacketMaps`, `readPacketData`): map id (VarInt), scale
//! (i8), icon count (VarInt), per icon three bytes (type in the high nibble
//! and direction in the low nibble of the first, then x and z as i8), the
//! number of columns (unsigned byte) and, only when that is above zero, the
//! row count, first column and first row (unsigned bytes) and a byte array
//! (VarInt length) of colour indices.
//!
//! The icon count and the array length come from the wire, so both are
//! capped before allocating; a map is 128 by 128 colours at most.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_byte_array, read_u8, write_byte_array};
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// Most decorations accepted in one packet (vanilla maps hold far fewer).
const MAX_ICONS: usize = 4096;
/// Largest colour payload: a full 128 by 128 map.
const MAX_COLOR_BYTES: usize = 128 * 128;
/// Initial `Vec` capacity; the vector grows with the data actually received.
const INITIAL_CAPACITY: usize = 16;

/// One map decoration (player marker, frame, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapIcon {
    /// Icon type (0 to 15; 0 white arrow, 1 green arrow, ...).
    pub icon_type: u8,
    /// Rotation (0 to 15, sixteenths of a turn).
    pub direction: u8,
    /// Horizontal position on the map, -128 to 127.
    pub x: i8,
    /// Vertical position on the map, -128 to 127.
    pub z: i8,
}

/// A rectangle of map colours sent with the packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapColumns {
    /// Width of the rectangle in columns (always above zero).
    pub columns: u8,
    /// Height of the rectangle in rows.
    pub rows: u8,
    /// First column of the rectangle.
    pub x: u8,
    /// First row of the rectangle.
    pub z: u8,
    /// Colour indices of the rectangle.
    pub data: Vec<u8>,
}

/// `Maps` (0x34, clientbound, play).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Maps {
    /// Map item damage value identifying the map.
    pub map_id: i32,
    /// Map zoom level.
    pub scale: i8,
    /// Decorations in wire order.
    pub icons: Vec<MapIcon>,
    /// Updated colour rectangle, absent when the column count was zero.
    pub columns: Option<MapColumns>,
}

fn read_icon(reader: &mut impl Read) -> Result<MapIcon, ProtocolError> {
    let packed = read_u8(reader)?;
    Ok(MapIcon {
        icon_type: packed >> 4,
        direction: packed & 0x0F,
        x: read_i8(reader)?,
        z: read_i8(reader)?,
    })
}

fn read_columns(reader: &mut impl Read, columns: u8) -> Result<MapColumns, ProtocolError> {
    Ok(MapColumns {
        columns,
        rows: read_u8(reader)?,
        x: read_u8(reader)?,
        z: read_u8(reader)?,
        data: read_byte_array(reader, MAX_COLOR_BYTES)?,
    })
}

impl Maps {
    /// Wire packet id.
    pub const ID: u8 = 0x34;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a malformed VarInt, or a negative or
    /// oversized icon count or colour array.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let map_id = read_var_i32(reader)?;
        let scale = read_i8(reader)?;
        let declared = read_var_i32(reader)?;
        let count = usize::try_from(declared)
            .ok()
            .filter(|&n| n <= MAX_ICONS)
            .ok_or_else(|| {
                ProtocolError::Invariant(format!("map icon count {declared} out of range"))
            })?;
        let mut icons = Vec::with_capacity(count.min(INITIAL_CAPACITY));
        for _ in 0..count {
            icons.push(read_icon(reader)?);
        }
        let column_count = read_u8(reader)?;
        let columns = if column_count > 0 {
            Some(read_columns(reader, column_count)?)
        } else {
            None
        };
        Ok(Self {
            map_id,
            scale,
            icons,
            columns,
        })
    }
}

impl EncodablePacket for Maps {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.map_id as u32)?;
        write_i8(writer, self.scale)?;
        write_var_u32(writer, self.icons.len() as u32)?;
        for icon in &self.icons {
            if icon.icon_type > 15 || icon.direction > 15 {
                return Err(ProtocolError::Invariant(format!(
                    "map icon nibbles out of range: type {}, direction {}",
                    icon.icon_type, icon.direction
                )));
            }
            writer.write_all(&[icon.icon_type << 4 | icon.direction])?;
            write_i8(writer, icon.x)?;
            write_i8(writer, icon.z)?;
        }
        match &self.columns {
            Some(update) if update.columns == 0 => {
                // Zero columns is the wire marker for "no update".
                return Err(ProtocolError::Invariant(
                    "a map column update needs at least one column".into(),
                ));
            }
            Some(update) => {
                writer.write_all(&[update.columns, update.rows, update.x, update.z])?;
                write_byte_array(writer, &update.data)?;
            }
            None => writer.write_all(&[0])?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_wire_shape_with_columns() {
        let expected = [
            0x03, // map id
            0x02, // scale
            0x01, // one icon
            0x1A, 0xF6, 0x0A, // type 1, direction 10, x -10, z 10
            0x02, 0x01, 0x04, 0x05, // 2 columns, 1 row, x 4, z 5
            0x02, 0x07, 0x08, // data
        ];
        let packet = Maps {
            map_id: 3,
            scale: 2,
            icons: vec![MapIcon {
                icon_type: 1,
                direction: 10,
                x: -10,
                z: 10,
            }],
            columns: Some(MapColumns {
                columns: 2,
                rows: 1,
                x: 4,
                z: 5,
                data: vec![7, 8],
            }),
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(Maps::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn zero_columns_means_no_update_and_nothing_follows() {
        let packet = Maps {
            map_id: 1,
            scale: 0,
            icons: Vec::new(),
            columns: None,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, [0x01, 0x00, 0x00, 0x00]);
        assert_eq!(Maps::decode(&mut &buffer[..]).unwrap(), packet);
    }

    #[test]
    fn hostile_maps_are_errors() {
        assert!(Maps::decode(&mut &[][..]).is_err());
        // Negative icon count (VarInt -1).
        assert!(Maps::decode(&mut &[1, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F][..]).is_err());
        // Icon count of two million.
        assert!(Maps::decode(&mut &[1, 0, 0x80, 0x89, 0x7A][..]).is_err());
        // Colour array of 1 MiB.
        assert!(Maps::decode(&mut &[1, 0, 0, 1, 1, 0, 0, 0x80, 0x80, 0x40][..]).is_err());
        // Truncated array.
        assert!(Maps::decode(&mut &[1, 0, 0, 1, 1, 0, 0, 5, 1][..]).is_err());
    }
}
