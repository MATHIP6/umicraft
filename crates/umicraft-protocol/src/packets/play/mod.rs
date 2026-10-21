// SPDX-License-Identifier: MIT OR Apache-2.0

//! Play state (`State::Play`): the minimal packet set needed to join a 1.8
//! server and stay connected, plus the M1 acceptance traffic.
//!
//! Direction convention: `clientbound` flows server → client (inbound on a
//! client), `serverbound` flows client → server (outbound on a client).
//!
//! Clientbound: `Keep Alive` (0x00), `Join Game` (0x01), `Chat Message`
//! (0x02), `Player Position and Look` (0x08), `Disconnect` (0x40).
//! Serverbound: `Keep Alive` (0x00), `Chat Message` (0x01), `Player Position
//! and Look` (0x06), `Client Settings` (0x15), `Plugin Message` (0x17).
//!
//! Ids outside this subset are rejected as [`ProtocolError::UnknownPacket`]
//! and skipped by the caller (see `umicraft-net`'s play loop).

pub mod clientbound;
pub mod serverbound;

use std::io::{Read, Write};

use crate::error::ProtocolError;

use super::EncodablePacket;

/// Clientbound play-state packets (inbound for a client).
#[derive(Debug, Clone, PartialEq)]
pub enum PlayClientbound {
    /// `Keep Alive` (0x00): must be echoed with the same id.
    KeepAlive(clientbound::KeepAlive),
    /// `Join Game` (0x01): entry into the world.
    JoinGame(clientbound::JoinGame),
    /// `Chat Message` (0x02): chat/system JSON.
    Chat(clientbound::Chat),
    /// `Player Position and Look` (0x08): teleport / initial placement.
    PositionLook(clientbound::PositionLook),
    /// `Disconnect` (0x40): reason as chat JSON; the socket closes after.
    Disconnect(clientbound::Disconnect),
}

impl PlayClientbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::KeepAlive(_) => clientbound::KeepAlive::ID,
            Self::JoinGame(_) => clientbound::JoinGame::ID,
            Self::Chat(_) => clientbound::Chat::ID,
            Self::PositionLook(_) => clientbound::PositionLook::ID,
            Self::Disconnect(_) => clientbound::Disconnect::ID,
        }
    }

    /// Writes the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on invalid fields or stream errors.
    pub fn encode_body<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        match self {
            Self::KeepAlive(packet) => packet.encode_body(writer),
            Self::JoinGame(packet) => packet.encode_body(writer),
            Self::Chat(packet) => packet.encode_body(writer),
            Self::PositionLook(packet) => packet.encode_body(writer),
            Self::Disconnect(packet) => packet.encode_body(writer),
        }
    }

    /// Reads a body for the given packet id.
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for ids outside the M1 subset.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            clientbound::KeepAlive::ID => Self::KeepAlive(clientbound::KeepAlive::decode(reader)?),
            clientbound::JoinGame::ID => Self::JoinGame(clientbound::JoinGame::decode(reader)?),
            clientbound::Chat::ID => Self::Chat(clientbound::Chat::decode(reader)?),
            clientbound::PositionLook::ID => {
                Self::PositionLook(clientbound::PositionLook::decode(reader)?)
            }
            clientbound::Disconnect::ID => {
                Self::Disconnect(clientbound::Disconnect::decode(reader)?)
            }
            _ => {
                return Err(ProtocolError::UnknownPacket {
                    id,
                    state: crate::State::Play,
                })
            }
        })
    }
}

/// Serverbound play-state packets needed for the M1 acceptance criteria.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayServerbound {
    /// `Keep Alive` (0x00): echo of the server's id.
    KeepAlive(serverbound::KeepAlive),
    /// `Chat Message` (0x01): user-typed text (includes leading slash commands).
    Chat(serverbound::Chat),
    /// `Player Position and Look` (0x06): vanilla's standard teleport answer.
    PositionLook(serverbound::PositionLook),
    /// `Client Settings` (0x15): required before some servers release content.
    ClientSettings(serverbound::ClientSettings),
    /// `Plugin Message` (0x17): client brand channel payload.
    Brand(serverbound::Brand),
}

impl PlayServerbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::KeepAlive(_) => serverbound::KeepAlive::ID,
            Self::Chat(_) => serverbound::Chat::ID,
            Self::PositionLook(_) => serverbound::PositionLook::ID,
            Self::ClientSettings(_) => serverbound::ClientSettings::ID,
            Self::Brand(_) => serverbound::Brand::ID,
        }
    }

    /// Reads a body for the given packet id (M1 serverbound subset only).
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for ids outside the M1 subset.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            serverbound::KeepAlive::ID => Self::KeepAlive(serverbound::KeepAlive::decode(reader)?),
            serverbound::Chat::ID => Self::Chat(serverbound::Chat::decode(reader)?),
            serverbound::PositionLook::ID => {
                Self::PositionLook(serverbound::PositionLook::decode(reader)?)
            }
            serverbound::ClientSettings::ID => {
                Self::ClientSettings(serverbound::ClientSettings::decode(reader)?)
            }
            serverbound::Brand::ID => Self::Brand(serverbound::Brand::decode(reader)?),
            _ => {
                return Err(ProtocolError::UnknownPacket {
                    id,
                    state: crate::State::Play,
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
            Self::KeepAlive(packet) => packet.encode_body(writer),
            Self::Chat(packet) => packet.encode_body(writer),
            Self::PositionLook(packet) => packet.encode_body(writer),
            Self::ClientSettings(packet) => packet.encode_body(writer),
            Self::Brand(packet) => packet.encode_body(writer),
        }
    }
}

impl EncodablePacket for PlayClientbound {
    fn packet_id(&self) -> u8 {
        PlayClientbound::packet_id(self)
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        PlayClientbound::encode_body(self, writer)
    }
}

impl EncodablePacket for PlayServerbound {
    fn packet_id(&self) -> u8 {
        PlayServerbound::packet_id(self)
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        PlayServerbound::encode_body(self, writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keepalive_round_trips_across_directions() {
        let sent = PlayServerbound::KeepAlive(serverbound::KeepAlive { id: 424_242 });
        let mut buffer = Vec::new();
        sent.encode_body(&mut buffer).unwrap();
        assert_eq!(
            PlayClientbound::decode(clientbound::KeepAlive::ID, &mut &buffer[..]).unwrap(),
            PlayClientbound::KeepAlive(clientbound::KeepAlive { id: 424_242 })
        );
    }

    #[test]
    fn position_look_roundtrip_through_the_answer_packet() {
        let inbound = PlayClientbound::PositionLook(clientbound::PositionLook {
            x: 0.5,
            y: 64.0,
            z: -0.5,
            yaw: 90.0,
            pitch: 0.0,
            flags: 0,
        });
        let answer = PlayServerbound::PositionLook(serverbound::PositionLook {
            x: 0.5,
            y: 64.0,
            z: -0.5,
            yaw: 90.0,
            pitch: 0.0,
            on_ground: true,
        });
        let mut buffer = Vec::new();
        answer.encode_body(&mut buffer).unwrap();
        assert_eq!(answer.packet_id(), 0x06);

        // The clientbound variant echoes what a server would send for the
        // same place, with a trailing relative-flags byte instead of
        // `on_ground`.
        assert_eq!(inbound.packet_id(), 0x08);
        let mut raw = buffer[..buffer.len() - 1].to_vec();
        raw.push(0);
        let decoded = PlayClientbound::decode(0x08, &mut &raw[..]).unwrap();
        assert_eq!(decoded, inbound);
    }

    #[test]
    fn dispatch_rejects_unknown_play_ids() {
        let mut empty: &[u8] = &[];
        let err = PlayClientbound::decode(0x42, &mut empty).unwrap_err();
        assert!(matches!(err, ProtocolError::UnknownPacket { id: 0x42, .. }));

        let mut empty: &[u8] = &[];
        let err = PlayServerbound::decode(0x42, &mut empty).unwrap_err();
        assert!(matches!(err, ProtocolError::UnknownPacket { id: 0x42, .. }));
    }

    /// Serverbound packets must decode back from their own encodings
    /// (symmetry contract the fake server depends on).
    #[test]
    fn serverbound_round_trip_symmetry() {
        let play = vec![
            (
                serverbound::KeepAlive::ID,
                PlayServerbound::KeepAlive(serverbound::KeepAlive { id: -7 }),
            ),
            (
                serverbound::Chat::ID,
                PlayServerbound::Chat(serverbound::Chat {
                    message: "msg".to_string(),
                }),
            ),
            (
                serverbound::PositionLook::ID,
                PlayServerbound::PositionLook(serverbound::PositionLook {
                    x: 1.5,
                    y: -2.5,
                    z: 300.0,
                    yaw: -90.0,
                    pitch: 12.0,
                    on_ground: false,
                }),
            ),
            (
                serverbound::ClientSettings::ID,
                PlayServerbound::ClientSettings(serverbound::ClientSettings {
                    locale: "de_de".to_string(),
                    view_distance: 12,
                    chat_mode: 2,
                    chat_colors: false,
                    skin_parts: 0b0101,
                }),
            ),
            (
                serverbound::Brand::ID,
                PlayServerbound::Brand(serverbound::Brand {
                    channel: "MC|Brand".to_string(),
                    data: vec![8, b'u', b'm', b'i'],
                }),
            ),
        ];
        for (id, packet) in play {
            let mut buffer = Vec::new();
            packet.encode_body(&mut buffer).unwrap();
            assert_eq!(
                PlayServerbound::decode(id, &mut &buffer[..]).unwrap(),
                packet
            );
        }
    }
}
