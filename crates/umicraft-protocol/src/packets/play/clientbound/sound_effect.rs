// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Sound Effect` (0x29, clientbound, play): a named sound at a position.
//!
//! Layout (MCP `S29PacketSoundEffect`, `readPacketData`): sound name (string,
//! at most 256 chars), x/y/z as i32 holding the coordinate * 8, volume (f32)
//! and pitch as an unsigned byte (the reference divides by 63 to get the
//! playback pitch).

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_f32, read_i32, read_u8};
use crate::packets::EncodablePacket;
use crate::types::{read_string, write_string};

/// Longest sound name the reference client reads.
const MAX_NAME_CHARS: usize = 256;

/// `Sound Effect` (0x29, clientbound, play).
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEffect {
    /// Sound event name, for example `random.click`.
    pub name: String,
    /// X position as coordinate * 8.
    pub x: i32,
    /// Y position as coordinate * 8.
    pub y: i32,
    /// Z position as coordinate * 8.
    pub z: i32,
    /// Volume multiplier.
    pub volume: f32,
    /// Pitch byte (63 is normal pitch).
    pub pitch: u8,
}

impl SoundEffect {
    /// Wire packet id.
    pub const ID: u8 = 0x29;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or an over-long or non-UTF-8 name.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            name: read_string(reader, MAX_NAME_CHARS)?,
            x: read_i32(reader)?,
            y: read_i32(reader)?,
            z: read_i32(reader)?,
            volume: read_f32(reader)?,
            pitch: read_u8(reader)?,
        })
    }
}

impl EncodablePacket for SoundEffect {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_string(writer, &self.name, MAX_NAME_CHARS)?;
        writer.write_all(&self.x.to_be_bytes())?;
        writer.write_all(&self.y.to_be_bytes())?;
        writer.write_all(&self.z.to_be_bytes())?;
        writer.write_all(&self.volume.to_be_bytes())?;
        writer.write_all(&[self.pitch])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_effect_wire_shape() {
        let mut expected = vec![0x0C];
        expected.extend_from_slice(b"random.click");
        expected.extend_from_slice(&80i32.to_be_bytes());
        expected.extend_from_slice(&(-8i32).to_be_bytes());
        expected.extend_from_slice(&16i32.to_be_bytes());
        expected.extend_from_slice(&1.0f32.to_be_bytes());
        expected.push(63);
        let packet = SoundEffect {
            name: "random.click".into(),
            x: 80,
            y: -8,
            z: 16,
            volume: 1.0,
            pitch: 63,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(SoundEffect::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn hostile_sound_effects_are_errors() {
        assert!(SoundEffect::decode(&mut &[][..]).is_err());
        // Declared name length of 100000 bytes is over budget.
        assert!(SoundEffect::decode(&mut &[0xA0, 0x8D, 0x06][..]).is_err());
        assert!(SoundEffect::decode(&mut &[0x01, b'a', 0, 0][..]).is_err());
    }
}
