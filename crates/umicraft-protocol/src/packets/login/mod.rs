// SPDX-License-Identifier: MIT OR Apache-2.0

//! Login state (`State::Login`): offline join and the online-mode key
//! exchange, six packets total.
//!
//! Direction convention: `clientbound` flows server → client (inbound on a
//! client), `serverbound` flows client → server (outbound on a client).
//!
//! Offline flow: `Login Start` → (`Set Compression`) → `Login Success`.
//! Online flow inserts `Encryption Request`/`Encryption Response` before
//! `Login Success`; a refusal arrives as `Disconnect`.

pub mod clientbound;
pub mod serverbound;

use std::io::{Read, Write};

use crate::error::ProtocolError;

use super::EncodablePacket;

/// Clientbound login-state packets (inbound for a client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginClientbound {
    /// `Disconnect` (0x00): kick reason as chat JSON.
    Disconnect(clientbound::Disconnect),
    /// `Encryption Request` (0x01): signals an online-mode server.
    EncryptionRequest(clientbound::EncryptionRequest),
    /// `Login Success` (0x02): authentication completed.
    LoginSuccess(clientbound::LoginSuccess),
    /// `Set Compression` (0x03): enables zlib framing from now on.
    SetCompression(clientbound::SetCompression),
}

impl LoginClientbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::Disconnect(_) => clientbound::Disconnect::ID,
            Self::EncryptionRequest(_) => clientbound::EncryptionRequest::ID,
            Self::LoginSuccess(_) => clientbound::LoginSuccess::ID,
            Self::SetCompression(_) => clientbound::SetCompression::ID,
        }
    }

    /// Writes the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on invalid fields or stream errors.
    pub fn encode_body<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        match self {
            Self::Disconnect(packet) => packet.encode_body(writer),
            Self::EncryptionRequest(packet) => packet.encode_body(writer),
            Self::LoginSuccess(packet) => packet.encode_body(writer),
            Self::SetCompression(packet) => packet.encode_body(writer),
        }
    }

    /// Reads a body for the given packet id.
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for unrecognized ids.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            clientbound::Disconnect::ID => {
                Self::Disconnect(clientbound::Disconnect::decode(reader)?)
            }
            clientbound::EncryptionRequest::ID => {
                Self::EncryptionRequest(clientbound::EncryptionRequest::decode(reader)?)
            }
            clientbound::LoginSuccess::ID => {
                Self::LoginSuccess(clientbound::LoginSuccess::decode(reader)?)
            }
            clientbound::SetCompression::ID => {
                Self::SetCompression(clientbound::SetCompression::decode(reader)?)
            }
            _ => {
                return Err(ProtocolError::UnknownPacket {
                    id,
                    state: crate::State::Login,
                })
            }
        })
    }
}

/// Serverbound login-state packets (outbound for a client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginServerbound {
    /// `Login Start` (0x00): the chosen username.
    LoginStart(serverbound::LoginStart),
    /// `Encryption Response` (0x01): RSA-wrapped shared secret + verify token.
    EncryptionResponse(serverbound::EncryptionResponse),
}

impl LoginServerbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::LoginStart(_) => serverbound::LoginStart::ID,
            Self::EncryptionResponse(_) => serverbound::EncryptionResponse::ID,
        }
    }

    /// Reads a body for the given packet id.
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for unrecognized ids.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            serverbound::LoginStart::ID => {
                Self::LoginStart(serverbound::LoginStart::decode(reader)?)
            }
            serverbound::EncryptionResponse::ID => {
                Self::EncryptionResponse(serverbound::EncryptionResponse::decode(reader)?)
            }
            _ => {
                return Err(ProtocolError::UnknownPacket {
                    id,
                    state: crate::State::Login,
                })
            }
        })
    }

    /// Writes the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on invalid fields or stream errors.
    pub fn encode_body<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        match self {
            Self::LoginStart(packet) => packet.encode_body(writer),
            Self::EncryptionResponse(packet) => packet.encode_body(writer),
        }
    }
}

impl EncodablePacket for LoginClientbound {
    fn packet_id(&self) -> u8 {
        LoginClientbound::packet_id(self)
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        LoginClientbound::encode_body(self, writer)
    }
}

impl EncodablePacket for LoginServerbound {
    fn packet_id(&self) -> u8 {
        LoginServerbound::packet_id(self)
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        LoginServerbound::encode_body(self, writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_rejects_unknown_ids() {
        let mut empty: &[u8] = &[];
        let err = LoginClientbound::decode(0x42, &mut empty).unwrap_err();
        assert!(matches!(err, ProtocolError::UnknownPacket { id: 0x42, .. }));

        let mut empty: &[u8] = &[];
        let err = LoginServerbound::decode(0x42, &mut empty).unwrap_err();
        assert!(matches!(err, ProtocolError::UnknownPacket { id: 0x42, .. }));
    }

    /// Serverbound packets must decode back from their own encodings
    /// (symmetry contract the fake server depends on).
    #[test]
    fn serverbound_round_trip_symmetry() {
        let packets = [
            (
                serverbound::LoginStart::ID,
                LoginServerbound::LoginStart(serverbound::LoginStart {
                    username: "Alice".to_string(),
                }),
            ),
            (
                serverbound::EncryptionResponse::ID,
                LoginServerbound::EncryptionResponse(serverbound::EncryptionResponse {
                    shared_secret: vec![9u8; 16],
                    nonce: vec![7u8; 128],
                }),
            ),
        ];
        for (id, packet) in packets {
            let mut buffer = Vec::new();
            packet.encode_body(&mut buffer).unwrap();
            assert_eq!(
                LoginServerbound::decode(id, &mut &buffer[..]).unwrap(),
                packet
            );
        }
    }
}
