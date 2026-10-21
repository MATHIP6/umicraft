// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Status Request` (0x00, serverbound): empty body, asks for the server list.

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::packets::EncodablePacket;

/// `Status Request` (0x00, serverbound): zero-byte body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request;

impl Request {
    /// Wire packet id.
    pub const ID: u8 = 0x00;

    /// Reads the (empty) body.
    ///
    /// # Errors
    /// Never: the packet carries no fields.
    pub fn decode(_reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(Self)
    }
}

impl EncodablePacket for Request {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, _writer: &mut impl Write) -> Result<(), ProtocolError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_has_an_empty_body() {
        let mut buffer = Vec::new();
        Request.encode_body(&mut buffer).unwrap();
        assert!(buffer.is_empty());
        assert_eq!(Request::decode(&mut &[][..]).unwrap(), Request);
    }
}
