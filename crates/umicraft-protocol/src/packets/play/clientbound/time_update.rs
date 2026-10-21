// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Time Update` (0x03, clientbound, play): world age and time of day.
//!
//! Layout (MCP `S03PacketTimeUpdate`, `readPacketData`): two big-endian
//! `i64` values — the total world age and the time of day.
//!
//! The reference server negates the time of day when the day/light cycle is
//! switched off (`S03PacketTimeUpdate(long, long, boolean)` flips the sign
//! and maps `0` to `-1`), so a negative time of day means "the sun does not
//! move".

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i64;
use crate::packets::EncodablePacket;

/// `Time Update` (0x03, clientbound, play): the world clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeUpdate {
    /// Total age of the world in ticks.
    pub world_age: i64,
    /// Time of day in ticks; negative when the day/light cycle is frozen.
    pub time_of_day: i64,
}

impl TimeUpdate {
    /// Wire packet id.
    pub const ID: u8 = 0x03;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            world_age: read_i64(reader)?,
            time_of_day: read_i64(reader)?,
        })
    }
}

impl EncodablePacket for TimeUpdate {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.world_age.to_be_bytes())?;
        writer.write_all(&self.time_of_day.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_update_wire_shape() {
        // Captured from a live 1.8 server: 16 body bytes, two i64 values.
        let bytes = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x1C, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
            0x1C, 0x42,
        ];
        let packet = TimeUpdate::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.world_age, 72_770);
        assert_eq!(packet.time_of_day, 72_770);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, bytes);
    }

    #[test]
    fn a_frozen_day_light_cycle_arrives_as_a_negative_time() {
        let bytes = [0xFF; 16];
        let packet = TimeUpdate::decode(&mut &bytes[..]).unwrap();
        assert_eq!(packet.world_age, -1);
        assert_eq!(packet.time_of_day, -1);
    }
}
