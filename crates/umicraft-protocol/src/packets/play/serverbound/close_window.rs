// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Close Window` (0x0D, serverbound, play): the client closes a container.
//!
//! Layout (MCP `C0DPacketCloseWindow`, `readPacketData`): one signed byte,
//! the window id.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::play::wire::{read_i8, write_i8};
use crate::packets::EncodablePacket;

/// `Close Window` (0x0D, serverbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseWindow {
    /// The window id; `0` is the player inventory.
    pub window_id: i8,
}

impl CloseWindow {
    /// Wire packet id.
    pub const ID: u8 = 0x0D;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            window_id: read_i8(reader)?,
        })
    }
}

impl EncodablePacket for CloseWindow {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        write_i8(writer, self.window_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_window_round_trips() {
        let packet = CloseWindow { window_id: 3 };
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        assert_eq!(CloseWindow::decode(&mut &buf[..]).unwrap(), packet);
    }
}
