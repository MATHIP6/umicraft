// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Close Window` (0x2E, clientbound, play): the server closes a container GUI.
//!
//! Layout (MCP `S2EPacketCloseWindow`, `readPacketData`): one unsigned byte,
//! the window id.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_u8;
use crate::packets::EncodablePacket;

/// `Close Window` (0x2E, clientbound, play).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseWindow {
    /// The window id; `0` is the player inventory.
    pub window_id: u8,
}

impl CloseWindow {
    /// Wire packet id.
    pub const ID: u8 = 0x2E;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            window_id: read_u8(reader)?,
        })
    }
}

impl EncodablePacket for CloseWindow {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[self.window_id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_window_round_trips() {
        let packet = CloseWindow { window_id: 5 };
        let mut buf = Vec::new();
        packet.encode_body(&mut buf).unwrap();
        assert_eq!(CloseWindow::decode(&mut &buf[..]).unwrap(), packet);
    }
}
