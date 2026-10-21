// SPDX-License-Identifier: MIT OR Apache-2.0

//! Play-state runner for the M1 acceptance profile: respond to keepalives,
//! record the join/teleport facts, answer teleports, and surface a
//! play-state `Disconnect` as a clean, readable failure (spec NET-6).

use std::time::{Duration, Instant};

use tokio::time;
use umicraft_protocol::{varint::write_var_u32, PlayClientbound, PlayServerbound, State};

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

impl Connection {
    /// Enters play state behavior until `stay_for` elapses (when given) or the
    /// server disconnects.
    ///
    /// Sends `Client Settings` and the `MC|Brand` plugin message on entry,
    /// then reacts to the M1 packet subset and silently tolerates (traces)
    /// everything else so real server chatter cannot kill the connection.
    ///
    /// # Errors
    /// [`NetError::KickedAtPlay`] on a play `Disconnect`, transport/protocol
    /// errors on malformed packets or socket failures.
    pub async fn run_play(
        &mut self,
        stay_for: Option<Duration>,
        options: &PlayOptions,
    ) -> Result<PlaySummary, NetError> {
        let started = Instant::now();
        self.send_entry_packets(options).await?;

        let mut summary = PlaySummary::default();
        let deadline = stay_for.map(|span| time::Instant::now() + span);

        loop {
            if let Some(deadline) = deadline {
                if time::Instant::now() >= deadline {
                    summary.stayed = started.elapsed();
                    return Ok(summary);
                }
            }

            let (id, body) = match deadline {
                Some(deadline) => match time::timeout_at(deadline, self.read_packet()).await {
                    Ok(result) => result?,
                    Err(_elapsed) => break,
                },
                None => self.read_packet().await?,
            };

            match id {
                0x00 => match PlayClientbound::decode(id, &mut body.as_slice())? {
                    PlayClientbound::KeepAlive { id: keepalive_id } => {
                        self.write_packet(&PlayServerbound::KeepAlive { id: keepalive_id })
                            .await?;
                        summary.keepalives_answered += 1;
                    }
                    _ => {
                        return Err(NetError::UnexpectedPacket {
                            state: State::Play,
                            id,
                        });
                    }
                },
                0x01 => match PlayClientbound::decode(id, &mut body.as_slice())? {
                    PlayClientbound::JoinGame {
                        entity_id,
                        gamemode,
                        dimension,
                        difficulty,
                        max_players,
                        level_type,
                    } => {
                        summary.join = Some(JoinGameInfo {
                            entity_id,
                            gamemode,
                            dimension,
                            difficulty,
                            max_players,
                            level_type,
                        });
                    }
                    _ => {
                        return Err(NetError::UnexpectedPacket {
                            state: State::Play,
                            id,
                        });
                    }
                },
                0x02 => match PlayClientbound::decode(id, &mut body.as_slice())? {
                    PlayClientbound::Chat { json, .. } => {
                        tracing::trace!(json = %json, "server chat");
                        summary.chat_messages += 1;
                    }
                    _ => {
                        return Err(NetError::UnexpectedPacket {
                            state: State::Play,
                            id,
                        });
                    }
                },
                0x08 => match PlayClientbound::decode(id, &mut body.as_slice())? {
                    PlayClientbound::PositionLook {
                        x,
                        y,
                        z,
                        yaw,
                        pitch,
                        ..
                    } => {
                        // 1.8 has no teleport confirm packet; the vanilla
                        // client answers with `Player Position and Look`,
                        // which we echo (with absolute values, matching the
                        // flags-0 spawn packet 1.8 servers send first).
                        self.write_packet(&PlayServerbound::PositionLook {
                            x,
                            y,
                            z,
                            yaw,
                            pitch,
                            on_ground: true,
                        })
                        .await?;
                        summary.placement = Some(PlayerPlacement {
                            x,
                            y,
                            z,
                            yaw,
                            pitch,
                        });
                    }
                    _ => {
                        return Err(NetError::UnexpectedPacket {
                            state: State::Play,
                            id,
                        });
                    }
                },
                0x40 => match PlayClientbound::decode(id, &mut body.as_slice())? {
                    PlayClientbound::Disconnect { reason } => {
                        summary.stayed = started.elapsed();
                        return Err(NetError::KickedAtPlay {
                            reason: disconnect_reason(&reason),
                        });
                    }
                    _ => {
                        return Err(NetError::UnexpectedPacket {
                            state: State::Play,
                            id,
                        });
                    }
                },
                _ => {
                    // Tolerant skip: servers stream dozens of play packet
                    // kinds (chunks, entities, particles) outside the M1 set.
                    tracing::trace!(id, bytes = body.len(), "skipped play packet");
                }
            }
        }

        summary.stayed = started.elapsed();
        Ok(summary)
    }

    async fn send_entry_packets(&mut self, options: &PlayOptions) -> Result<(), NetError> {
        self.write_packet(&PlayServerbound::ClientSettings {
            locale: options.locale.clone(),
            view_distance: options.view_distance,
            chat_mode: 0,
            chat_colors: options.chat_colors,
            skin_parts: 0b0111_1111,
        })
        .await?;

        let brand_payload = "umicraft".as_bytes();
        let mut brand = Vec::with_capacity(brand_payload.len() + 2);
        write_var_u32(&mut brand, brand_payload.len() as u32)?;
        brand.extend_from_slice(brand_payload);
        self.write_packet(&PlayServerbound::Brand {
            channel: "MC|Brand".to_string(),
            data: brand,
        })
        .await?;
        Ok(())
    }
}
