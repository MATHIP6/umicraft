// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Confirm Transaction` (0x0F, serverbound, play): the client acknowledges
//! a rejected transaction.
//!
//! Layout (MCP `C0FPacketConfirmTransaction`, `readPacketData`):
//!
//! | field | width | notes |
//! |---|---|---|
//! | window id | i8 | |
//! | action number | i16 | |
//! | accepted | i8 | `0` = rejected echo, non-zero = accepted |
//!
//! The MCP source reads `accepted` with `readByte() != 0`, not
//! `readBoolean()`.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i16, read_i8, write_i16, write_i8};
use crate::packets::EncodablePacket;

/// `Confirm Transaction` (0x0F, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfirmTransaction {
    /// The window id.
    pub window_id: i8,
    /// The action number of the original `Click Window`.
    pub action_number: i16,
    /// Whether the transaction is accepted.
    pub accepted: bool,
}

impl ConfirmTransaction {
    /// Wire packet id.
    pub const ID: u8 = 0x0F;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        let window_id = read_i8(reader)?;
        let action_number = read_i16(reader)?;
        let accepted_byte = read_i8(reader)?;
        Ok(Self {
            window_id,
            action_number,
            accepted: accepted_byte != 0,
        })
    }
}

impl EncodablePacket for ConfirmTransaction {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_i8(writer, self.window_id)?;
        write_i16(writer, self.action_number)?;
        write_i8(writer, if self.accepted { 1 } else { 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_transaction_round_trips() {
        let packet = ConfirmTransaction {
            window_id: 0,
            action_number: 5,
            accepted: true,
        };
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        assert_eq!(ConfirmTransaction::decode(&mut &buf[..]).unwrap(), packet);
    }

    #[test]
    fn a_rejection_echo_round_trips() {
        let packet = ConfirmTransaction {
            window_id: 2,
            action_number: -3,
            accepted: false,
        };
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        assert_eq!(ConfirmTransaction::decode(&mut &buf[..]).unwrap(), packet);
    }
}
