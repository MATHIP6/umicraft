// SPDX-License-Identifier: MIT OR Apache-2.0

//! Packet definitions and codecs for the M1 protocol subset.
//!
//! Covers the full handshake/status flow, offline login, and the minimal play
//! packet set required to join a 1.8 server and stay alive: `Keep Alive`,
//! `Join Game`, `Disconnect`, `Player Position and Look`, chat, client
//! settings, and the brand plugin message.
//!
//! Packet ids follow the 1.8 (protocol 47) numbering from the community
//! protocol docs; directions differ (there are two tables per state).
//!
//! Encoding contract: `encode_body` writes the id's *body* only — the id itself
//! travels via [`packet_id`], and the outer length/compression layers live in
//! [`crate::framing`].

use std::io::{Read, Write};

use crate::error::ProtocolError;
use crate::types::{read_string_capped, write_string_capped};
use crate::varint::{read_var_i32, write_var_u32};

pub mod codec;

pub use codec::{
    read_bool, read_byte_array, read_f32, read_f64, read_i32, read_i64, read_rest, read_u8,
};

/// Handshake packet (`Handshake`, id 0x00) sent during state `Handshake`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handshake {
    /// Intended protocol version; 1.8 clients send 47.
    pub protocol_version: i32,
    /// Server address as typed by the player (empty on dedicated proxies).
    pub server_address: String,
    /// Server port.
    pub server_port: u16,
    /// Next protocol state: 1 = status, 2 = login.
    pub next_state: i32,
}

impl Handshake {
    /// Id of the Handshake packet.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        0x00
    }

    /// Writes the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on invalid fields or stream errors.
    pub fn encode_body<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        write_var_u32(writer, self.protocol_version as u32)?;
        write_string_capped(writer, &self.server_address)?;
        writer.write_all(&self.server_port.to_be_bytes())?;
        write_var_u32(writer, self.next_state as u32)?;
        Ok(())
    }

    /// Reads the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on invalid fields or truncation.
    pub fn decode<R: Read>(reader: &mut R) -> Result<Self, ProtocolError> {
        let protocol_version = read_var_i32(reader)?;
        let server_address = read_string_capped(reader)?;
        let mut port = [0u8; 2];
        reader.read_exact(&mut port)?;
        let next_state = read_var_i32(reader)?;
        if !matches!(next_state, 1 | 2) {
            return Err(ProtocolError::Invariant(format!(
                "handshake next_state {next_state}"
            )));
        }
        Ok(Self {
            protocol_version,
            server_address,
            server_port: u16::from_be_bytes(port),
            next_state,
        })
    }
}

/// Clientbound status-state packets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusClientbound {
    /// `Status Response` (0x00): the server list payload as compact JSON.
    Response {
        /// Raw JSON body (`version`, `players`, `description`, ...).
        json: String,
    },
    /// `Pong` (0x01): echoes the `Ping` payload.
    Pong {
        /// Value copied from the ping packet.
        time: i64,
    },
}

impl StatusClientbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::Response { .. } => 0x00,
            Self::Pong { .. } => 0x01,
        }
    }

    /// Writes the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on invalid fields or stream errors.
    pub fn encode_body<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        match self {
            Self::Response { json } => write_string_capped(writer, json),
            Self::Pong { time } => {
                writer.write_all(&time.to_be_bytes())?;
                Ok(())
            }
        }
    }

    /// Reads a body for the given packet id.
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for unrecognized ids.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            0x00 => Self::Response {
                json: crate::types::read_string(reader, crate::types::MAX_STRING_CHARS)?,
            },
            0x01 => Self::Pong {
                time: read_i64(reader)?,
            },
            _ => {
                return Err(ProtocolError::UnknownPacket {
                    id,
                    state: crate::State::Status,
                })
            }
        })
    }
}

/// Serverbound status-state packets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusServerbound {
    /// `Status Request` (0x00): empty body.
    StatusRequest,
    /// `Ping` (0x01): latency probe payload, echoed by `Pong`.
    Ping {
        /// Arbitrary payload the server must copy into the pong.
        time: i64,
    },
}

impl StatusServerbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::StatusRequest => 0x00,
            Self::Ping { .. } => 0x01,
        }
    }

    /// Reads a body for the given packet id.
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for unrecognized ids.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            0x00 => Self::StatusRequest,
            0x01 => Self::Ping {
                time: read_i64(reader)?,
            },
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
            Self::StatusRequest => Ok(()),
            Self::Ping { time } => {
                writer.write_all(&time.to_be_bytes())?;
                Ok(())
            }
        }
    }
}

/// Clientbound login-state packets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginClientbound {
    /// `Disconnect` (0x00): kick reason as chat JSON.
    Disconnect {
        /// Chat JSON body of the kick reason.
        reason: String,
    },
    /// `Encryption Request` (0x01): signals an online-mode server.
    EncryptionRequest {
        /// Usually empty since 1.7; authenticates the request token narrative.
        server_id: String,
        /// DER-encoded X.509 SubjectPublicKeyInfo of the server's RSA key.
        public_key: Vec<u8>,
        /// Verify token to be returned unmodified after encryption.
        verify_token: Vec<u8>,
    },
    /// `Login Success` (0x02): authentication completed.
    LoginSuccess {
        /// Canonical string form of the player UUID.
        uuid: String,
        /// Profile username used for the session.
        username: String,
    },
    /// `Set Compression` (0x03): enables zlib framing from now on.
    SetCompression {
        /// Threshold in bytes; 0 disables compression entirely.
        threshold: i32,
    },
}

impl LoginClientbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::Disconnect { .. } => 0x00,
            Self::EncryptionRequest { .. } => 0x01,
            Self::LoginSuccess { .. } => 0x02,
            Self::SetCompression { .. } => 0x03,
        }
    }

    /// Writes the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on invalid fields or stream errors.
    pub fn encode_body<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        match self {
            Self::Disconnect { reason } => write_string_capped(writer, reason),
            Self::EncryptionRequest {
                server_id,
                public_key,
                verify_token,
            } => {
                crate::types::write_string(writer, server_id, 20)?;
                crate::varint::write_var_u32(writer, public_key.len() as u32)?;
                writer.write_all(public_key)?;
                crate::varint::write_var_u32(writer, verify_token.len() as u32)?;
                writer.write_all(verify_token)?;
                Ok(())
            }
            Self::LoginSuccess { uuid, username } => {
                crate::types::write_string(writer, uuid, 36)?;
                crate::types::write_string(writer, username, 16)
            }
            Self::SetCompression { threshold } => write_var_u32(writer, *threshold as u32),
        }
    }

    /// Reads a body for the given packet id.
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for unrecognized ids.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            0x00 => Self::Disconnect {
                reason: crate::types::read_string(reader, crate::types::MAX_STRING_CHARS)?,
            },
            0x01 => Self::EncryptionRequest {
                server_id: crate::types::read_string(reader, 20)?,
                public_key: read_byte_array(reader, 8192)?,
                verify_token: read_byte_array(reader, 128)?,
            },
            0x02 => Self::LoginSuccess {
                uuid: crate::types::read_string(reader, 36)?,
                username: crate::types::read_string(reader, 16)?,
            },
            0x03 => Self::SetCompression {
                threshold: read_var_i32(reader)?,
            },
            _ => {
                return Err(ProtocolError::UnknownPacket {
                    id,
                    state: crate::State::Login,
                })
            }
        })
    }
}

/// Serverbound login-state packets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginServerbound {
    /// `Login Start` (0x00): the chosen username.
    LoginStart {
        /// Profile username.
        username: String,
    },
    /// `Encryption Response` (0x01): RSA-wrapped shared secret + verify token.
    EncryptionResponse {
        /// Shared secret encrypted with the server's public key (128 bytes).
        shared_secret: Vec<u8>,
        /// Verify token encrypted with the same key (16 bytes vanilla).
        nonce: Vec<u8>,
    },
}

impl LoginServerbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::LoginStart { .. } => 0x00,
            Self::EncryptionResponse { .. } => 0x01,
        }
    }

    /// Reads a body for the given packet id.
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for unrecognized ids.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            0x00 => Self::LoginStart {
                username: crate::types::read_string(reader, 16)?,
            },
            0x01 => Self::EncryptionResponse {
                shared_secret: read_byte_array(reader, 256)?,
                nonce: read_byte_array(reader, 256)?,
            },
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
            Self::LoginStart { username } => write_string_capped(writer, username),
            Self::EncryptionResponse {
                shared_secret,
                nonce,
            } => {
                crate::varint::write_var_u32(writer, shared_secret.len() as u32)?;
                writer.write_all(shared_secret)?;
                crate::varint::write_var_u32(writer, nonce.len() as u32)?;
                writer.write_all(nonce)?;
                Ok(())
            }
        }
    }
}

/// Minimal play-state packets needed to join a server and stay connected.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayClientbound {
    /// `Keep Alive` (0x00): must be echoed with the same id.
    KeepAlive {
        /// Id as a VarInt int in 1.8 (`S00PacketKeepAlive` reads it via
        /// `readVarIntFromBuffer`); the 64-bit id is a 1.12.2+ change.
        id: i32,
    },
    /// `Join Game` (0x01): entry into the world.
    JoinGame {
        /// The player's entity id (plain 4-byte Int in 1.8).
        entity_id: i32,
        /// Raw gamemode byte (hardcore bit 8 folded in by 1.8).
        gamemode: u8,
        /// Dimension id (0 overworld; 1.8 sends it as a signed byte).
        dimension: i8,
        /// Difficulty level byte.
        difficulty: u8,
        /// Player cap advertised by the server.
        max_players: u8,
        /// World generator name, e.g. `flat` on test worlds.
        level_type: String,
        /// 1.8 trailing flag disabling the debug F3 pie chart overlay.
        reduced_debug_info: bool,
    },
    /// `Chat Message` (0x02): chat/system JSON.
    Chat {
        /// Raw chat JSON (`{"text": ...}` or component tree).
        json: String,
        /// System-bit field raw u8 (1.8 uses an i8 message filter).
        position: u8,
    },
    /// `Player Position and Look` (0x08): teleport / initial placement.
    PositionLook {
        /// World X.
        x: f64,
        /// World Y.
        y: f64,
        /// World Z.
        z: f64,
        /// Yaw in degrees.
        yaw: f32,
        /// Pitch in degrees.
        pitch: f32,
        /// Relative bitfield.
        flags: u8,
    },
    /// `Disconnect` (0x40): reason as chat JSON; the socket closes after.
    Disconnect {
        /// Kick reason JSON.
        reason: String,
    },
}

impl PlayClientbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::KeepAlive { .. } => 0x00,
            Self::JoinGame { .. } => 0x01,
            Self::Chat { .. } => 0x02,
            Self::PositionLook { .. } => 0x08,
            Self::Disconnect { .. } => 0x40,
        }
    }

    /// Writes the body.
    ///
    /// # Errors
    /// [`ProtocolError`] on invalid fields or stream errors.
    pub fn encode_body<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        match self {
            Self::KeepAlive { id } => {
                write_var_u32(writer, *id as u32)?;
                Ok(())
            }
            Self::JoinGame {
                entity_id,
                gamemode,
                dimension,
                difficulty,
                max_players,
                level_type,
                reduced_debug_info,
            } => {
                // Entity id is a plain big-endian Int in 1.8 (not a VarInt).
                writer.write_all(&entity_id.to_be_bytes())?;
                writer.write_all(&[*gamemode, *dimension as u8, *difficulty, *max_players])?;
                crate::types::write_string(writer, level_type, 16)?;
                writer.write_all(&[u8::from(*reduced_debug_info)])?;
                Ok(())
            }
            Self::Chat { json, position } => {
                write_string_capped(writer, json)?;
                writer.write_all(&[*position])?;
                Ok(())
            }
            Self::PositionLook {
                x,
                y,
                z,
                yaw,
                pitch,
                flags,
            } => {
                writer.write_all(&x.to_be_bytes())?;
                writer.write_all(&y.to_be_bytes())?;
                writer.write_all(&z.to_be_bytes())?;
                writer.write_all(&yaw.to_be_bytes())?;
                writer.write_all(&pitch.to_be_bytes())?;
                writer.write_all(&[*flags])?;
                Ok(())
            }
            Self::Disconnect { reason } => write_string_capped(writer, reason),
        }
    }

    /// Reads a body for the given packet id.
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for ids outside the M1 subset.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            0x00 => Self::KeepAlive {
                id: read_var_i32(reader)?,
            },
            0x01 => Self::JoinGame {
                entity_id: read_i32(reader)?,
                gamemode: read_u8(reader)?,
                dimension: read_u8(reader)? as i8,
                difficulty: read_u8(reader)?,
                max_players: read_u8(reader)?,
                level_type: crate::types::read_string(reader, 16)?,
                reduced_debug_info: read_bool(reader)?,
            },
            0x02 => Self::Chat {
                json: crate::types::read_string(reader, crate::types::MAX_STRING_CHARS)?,
                position: read_u8(reader)?,
            },
            0x08 => Self::PositionLook {
                x: read_f64(reader)?,
                y: read_f64(reader)?,
                z: read_f64(reader)?,
                yaw: read_f32(reader)?,
                pitch: read_f32(reader)?,
                flags: read_u8(reader)?,
            },
            0x40 => Self::Disconnect {
                reason: crate::types::read_string(reader, crate::types::MAX_STRING_CHARS)?,
            },
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
    KeepAlive {
        /// Id copied from the server's packet (VarInt in 1.8).
        id: i32,
    },
    /// `Chat Message` (0x01): user-typed text (includes leading slash commands).
    Chat {
        /// Raw text (1.8 flower up to 256 chars).
        message: String,
    },
    /// `Player Position and Look` (0x06): vanilla's standard teleport answer.
    PositionLook {
        /// Accepted world X.
        x: f64,
        /// Accepted world Y.
        y: f64,
        /// Accepted world Z.
        z: f64,
        /// Accepted yaw.
        yaw: f32,
        /// Accepted pitch.
        pitch: f32,
        /// True when the client claims to be standing on ground.
        on_ground: bool,
    },
    /// `Client Settings` (0x15): required before some servers release content.
    ClientSettings {
        /// e.g. `en_us` (server reads at most 7 chars in 1.8).
        locale: String,
        /// Render distance the client accepts (chunks).
        view_distance: u8,
        /// Raw chat-visibility byte (`C15PacketClientSettings` reads a byte).
        chat_mode: u8,
        /// Whether colors are enabled in chat.
        chat_colors: bool,
        /// Raw displayed-skin-part mask.
        skin_parts: u8,
    },
    /// `Plugin Message` (0x17): client brand channel payload.
    Brand {
        /// Channel name, normally `MC|Brand`.
        channel: String,
        /// Raw channel payload (everything after the channel string; the
        /// 1.8 packet does not carry an inner length prefix itself).
        data: Vec<u8>,
    },
}

impl PlayServerbound {
    /// Wire packet id.
    #[must_use]
    pub const fn packet_id(&self) -> u8 {
        match self {
            Self::KeepAlive { .. } => 0x00,
            Self::Chat { .. } => 0x01,
            Self::PositionLook { .. } => 0x06,
            Self::ClientSettings { .. } => 0x15,
            Self::Brand { .. } => 0x17,
        }
    }

    /// Reads a body for the given packet id (M1 serverbound subset only).
    ///
    /// # Errors
    /// [`ProtocolError::UnknownPacket`] for ids outside the M1 subset.
    pub fn decode(id: u8, reader: &mut impl Read) -> Result<Self, ProtocolError> {
        Ok(match id {
            0x00 => Self::KeepAlive {
                id: read_var_i32(reader)?,
            },
            0x01 => Self::Chat {
                message: crate::types::read_string(reader, crate::types::MAX_STRING_CHARS)?,
            },
            0x06 => Self::PositionLook {
                x: read_f64(reader)?,
                y: read_f64(reader)?,
                z: read_f64(reader)?,
                yaw: read_f32(reader)?,
                pitch: read_f32(reader)?,
                on_ground: read_bool(reader)?,
            },
            0x15 => Self::ClientSettings {
                locale: crate::types::read_string(reader, 7)?,
                view_distance: read_u8(reader)?,
                chat_mode: read_u8(reader)?,
                chat_colors: read_bool(reader)?,
                skin_parts: read_u8(reader)?,
            },
            0x17 => Self::Brand {
                // 1.8 caps the channel at 20 chars; the data is simply the
                // rest of the body (`readBytes(readableBytes)` in MCP).
                channel: crate::types::read_string(reader, 20)?,
                data: read_rest(reader)?,
            },
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
            Self::KeepAlive { id } => write_var_u32(writer, *id as u32),
            Self::Chat { message } => write_string_capped(writer, message),
            Self::PositionLook {
                x,
                y,
                z,
                yaw,
                pitch,
                on_ground,
            } => {
                writer.write_all(&x.to_be_bytes())?;
                writer.write_all(&y.to_be_bytes())?;
                writer.write_all(&z.to_be_bytes())?;
                writer.write_all(&yaw.to_be_bytes())?;
                writer.write_all(&pitch.to_be_bytes())?;
                writer.write_all(&[u8::from(*on_ground)])?;
                Ok(())
            }
            Self::ClientSettings {
                locale,
                view_distance,
                chat_mode,
                chat_colors,
                skin_parts,
            } => {
                write_string_capped(writer, locale)?;
                writer.write_all(&[*view_distance, *chat_mode])?;
                writer.write_all(&[u8::from(*chat_colors), *skin_parts])?;
                Ok(())
            }
            Self::Brand { channel, data } => {
                write_string_capped(writer, channel)?;
                // 1.8 sends the payload raw; MC|Brand's own content is a
                // length-prefixed string built by the caller.
                writer.write_all(data)?;
                Ok(())
            }
        }
    }
}

/// Unified write side shared by packet types across states and directions.
///
/// Framing layers ([`crate::Framing`]) and the connection write path only need
/// these two pieces plus a fresh body buffer per packet.
pub trait EncodablePacket {
    /// Wire packet id for the packet's own direction and state.
    fn packet_id(&self) -> u8;

    /// Writes the body only; the caller writes the id via [`Self::packet_id`]
    /// and wraps the bytes in the outer length/compression layers.
    ///
    /// # Errors
    /// [`ProtocolError`] on oversized fields or stream errors.
    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError>;
}

impl EncodablePacket for Handshake {
    fn packet_id(&self) -> u8 {
        Handshake::packet_id(self)
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        Handshake::encode_body(self, writer)
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
    fn handshake_round_trip() {
        let packet = Handshake {
            protocol_version: crate::state::PROTOCOL_VERSION_1_8,
            server_address: "localhost".to_string(),
            server_port: 25565,
            next_state: 2,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();

        let mut reader = &buffer[..];
        let decoded = Handshake::decode(&mut reader).unwrap();
        assert_eq!(decoded, packet);
        assert!(reader.is_empty());
        assert_eq!(packet.packet_id(), 0x00);
    }

    #[test]
    fn invalid_next_state_is_rejected() {
        let mut buffer = Vec::new();
        write_var_u32(&mut buffer, crate::state::PROTOCOL_VERSION_1_8 as u32).unwrap();
        write_string_capped(&mut buffer, "x").unwrap();
        buffer.extend_from_slice(&25565u16.to_be_bytes());
        write_var_u32(&mut buffer, 7).unwrap();
        assert!(Handshake::decode(&mut &buffer[..]).is_err());
    }

    #[test]
    fn login_success_parse() {
        let mut buffer = Vec::new();
        crate::types::write_string_capped(&mut buffer, "00000000-0000-0000-0000-000000000001")
            .unwrap();
        crate::types::write_string_capped(&mut buffer, "Player").unwrap();
        let decoded = LoginClientbound::decode(0x02, &mut &buffer[..]).unwrap();
        match decoded {
            LoginClientbound::LoginSuccess { uuid, username } => {
                assert_eq!(uuid, "00000000-0000-0000-0000-000000000001");
                assert_eq!(username, "Player");
            }
            other => panic!("unexpected decode {other:?}"),
        }
    }

    #[test]
    fn set_compression_and_disconnect_parse() {
        let mut buffer = Vec::new();
        crate::varint::write_var_u32(&mut buffer, 256).unwrap();
        assert!(matches!(
            LoginClientbound::decode(0x03, &mut &buffer[..]).unwrap(),
            LoginClientbound::SetCompression { threshold: 256 }
        ));

        let mut buffer = Vec::new();
        crate::types::write_string_capped(&mut buffer, "{\"text\":\"bye\"}").unwrap();
        assert!(matches!(
            LoginClientbound::decode(0x00, &mut &buffer[..]).unwrap(),
            LoginClientbound::Disconnect { .. }
        ));
    }

    #[test]
    fn keepalive_round_trip() {
        let mut buffer = Vec::new();
        let packet = PlayServerbound::KeepAlive { id: 424_242 };
        packet.encode_body(&mut buffer).unwrap();
        // 1.8 keepalive ids are VarInts (MCP S00PacketKeepAlive), so this id
        // takes 3 bytes, not 8.
        assert_eq!(buffer, [0xB2, 0xF2, 0x19]);
        let mut slice = &buffer[..];
        assert_eq!(
            PlayClientbound::decode(0x00, &mut slice).unwrap(),
            PlayClientbound::KeepAlive { id: 424_242 }
        );
    }

    #[test]
    fn join_game_wire_shape_matches_1_8() {
        // Layout pinned to S01PacketJoinGame (MCP): Int entity id, gamemode
        // byte (hardcore bit 8), byte dimension, u8 difficulty, u8 cap,
        // 16-cap world string, trailing reduced-debug bool.
        let packet = PlayClientbound::JoinGame {
            entity_id: 26,
            gamemode: 0,
            dimension: 0,
            difficulty: 1,
            max_players: 20,
            level_type: "flat".to_string(),
            reduced_debug_info: false,
        };
        let mut expected = Vec::new();
        expected.extend(26i32.to_be_bytes());
        expected.extend([0, 0, 1, 20]);
        crate::types::write_string(&mut expected, "flat", 16).unwrap();
        expected.push(0);

        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(buffer, expected);
        assert_eq!(
            PlayClientbound::decode(0x01, &mut &expected[..]).unwrap(),
            packet
        );

        // Wide and negative values survive the fixed-width Int read.
        let packet = PlayClientbound::JoinGame {
            entity_id: -123_456_789,
            gamemode: 1,
            dimension: -1,
            difficulty: 2,
            max_players: 5,
            level_type: "default".to_string(),
            reduced_debug_info: true,
        };
        let mut buffer = Vec::new();
        packet.encode_body(&mut buffer).unwrap();
        assert_eq!(
            PlayClientbound::decode(0x01, &mut &buffer[..]).unwrap(),
            packet
        );
    }

    #[test]
    fn position_look_roundtrip_through_the_answer_packet() {
        let inbound = PlayClientbound::PositionLook {
            x: 0.5,
            y: 64.0,
            z: -0.5,
            yaw: 90.0,
            pitch: 0.0,
            flags: 0,
        };
        let answer = PlayServerbound::PositionLook {
            x: 0.5,
            y: 64.0,
            z: -0.5,
            yaw: 90.0,
            pitch: 0.0,
            on_ground: true,
        };
        let mut buffer = Vec::new();
        answer.encode_body(&mut buffer).unwrap();
        assert_eq!(answer.packet_id(), 0x06);

        // The clientbound variant echoes what a server would send for the same place.
        assert_eq!(inbound.packet_id(), 0x08);
        let mut raw = Vec::new();
        raw.extend_from_slice(&0.5f64.to_be_bytes());
        raw.extend_from_slice(&64.0f64.to_be_bytes());
        raw.extend_from_slice(&(-0.5f64).to_be_bytes());
        raw.extend_from_slice(&90.0f32.to_be_bytes());
        raw.extend_from_slice(&0.0f32.to_be_bytes());
        raw.push(0);
        let decoded = PlayClientbound::decode(0x08, &mut &raw[..]).unwrap();
        assert_eq!(decoded, inbound);
    }

    #[test]
    fn client_settings_and_brand_encode() {
        let settings = PlayServerbound::ClientSettings {
            locale: "en_us".to_string(),
            view_distance: 8,
            chat_mode: 0,
            chat_colors: true,
            skin_parts: 0b0111_1111,
        };
        let mut buffer = Vec::new();
        settings.encode_body(&mut buffer).unwrap();

        let brand = PlayServerbound::Brand {
            channel: "MC|Brand".to_string(),
            data: b"\x07umicraft".to_vec(),
        };
        assert_eq!(brand.packet_id(), 0x17);
        let mut buffer = Vec::new();
        brand.encode_body(&mut buffer).unwrap();
        // The brand payload boilerplate: string channel + inner-length-prefixed data.
        assert!(buffer.starts_with(b"\x08MC|Brand"));
    }

    #[test]
    fn unknown_play_ids_reject() {
        let mut empty: &[u8] = &[];
        let err = PlayClientbound::decode(0x42, &mut empty).unwrap_err();
        assert!(matches!(err, ProtocolError::UnknownPacket { id: 0x42, .. }));

        let mut empty: &[u8] = &[];
        let err = StatusClientbound::decode(0x42, &mut empty).unwrap_err();
        assert!(matches!(err, ProtocolError::UnknownPacket { id: 0x42, .. }));
    }

    /// Serverbound packets must decode back from their own encodings
    /// (symmetry contract the fake server depends on).
    #[test]
    fn serverbound_round_trip_symmetry() {
        let packets: Vec<(u8, StatusServerbound)> = vec![
            (0x00, StatusServerbound::StatusRequest),
            (0x01, StatusServerbound::Ping { time: 123_456_789 }),
        ];
        for (id, packet) in packets {
            let mut buffer = Vec::new();
            packet.encode_body(&mut buffer).unwrap();
            assert_eq!(
                StatusServerbound::decode(id, &mut &buffer[..]).unwrap(),
                packet
            );
        }

        let mut buffer = Vec::new();
        LoginServerbound::LoginStart {
            username: "Alice".to_string(),
        }
        .encode_body(&mut buffer)
        .unwrap();
        assert_eq!(
            LoginServerbound::decode(0x00, &mut &buffer[..]).unwrap(),
            LoginServerbound::LoginStart {
                username: "Alice".to_string()
            }
        );

        let mut buffer = Vec::new();
        LoginServerbound::EncryptionResponse {
            shared_secret: vec![9u8; 16],
            nonce: vec![7u8; 128],
        }
        .encode_body(&mut buffer)
        .unwrap();
        assert_eq!(
            LoginServerbound::decode(0x01, &mut &buffer[..]).unwrap(),
            LoginServerbound::EncryptionResponse {
                shared_secret: vec![9u8; 16],
                nonce: vec![7u8; 128],
            }
        );

        let play = vec![
            (0x00, PlayServerbound::KeepAlive { id: -7 }),
            (
                0x01,
                PlayServerbound::Chat {
                    message: "msg".to_string(),
                },
            ),
            (
                0x06,
                PlayServerbound::PositionLook {
                    x: 1.5,
                    y: -2.5,
                    z: 300.0,
                    yaw: -90.0,
                    pitch: 12.0,
                    on_ground: false,
                },
            ),
            (
                0x15,
                PlayServerbound::ClientSettings {
                    locale: "de_de".to_string(),
                    view_distance: 12,
                    chat_mode: 2,
                    chat_colors: false,
                    skin_parts: 0b0101,
                },
            ),
            (
                0x17,
                PlayServerbound::Brand {
                    channel: "MC|Brand".to_string(),
                    data: vec![8, b'u', b'm', b'i'],
                },
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
