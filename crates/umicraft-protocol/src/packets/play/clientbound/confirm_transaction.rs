// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Confirm Transaction` (0x32, clientbound, play): the server confirms or
//! rejects an inventory transaction.
//!
//! Layout (MCP `S32PacketConfirmTransaction`, `readPacketData`):
//!
//! | field | width | notes |
//! |---|---|---|
//! | window id | u8 | |
//! | action number | i16 | the number sent in the `Click Window` |
//! | accepted | bool | `false` if the server rejected the click |

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::{read_bool, read_u8};
use crate::packets::play::wire::{read_i16, write_i16};
use crate::packets::EncodablePacket;

/// `Confirm Transaction` (0x32, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfirmTransaction {
    /// The window id.
    pub window_id: u8,
    /// The action number from the `Click Window` packet.
    pub action_number: i16,
    /// Whether the server accepted the click.
    pub accepted: bool,
}

impl ConfirmTransaction {
    /// Wire packet id.
    pub const ID: u8 = 0x32;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let window_id = read_u8(reader)?;
        let action_number = read_i16(reader)?;
        let accepted = read_bool(reader)?;
        Ok(Self {
            window_id,
            action_number,
            accepted,
        })
    }
}

impl EncodablePacket for ConfirmTransaction {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[self.window_id])?;
        write_i16(writer, self.action_number)?;
        writer.write_all(&[u8::from(self.accepted)])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_accepted_transaction_round_trips() {
        let packet = ConfirmTransaction {
            window_id: 0,
            action_number: 1,
            accepted: true,
        };
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        assert_eq!(ConfirmTransaction::decode(&mut &buf[..]).unwrap(), packet);
    }

    #[test]
    fn a_rejected_transaction_round_trips() {
        let packet = ConfirmTransaction {
            window_id: 3,
            action_number: -7,
            accepted: false,
        };
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        assert_eq!(ConfirmTransaction::decode(&mut &buf[..]).unwrap(), packet);
    }
}
