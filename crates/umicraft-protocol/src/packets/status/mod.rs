// SPDX-License-Identifier: MIT OR Apache-2.0

//! Status state (`State::Status`): server-list ping, four packets total.
//!
//! Direction convention for the whole packet tree: `clientbound` flows
//! server → client (inbound on a client), `serverbound` flows client →
//! server (outbound on a client). Names follow the wiki.vg tables, which
//! keeps them aligned with the enums (`StatusClientbound`, ...).
//!
//! Wire order for a ping: `Status Request` → `Status Response` → `Ping` →
//! `Pong`, then the connection closes.

pub mod clientbound;
pub mod serverbound;

use std::io::{Read, Write};

use crate::error::ProtocolError;

use super::EncodablePacket;

/// Clientbound status-state packets (inbound for a client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusClientbound {
    /// `Status Response` (0x00): the server list payload as compact JSON.
    Response(clientbound::Response),
    /// `Pong` (0x01): echoes the `Ping` payload.
    Pong(clientbound::Pong),
}

impl StatusClientbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::Response(_) => clientbound::Response::ID,
            Self::Pong(_) => clientbound::Pong::ID,
        }
    }

    /// Writes the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on invalid fields or stream errors.
    pub fn encode_body<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        match self {
            Self::Response(packet) => packet.encode_body(writer),
            Self::Pong(packet) => packet.encode_body(writer),
        }
    }

    /// Reads a body for the given packet id.
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for unrecognized ids.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            clientbound::Response::ID => Self::Response(clientbound::Response::decode(reader)?),
            clientbound::Pong::ID => Self::Pong(clientbound::Pong::decode(reader)?),
            _ => {
                return Err(ProtocolError::UnknownPacket {
                    id,
                    state: crate::State::Status,
                })
            }
        })
    }
}

/// Serverbound status-state packets (outbound for a client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusServerbound {
    /// `Status Request` (0x00): empty body.
    StatusRequest(serverbound::Request),
    /// `Ping` (0x01): latency probe payload, echoed by `Pong`.
    Ping(serverbound::Ping),
}

impl StatusServerbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::StatusRequest(_) => serverbound::Request::ID,
            Self::Ping(_) => serverbound::Ping::ID,
        }
    }

    /// Reads a body for the given packet id.
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for unrecognized ids.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            serverbound::Request::ID => Self::StatusRequest(serverbound::Request::decode(reader)?),
            serverbound::Ping::ID => Self::Ping(serverbound::Ping::decode(reader)?),
            _ => {
                return Err(ProtocolError::UnknownPacket {
                    id,
                    state: crate::State::Status,
                })
            }
        })
    }

    /// Writes the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on stream errors.
    pub fn encode_body<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        match self {
            Self::StatusRequest(packet) => packet.encode_body(writer),
            Self::Ping(packet) => packet.encode_body(writer),
        }
    }
}

impl EncodablePacket for StatusClientbound {
    fn packet_id(&self) -> u8 {
        StatusClientbound::packet_id(self)
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        StatusClientbound::encode_body(self, writer)
    }
}

impl EncodablePacket for StatusServerbound {
    fn packet_id(&self) -> u8 {
        StatusServerbound::packet_id(self)
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        StatusServerbound::encode_body(self, writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_rejects_unknown_ids() {
        let mut empty: &[u8] = &[];
        let err = StatusClientbound::decode(0x42, &mut empty).unwrap_err();
        assert!(matches!(err, ProtocolError::UnknownPacket { id: 0x42, .. }));

        let mut empty: &[u8] = &[];
        let err = StatusServerbound::decode(0x42, &mut empty).unwrap_err();
        assert!(matches!(err, ProtocolError::UnknownPacket { id: 0x42, .. }));
    }

    /// Serverbound packets must decode back from their own encodings
    /// (symmetry contract the fake server depends on).
    #[test]
    fn serverbound_round_trip_symmetry() {
        let packets = [
            (
                serverbound::Request::ID,
                StatusServerbound::StatusRequest(serverbound::Request),
            ),
            (
                serverbound::Ping::ID,
                StatusServerbound::Ping(serverbound::Ping { time: 123_456_789 }),
            ),
        ];
        for (id, packet) in packets {
            let mut buffer = Vec::new();
            packet.encode_body(&mut buffer).unwrap();
            assert_eq!(
                StatusServerbound::decode(id, &mut &buffer[..]).unwrap(),
                packet
            );
        }
    }
}
