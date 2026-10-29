// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Entity Effect` (0x1D, clientbound, play): a potion effect was applied.
//!
//! Layout (MCP `S1DPacketEntityEffect`, `readPacketData`): entity id
//! (VarInt), effect id (i8), amplifier (i8), duration in ticks (VarInt) and
//! a "hide particles" flag (i8, nonzero hides them). The reference treats a
//! duration of 32767 as "infinite" for display.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;
use crate::varint::{read_var_i32, write_var_u32};

/// `Entity Effect` (0x1D, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityEffect {
    /// Entity the effect applies to.
    pub entity_id: i32,
    /// Potion effect id (1 speed, 2 slowness, ...).
    pub effect_id: i8,
    /// Effect level minus one.
    pub amplifier: i8,
    /// Remaining duration in ticks.
    pub duration: i32,
    /// Nonzero when the effect's particles are hidden.
    pub hide_particles: i8,
}

impl EntityEffect {
    /// Wire packet id.
    pub const ID: u8 = 0x1D;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on truncation or a malformed VarInt.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            entity_id: read_var_i32(reader)?,
            effect_id: read_i8(reader)?,
            amplifier: read_i8(reader)?,
            duration: read_var_i32(reader)?,
            hide_particles: read_i8(reader)?,
        })
    }
}

impl EncodablePacket for EntityEffect {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.entity_id as u32)?;
        write_i8(writer, self.effect_id)?;
        write_i8(writer, self.amplifier)?;
        write_var_u32(writer, self.duration as u32)?;
        write_i8(writer, self.hide_particles)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_effect_wire_shape() {
        let expected = [0x03, 0x01, 0x02, 0xF0, 0x2E, 0x00]; // duration 6000
        let packet = EntityEffect {
            entity_id: 3,
            effect_id: 1,
            amplifier: 2,
            duration: 6000,
            hide_particles: 0,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(EntityEffect::decode(&mut &expected[..]).unwrap(), packet);
    }

    #[test]
    fn a_truncated_effect_is_an_error() {
        assert!(EntityEffect::decode(&mut &[][..]).is_err());
        assert!(EntityEffect::decode(&mut &[0x03, 0x01, 0x02, 0xF0, 0x2E][..]).is_err());
    }
}
