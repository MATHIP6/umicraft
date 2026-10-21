// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Ping` (0x01, serverbound): latency probe payload, echoed by `Pong`.
//!
//! Layout (`wiki.vg`, protocol 47): one big-endian i64.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::codec::read_i64;
use crate::packets::EncodablePacket;

/// `Ping` (0x01, serverbound): latency probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ping {
    /// Arbitrary payload the server must copy into the pong.
    pub time: i64,
}

impl Ping {
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

impl EncodablePacket for Ping {
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
    fn ping_round_trip() {
        let packet = Ping { time: -42 };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(Ping::decode(&mut &buffer[..]).unwrap(), packet);
    }
}
