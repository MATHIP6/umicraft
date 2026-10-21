// SPDX-License-Identifier: MIT OR Apache-2.0

//! Play-state runner: respond to keepalives, send entry packets, echo
//! teleports, and stream decoded world packets to a caller (spec NET-6).
//!
//! [`PlaySession`] is the shared state machine behind the M1 headless client
//! (which discards events) and the M2 graphical client (which feeds them into
//! `umicraft-world`).

use std::time::{Duration, Instant};

use tokio::time;
use umicraft_protocol::packets::play::clientbound::{
    BlockChange, ChunkData, MapChunkBulk, MultiBlockChange, PositionLook,
};
use umicraft_protocol::packets::play::{clientbound, serverbound};
use umicraft_protocol::{varint::write_var_u32, PlayClientbound, PlayServerbound};

use crate::connection::Connection;
use crate::error::NetError;
use crate::status::disconnect_reason;

/// Facts captured from `Join Game`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JoinGameInfo {
    /// Player's entity id in this world.
    pub entity_id: i32,
    /// Raw gamemode byte (1.8 folds hardcore into it).
    pub gamemode: u8,
    /// Dimension byte.
    pub dimension: i8,
    /// Difficulty byte.
    pub difficulty: u8,
    /// Player cap advertised in the packet.
    pub max_players: u8,
    /// World type string, e.g. `flat`.
    pub level_type: String,
    /// 1.8 trailing reduced-debug flag from `Join Game`.
    pub reduced_debug_info: bool,
}

/// Position from the latest `Player Position and Look`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerPlacement {
    /// X.
    pub x: f64,
    /// Y.
    pub y: f64,
    /// Z.
    pub z: f64,
    /// Yaw.
    pub yaw: f32,
    /// Pitch.
    pub pitch: f32,
}

/// Summary of a play session that ended cleanly (deadline reached).
#[derive(Debug, Clone, Default)]
pub struct PlaySummary {
    /// Facts from the server's `Join Game`, when it arrived.
    pub join: Option<JoinGameInfo>,
    /// Latest accepted placement, when any was sent.
    pub placement: Option<PlayerPlacement>,
    /// Keepalives answered with the echoed id.
    pub keepalives_answered: u64,
    /// Server chat/system messages observed.
    pub chat_messages: u64,
    /// Wall time spent in play state.
    pub stayed: Duration,
}

/// Client settings sent on entering play state.
#[derive(Debug, Clone)]
pub struct PlayOptions {
    /// Locale string, e.g. `en_us`.
    pub locale: String,
    /// Accepted render distance in chunks.
    pub view_distance: u8,
    /// Whether colors are rendered in chat.
    pub chat_colors: bool,
}

impl Default for PlayOptions {
    fn default() -> Self {
        Self {
            locale: "en_us".to_string(),
            view_distance: 8,
            chat_colors: true,
        }
    }
}

/// Decodes one clientbound play packet body, tracing packet id and body
/// length on failure. The framing layer peeled a complete packet, so a
/// decode error means the body shape/length assumptions are wrong — the
/// context pinpoints which managed id disagrees with reality.
fn decode_play(id: u8, body: &[u8]) -> Result<PlayClientbound, NetError> {
    let body_len = body.len();
    let result = PlayClientbound::decode(id, &mut &body[..]);
    if let Err(err) = &result {
        tracing::warn!(packet_id = id, body_len, %err, "play packet decode failed");
    }
    result.map_err(NetError::from)
}

/// One event surfaced by [`PlaySession::next`].
///
/// Keepalives are answered inside the session and never become events; chat is
/// surfaced as raw JSON for callers that want to display or inspect it.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayEvent {
    /// The server entered play state and described the world.
    JoinGame(JoinGameInfo),
    /// Absolute player placement after resolving the 1.8 relative flags.
    Placement(PlayerPlacement),
    /// Chat/system JSON.
    Chat(String),
    /// One chunk column, complete or a section update.
    ChunkData(ChunkData),
    /// Initial bulk chunk batch.
    MapChunkBulk(MapChunkBulk),
    /// One block update.
    BlockChange(BlockChange),
    /// Batched block updates in one chunk.
    MultiBlockChange(MultiBlockChange),
}

/// Streaming play-state state machine.
///
/// Handles the mandatory client behavior (keepalive echoes, entry packets after
/// `Join Game`, teleport answers) and returns every packet the caller needs for
/// world building. Packet ids outside the supported M2 subset are skipped.
#[derive(Debug)]
pub struct PlaySession {
    options: PlayOptions,
    started: Instant,
    entry_sent: bool,
    join: Option<JoinGameInfo>,
    placement: Option<PlayerPlacement>,
    keepalives_answered: u64,
    chat_messages: u64,
}

impl PlaySession {
    /// Creates a session; the stay timer starts immediately.
    #[must_use]
    pub fn new(options: PlayOptions) -> Self {
        Self {
            options,
            started: Instant::now(),
            entry_sent: false,
            join: None,
            placement: None,
            keepalives_answered: 0,
            chat_messages: 0,
        }
    }

    /// Snapshot of what the session has observed so far.
    #[must_use]
    pub fn summary(&self) -> PlaySummary {
        PlaySummary {
            join: self.join.clone(),
            placement: self.placement.clone(),
            keepalives_answered: self.keepalives_answered,
            chat_messages: self.chat_messages,
            stayed: self.started.elapsed(),
        }
    }

    /// Reads and handles packets until one world/chat/join event is produced.
    ///
    /// # Errors
    /// [`NetError::KickedAtPlay`] on a play `Disconnect`, transport/protocol
    /// errors on malformed packets or socket failures.
    pub async fn next(&mut self, connection: &mut Connection) -> Result<PlayEvent, NetError> {
        loop {
            let (id, body) = connection.read_packet().await?;
            let packet = match id {
                clientbound::KeepAlive::ID
                | clientbound::JoinGame::ID
                | clientbound::Chat::ID
                | clientbound::PositionLook::ID
                | clientbound::ChunkData::ID
                | clientbound::MultiBlockChange::ID
                | clientbound::BlockChange::ID
                | clientbound::MapChunkBulk::ID
                | clientbound::Disconnect::ID => decode_play(id, &body)?,
                _ => {
                    tracing::trace!(id, bytes = body.len(), "skipped play packet");
                    continue;
                }
            };

            match packet {
                PlayClientbound::KeepAlive(packet) => {
                    let answer = serverbound::KeepAlive { id: packet.id };
                    connection
                        .write_packet(&PlayServerbound::KeepAlive(answer))
                        .await?;
                    self.keepalives_answered += 1;
                }
                PlayClientbound::JoinGame(packet) => {
                    let info = JoinGameInfo {
                        entity_id: packet.entity_id,
                        gamemode: packet.gamemode,
                        dimension: packet.dimension,
                        difficulty: packet.difficulty,
                        max_players: packet.max_players,
                        level_type: packet.level_type,
                        reduced_debug_info: packet.reduced_debug_info,
                    };
                    if !self.entry_sent {
                        connection.send_entry_packets(&self.options).await?;
                        self.entry_sent = true;
                    }
                    self.join = Some(info.clone());
                    return Ok(PlayEvent::JoinGame(info));
                }
                PlayClientbound::Chat(packet) => {
                    tracing::trace!(json = %packet.json, "server chat");
                    self.chat_messages += 1;
                    return Ok(PlayEvent::Chat(packet.json));
                }
                PlayClientbound::PositionLook(packet) => {
                    let placement = self.resolve_placement(&packet);
                    let answer = serverbound::PositionLook {
                        x: placement.x,
                        y: placement.y,
                        z: placement.z,
                        yaw: placement.yaw,
                        pitch: placement.pitch,
                        on_ground: true,
                    };
                    connection
                        .write_packet(&PlayServerbound::PositionLook(answer))
                        .await?;
                    self.placement = Some(placement.clone());
                    return Ok(PlayEvent::Placement(placement));
                }
                PlayClientbound::ChunkData(packet) => {
                    return Ok(PlayEvent::ChunkData(packet));
                }
                PlayClientbound::MapChunkBulk(packet) => {
                    return Ok(PlayEvent::MapChunkBulk(packet));
                }
                PlayClientbound::BlockChange(packet) => {
                    return Ok(PlayEvent::BlockChange(packet));
                }
                PlayClientbound::MultiBlockChange(packet) => {
                    return Ok(PlayEvent::MultiBlockChange(packet));
                }
                PlayClientbound::Disconnect(packet) => {
                    return Err(NetError::KickedAtPlay {
                        reason: disconnect_reason(&packet.reason),
                    });
                }
            }
        }
    }

    /// Applies the 1.8 relative-position flag bits to the previous placement.
    ///
    /// Flags: X, Y, Z, yaw, pitch (bits 0–4); vanilla accumulates relative
    /// components on the last accepted position.
    fn resolve_placement(&self, packet: &PositionLook) -> PlayerPlacement {
        let previous = self.placement.clone().unwrap_or_default();
        PlayerPlacement {
            x: if packet.flags & 0x01 != 0 {
                previous.x + packet.x
            } else {
                packet.x
            },
            y: if packet.flags & 0x02 != 0 {
                previous.y + packet.y
            } else {
                packet.y
            },
            z: if packet.flags & 0x04 != 0 {
                previous.z + packet.z
            } else {
                packet.z
            },
            yaw: if packet.flags & 0x08 != 0 {
                previous.yaw + packet.yaw
            } else {
                packet.yaw
            },
            pitch: if packet.flags & 0x10 != 0 {
                previous.pitch + packet.pitch
            } else {
                packet.pitch
            },
        }
    }
}

impl Connection {
    /// Enters play state behavior until `stay_for` elapses (when given) or the
    /// server disconnects, discarding world events.
    ///
    /// This is the M1 headless acceptance path; [`PlaySession`] exposes the
    /// same state machine to clients that build a world.
    ///
    /// # Errors
    /// [`NetError::KickedAtPlay`] on a play `Disconnect`, transport/protocol
    /// errors on malformed packets or socket failures.
    pub async fn run_play(
        &mut self,
        stay_for: Option<Duration>,
        options: &PlayOptions,
    ) -> Result<PlaySummary, NetError> {
        let mut session = PlaySession::new(options.clone());
        let deadline = stay_for.map(|span| time::Instant::now() + span);

        loop {
            if let Some(deadline) = deadline {
                if time::Instant::now() >= deadline {
                    break;
                }
            }

            let read = match deadline {
                Some(deadline) => time::timeout_at(deadline, session.next(self)).await,
                None => Ok(session.next(self).await),
            };
            match read {
                Ok(Ok(_event)) => {}
                Ok(Err(err)) => {
                    tracing::warn!(%err, "play: read failed");
                    return Err(err);
                }
                Err(_elapsed) => break,
            }
        }

        Ok(session.summary())
    }

    async fn send_entry_packets(&mut self, options: &PlayOptions) -> Result<(), NetError> {
        let settings = serverbound::ClientSettings {
            locale: options.locale.clone(),
            view_distance: options.view_distance,
            chat_mode: 0,
            chat_colors: options.chat_colors,
            skin_parts: 0b0111_1111,
        };
        self.write_packet(&PlayServerbound::ClientSettings(settings))
            .await?;

        let brand_payload = "umicraft".as_bytes();
        let mut brand = Vec::with_capacity(brand_payload.len() + 2);
        write_var_u32(&mut brand, brand_payload.len() as u32)?;
        brand.extend_from_slice(brand_payload);
        let brand = serverbound::Brand {
            channel: "MC|Brand".to_string(),
            data: brand,
        };
        self.write_packet(&PlayServerbound::Brand(brand)).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(flags: u8) -> PositionLook {
        PositionLook {
            x: 1.5,
            y: 70.0,
            z: -2.5,
            yaw: 15.0,
            pitch: -20.0,
            flags,
        }
    }

    #[test]
    fn relative_flags_accumulate_independently() {
        let mut session = PlaySession::new(PlayOptions::default());
        session.placement = Some(PlayerPlacement {
            x: 10.0,
            y: 64.0,
            z: -5.0,
            yaw: 90.0,
            pitch: 10.0,
        });
        // X, Z and yaw are deltas; Y and pitch stay absolute.
        let placement = session.resolve_placement(&packet(0x01 | 0x04 | 0x08));
        assert_eq!(placement.x, 11.5);
        assert_eq!(placement.y, 70.0);
        assert_eq!(placement.z, -7.5);
        assert_eq!(placement.yaw, 105.0);
        assert_eq!(placement.pitch, -20.0);
    }

    #[test]
    fn clear_flags_replace_with_absolute_values() {
        let mut session = PlaySession::new(PlayOptions::default());
        session.placement = Some(PlayerPlacement {
            x: 10.0,
            y: 64.0,
            z: -5.0,
            yaw: 90.0,
            pitch: 10.0,
        });
        let placement = session.resolve_placement(&packet(0));
        assert_eq!(placement.x, 1.5);
        assert_eq!(placement.y, 70.0);
        assert_eq!(placement.z, -2.5);
        assert_eq!(placement.yaw, 15.0);
        assert_eq!(placement.pitch, -20.0);
    }
}
