// SPDX-License-Identifier: MIT OR Apache-2.0

//! `World Border` (0x44, clientbound, play): world border updates.
//!
//! Layout (MCP `S44PacketWorldBorder`, `readPacketData`): a VarInt action,
//! then by action (doubles are big-endian f64):
//!
//! | action | fields |
//! |---|---|
//! | 0 set size | diameter |
//! | 1 lerp size | old diameter, new diameter, VarLong milliseconds |
//! | 2 set center | x, z |
//! | 3 initialize | x, z, old diameter, new diameter, VarLong ms, VarInt portal boundary, VarInt warning time, VarInt warning blocks |
//! | 4 set warning time | VarInt seconds |
//! | 5 set warning blocks | VarInt blocks |
//!
//! The reference client indexes an enum with the action and crashes on an
//! unknown one; here it is [`ProtocolError::Invariant`]. Speeds are VarLong,
//! which [`crate::varint`] does not provide, so a bounded (10 byte) codec
//! lives in this module.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_f64;
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// Maximum number of bytes of a 64-bit VarLong.
const VAR_LONG_MAX: usize = 10;

fn read_var_i64(reader: &mut impl Read) -> Result<i64, ProtocolError> {
    let mut result = 0u64;
    for index in 0..VAR_LONG_MAX {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        let payload = u64::from(byte[0] & 0x7F);
        if index == VAR_LONG_MAX - 1 && payload > 1 {
            // The 10th byte may only carry bit 63.
            return Err(ProtocolError::VarIntTooLong);
        }
        result |= payload << (index * 7);
        if byte[0] & 0x80 == 0 {
            return Ok(result as i64);
        }
    }
    Err(ProtocolError::VarIntTooLong)
}

fn write_var_u64(writer: &mut impl Write, mut value: u64) -> Result<(), ProtocolError> {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            writer.write_all(&[byte])?;
            return Ok(());
        }
        writer.write_all(&[byte | 0x80])?;
    }
}

fn write_f64(writer: &mut impl Write, value: f64) -> Result<(), ProtocolError> {
    writer.write_all(&value.to_be_bytes())?;
    Ok(())
}

/// `World Border` (0x44, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorldBorder {
    /// Action 0: jumps to a diameter.
    SetSize {
        /// New diameter in blocks.
        diameter: f64,
    },
    /// Action 1: moves to a new diameter over time.
    LerpSize {
        /// Current diameter.
        old_diameter: f64,
        /// Target diameter.
        new_diameter: f64,
        /// Travel time in milliseconds.
        speed: i64,
    },
    /// Action 2: moves the centre.
    SetCenter {
        /// Centre X.
        x: f64,
        /// Centre Z.
        z: f64,
    },
    /// Action 3: sends the whole border (on join and respawn).
    Initialize {
        /// Centre X.
        x: f64,
        /// Centre Z.
        z: f64,
        /// Current diameter.
        old_diameter: f64,
        /// Target diameter.
        new_diameter: f64,
        /// Travel time in milliseconds.
        speed: i64,
        /// Portal teleport boundary.
        portal_boundary: i32,
        /// Warning time in seconds.
        warning_time: i32,
        /// Warning distance in blocks.
        warning_blocks: i32,
    },
    /// Action 4: sets the warning time.
    SetWarningTime {
        /// Seconds.
        warning_time: i32,
    },
    /// Action 5: sets the warning distance.
    SetWarningBlocks {
        /// Blocks.
        warning_blocks: i32,
    },
}

impl WorldBorder {
    /// Wire packet id.
    pub const ID: u8 = 0x44;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation, a malformed VarInt/VarLong or an
    /// unknown action.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        match read_var_i32(reader)? {
            0 => Ok(Self::SetSize {
                diameter: read_f64(reader)?,
            }),
            1 => Ok(Self::LerpSize {
                old_diameter: read_f64(reader)?,
                new_diameter: read_f64(reader)?,
                speed: read_var_i64(reader)?,
            }),
            2 => Ok(Self::SetCenter {
                x: read_f64(reader)?,
                z: read_f64(reader)?,
            }),
            3 => Ok(Self::Initialize {
                x: read_f64(reader)?,
                z: read_f64(reader)?,
                old_diameter: read_f64(reader)?,
                new_diameter: read_f64(reader)?,
                speed: read_var_i64(reader)?,
                portal_boundary: read_var_i32(reader)?,
                warning_time: read_var_i32(reader)?,
                warning_blocks: read_var_i32(reader)?,
            }),
            4 => Ok(Self::SetWarningTime {
                warning_time: read_var_i32(reader)?,
            }),
            5 => Ok(Self::SetWarningBlocks {
                warning_blocks: read_var_i32(reader)?,
            }),
            other => Err(ProtocolError::Invariant(format!(
                "unknown world border action {other}"
            ))),
        }
    }
}

impl EncodablePacket for WorldBorder {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        match *self {
            Self::SetSize { diameter } => {
                write_var_u32(writer, 0)?;
                write_f64(writer, diameter)
            }
            Self::LerpSize {
                old_diameter,
                new_diameter,
                speed,
            } => {
                write_var_u32(writer, 1)?;
                write_f64(writer, old_diameter)?;
                write_f64(writer, new_diameter)?;
                write_var_u64(writer, speed as u64)
            }
            Self::SetCenter { x, z } => {
                write_var_u32(writer, 2)?;
                write_f64(writer, x)?;
                write_f64(writer, z)
            }
            Self::Initialize {
                x,
                z,
                old_diameter,
                new_diameter,
                speed,
                portal_boundary,
                warning_time,
                warning_blocks,
            } => {
                write_var_u32(writer, 3)?;
                for value in [x, z, old_diameter, new_diameter] {
                    write_f64(writer, value)?;
                }
                write_var_u64(writer, speed as u64)?;
                write_var_u32(writer, portal_boundary as u32)?;
                write_var_u32(writer, warning_time as u32)?;
                write_var_u32(writer, warning_blocks as u32)
            }
            Self::SetWarningTime { warning_time } => {
                write_var_u32(writer, 4)?;
                write_var_u32(writer, warning_time as u32)
            }
            Self::SetWarningBlocks { warning_blocks } => {
                write_var_u32(writer, 5)?;
                write_var_u32(writer, warning_blocks as u32)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(packet: WorldBorder, expected: &[u8]) {
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(WorldBorder::decode(&mut &expected[..]).unwrap(), packet);
    }

    fn doubles(values: &[f64]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_be_bytes()).collect()
    }

    #[test]
    fn world_border_wire_shapes() {
        let mut bytes = vec![0];
        bytes.extend(doubles(&[100.0]));
        check(WorldBorder::SetSize { diameter: 100.0 }, &bytes);

        let mut bytes = vec![1];
        bytes.extend(doubles(&[100.0, 50.0]));
        bytes.extend([0xE8, 0x07]); // 1000 ms as a VarLong
        check(
            WorldBorder::LerpSize {
                old_diameter: 100.0,
                new_diameter: 50.0,
                speed: 1000,
            },
            &bytes,
        );

        let mut bytes = vec![2];
        bytes.extend(doubles(&[1.5, -2.5]));
        check(WorldBorder::SetCenter { x: 1.5, z: -2.5 }, &bytes);

        check(WorldBorder::SetWarningTime { warning_time: 15 }, &[4, 15]);
        check(WorldBorder::SetWarningBlocks { warning_blocks: 5 }, &[5, 5]);
    }

    #[test]
    fn initialize_wire_shape() {
        let mut bytes = vec![3];
        bytes.extend(doubles(&[0.0, 0.0, 60.0, 60.0]));
        bytes.push(0); // speed 0
        bytes.extend([0xF0, 0x86, 0xA7, 0x0E]); // 29999984 as a VarInt
        bytes.extend([15, 5]);
        check(
            WorldBorder::Initialize {
                x: 0.0,
                z: 0.0,
                old_diameter: 60.0,
                new_diameter: 60.0,
                speed: 0,
                portal_boundary: 29_999_984,
                warning_time: 15,
                warning_blocks: 5,
            },
            &bytes,
        );
    }

    #[test]
    fn var_long_round_trips_and_is_bounded() {
        for value in [0i64, 1, 127, 128, i64::MAX, -1, i64::MIN] {
            let mut buffer = Vec::new();
            write_var_u64(&mut buffer, value as u64).unwrap();
            assert!(buffer.len() <= VAR_LONG_MAX);
            assert_eq!(read_var_i64(&mut &buffer[..]).unwrap(), value);
        }
        assert!(matches!(
            read_var_i64(&mut &[0xFF; 10][..]),
            Err(ProtocolError::VarIntTooLong)
        ));
        assert!(read_var_i64(&mut &[0x80; 11][..]).is_err());
        assert!(read_var_i64(&mut &[0x80][..]).is_err());
    }

    #[test]
    fn hostile_world_border_is_an_error() {
        assert!(WorldBorder::decode(&mut &[][..]).is_err());
        assert!(WorldBorder::decode(&mut &[6][..]).is_err());
        assert!(WorldBorder::decode(&mut &[0, 0, 0][..]).is_err());
        assert!(WorldBorder::decode(&mut &[4][..]).is_err());
    }
}
