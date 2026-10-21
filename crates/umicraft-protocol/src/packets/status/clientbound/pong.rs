// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Pong` (0x01, clientbound): echoes the `Ping` payload byte-exact.
//!
//! Layout (`wiki.vg`, protocol 47): one big-endian i64.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i64;
use crate::packets::EncodablePacket;

/// `Pong` (0x01, clientbound): latency probe answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pong {
    /// Value copied from the ping packet.
    pub time: i64,
}

impl Pong {
    /// Wire packet id.
    pub const ID: u8 = 0x01;

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError::Io`] on stream truncation.
    pub fn decode(reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self {
            time: read_i64(reader)?,
        })
    }
}

impl EncodablePacket for Pong {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&self.time.to_be_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pong_round_trip() {
        let packet = Pong { time: 123_456_789 };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, 123_456_789i64.to_be_bytes());
        assert_eq!(Pong::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
