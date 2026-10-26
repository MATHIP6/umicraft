// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Window Property` (0x31, clientbound, play): the server updates a
//! property of a container (furnace progress, enchanting levels, …).
//!
//! Layout (MCP `S31PacketWindowProperty`, `readPacketData`):
//!
//! | field | width | notes |
//! |---|---|---|
//! | window id | u8 | |
//! | property | i16 | property index (meaning depends on window type) |
//! | value | i16 | |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::play::wire::{read_i16, write_i16};
use crate::packets::EncodablePacket;

/// `Window Property` (0x31, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowProperty {
    /// The window id.
    pub window_id: u8,
    /// The property index; the meaning depends on the window type.
    pub property: i16,
    /// The property value.
    pub value: i16,
}

impl WindowProperty {
    /// Wire packet id.
    pub const ID: u8 = 0x31;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let window_id = read_u8(reader)?;
        let property = read_i16(reader)?;
        let value = read_i16(reader)?;
        Ok(Self {
            window_id,
            property,
            value,
        })
    }
}

impl EncodablePacket for WindowProperty {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[self.window_id])?;
        write_i16(writer, self.property)?;
        write_i16(writer, self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_property_round_trips() {
        let packet = WindowProperty {
            window_id: 3,
            property: 0,
            value: 200,
        };
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        assert_eq!(WindowProperty::decode(&mut &buf[..]).unwrap(), packet);
    }
}
