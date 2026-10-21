// SPDX-License-Identifier: MIT OR Apache-2.0

//! The network thread: the owner of the socket and the decoder of the
//! clientbound play packets.
//!
//! The thread runs a Tokio current-thread runtime, answers the keepalives,
//! sends the entry packets after `Join Game` and forwards every decoded event
//! to the game thread over a bounded channel. The game thread answers with
//! [`ClientCommand`]s, which this thread turns into serverbound packets.
//!
//! The play loop lives here rather than in `umicraft-net` because the frozen
//! `PlaySession` of that crate skips the M3 play packets (`Update Health`,
//! `Held Item Change`, `Respawn`, `Entity Teleport`), and the game thread
//! needs them. The framing, the login and the connection stay in
//! `umicraft-net`.

use std::sync::mpsc::SyncSender;
use std::thread::JoinHandle;

use tokio::sync::mpsc::Receiver as CommandReceiver;
use umicraft_net::client::{JoinGameInfo, PlayerPlacement};
use umicraft_net::login::login_offline;
use umicraft_net::Connection;
use umicraft_protocol::packets::play::clientbound::{
    self, BlockChange, ChunkData, EntityEquipment, MapChunkBulk, MultiBlockChange, PositionLook,
    Respawn, SetExperience, SpawnPosition, UpdateHealth,
};
use umicraft_protocol::packets::play::serverbound::{
    self, Brand, ClientSettings, DiggingStatus, HeldItemChange as ServerHeldItemChange, Input,
    PlayerBlockPlacement, PlayerDigging, PlayerLook, PlayerPosition,
    PositionLook as ServerPositionLook,
};
use umicraft_protocol::packets::play::{Facing, ItemStack};
use umicraft_protocol::varint::write_var_u32;

use crate::error::ClientError;
use crate::game::GameEvent;

/// The number of serverbound packets that may be queued for the socket.
///
/// The game thread produces two or three packets per tick, so a small queue
/// is enough; when it is full the game thread keeps the command and retries
/// on the next tick instead of blocking the simulation.
pub const COMMAND_CAPACITY: usize = 64;

/// Server and identity settings for the network thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkConfig {
    /// Server host name or IP.
    pub host: String,
    /// Server port.
    pub port: u16,
    /// Offline-mode username.
    pub name: String,
    /// Client Settings view distance.
    pub view_distance: u8,
}

/// A decoded clientbound event.
#[derive(Debug)]
pub enum NetEvent {
    /// The server entered play state; start a fresh world for this dimension.
    JoinGame(JoinGameInfo),
    /// Absolute player placement after relative-flag resolution.
    Placement(PlayerPlacement),
    /// Raw chat JSON.
    Chat(String),
    /// One chunk column.
    ChunkData(Box<ChunkData>),
    /// Initial chunk batch.
    MapChunkBulk(Box<MapChunkBulk>),
    /// One block change.
    BlockChange(BlockChange),
    /// Batched block changes.
    MultiBlockChange(MultiBlockChange),
    /// The player's health, food and saturation.
    UpdateHealth(UpdateHealth),
    /// The selected hotbar slot.
    HeldItemChange(i8),
    /// The player changed dimension.
    Respawn(Respawn),
    /// An entity was teleported; only the local player matters.
    EntityTeleport(clientbound::EntityTeleport),
    /// The equipment of an entity; only the local player matters.
    EntityEquipment(EntityEquipment),
    /// The world spawn point.
    SpawnPosition(SpawnPosition),
    /// The experience bar changed.
    SetExperience(SetExperience),
    /// Fatal connection/session error, already formatted for the overlay.
    Error(String),
}

/// A serverbound packet the game thread wants the socket to send.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientCommand {
    /// `Input` (0x0C): the movement keys of the current tick.
    ///
    /// The values are the raw wire values: the sneak scaling of MCP
    /// `MovementInputFromOptions.java` is already applied, the `0.98` damping
    /// of `EntityLivingBase.java:2020` is applied by the game logic and
    /// therefore by the server as well.
    Input {
        /// The strafe axis: positive to the left.
        strafe: f32,
        /// The forward axis: positive forwards.
        forward: f32,
        /// Whether the jump key is held.
        jumping: bool,
        /// Whether the sneak key is held.
        sneaking: bool,
    },
    /// `Player Position` (0x04): the feet position and the ground flag.
    PlayerPosition {
        /// The feet X.
        x: f64,
        /// The feet Y.
        y: f64,
        /// The feet Z.
        z: f64,
        /// Whether the player rests on the ground.
        on_ground: bool,
    },
    /// `Player Look` (0x05): the view angles and the ground flag.
    PlayerLook {
        /// The yaw in degrees.
        yaw: f32,
        /// The pitch in degrees.
        pitch: f32,
        /// Whether the player rests on the ground.
        on_ground: bool,
    },
    /// `Player Position and Look` (0x06): the acknowledgement of a server
    /// placement, or a combined position and look report.
    PositionLook {
        /// The feet X.
        x: f64,
        /// The feet Y.
        y: f64,
        /// The feet Z.
        z: f64,
        /// The yaw in degrees.
        yaw: f32,
        /// The pitch in degrees.
        pitch: f32,
        /// Whether the player rests on the ground.
        on_ground: bool,
    },
    /// `Player Digging` (0x07): the start or the stop of a block break.
    PlayerDigging {
        /// The stage of the interaction.
        status: DiggingStatus,
        /// The block X.
        x: i32,
        /// The block Y.
        y: i32,
        /// The block Z.
        z: i32,
        /// The face of the block that was hit.
        face: Facing,
    },
    /// `Player Block Placement` (0x08): a block placement.
    PlayerBlockPlacement {
        /// The block X.
        x: i32,
        /// The block Y.
        y: i32,
        /// The block Z.
        z: i32,
        /// The face of the block that was hit.
        face: Facing,
        /// The item the player holds.
        item: ItemStack,
        /// The X offset of the click, in sixteenths of a block.
        cursor_x: u8,
        /// The Y offset of the click, in sixteenths of a block.
        cursor_y: u8,
        /// The Z offset of the click, in sixteenths of a block.
        cursor_z: u8,
    },
    /// `Held Item Change` (0x09): the selected hotbar slot.
    HeldItemChange {
        /// The slot index.
        slot: i16,
    },
    /// `Chat Message` (0x01): a line the player typed.
    Chat {
        /// The message, with the leading slash of a command.
        message: String,
    },
}

impl ClientCommand {
    /// The wire packet of the command.
    #[must_use]
    pub fn into_packet(self) -> umicraft_protocol::PlayServerbound {
        match self {
            Self::Input {
                strafe,
                forward,
                jumping,
                sneaking,
            } => umicraft_protocol::PlayServerbound::Input(Input {
                strafe,
                forward,
                jumping,
                sneaking,
            }),
            Self::PlayerPosition { x, y, z, on_ground } => {
                umicraft_protocol::PlayServerbound::PlayerPosition(PlayerPosition {
                    x,
                    y,
                    z,
                    on_ground,
                })
            }
            Self::PlayerLook {
                yaw,
                pitch,
                on_ground,
            } => umicraft_protocol::PlayServerbound::PlayerLook(PlayerLook {
                yaw,
                pitch,
                on_ground,
            }),
            Self::PositionLook {
                x,
                y,
                z,
                yaw,
                pitch,
                on_ground,
            } => umicraft_protocol::PlayServerbound::PositionLook(ServerPositionLook {
                x,
                y,
                z,
                yaw,
                pitch,
                on_ground,
            }),
            Self::PlayerDigging {
                status,
                x,
                y,
                z,
                face,
            } => umicraft_protocol::PlayServerbound::PlayerDigging(PlayerDigging {
                status,
                x,
                y,
                z,
                face,
            }),
            Self::PlayerBlockPlacement {
                x,
                y,
                z,
                face,
                item,
                cursor_x,
                cursor_y,
                cursor_z,
            } => umicraft_protocol::PlayServerbound::PlayerBlockPlacement(PlayerBlockPlacement {
                x,
                y,
                z,
                face,
                item,
                cursor_x,
                cursor_y,
                cursor_z,
            }),
            Self::HeldItemChange { slot } => {
                umicraft_protocol::PlayServerbound::HeldItemChange(ServerHeldItemChange { slot })
            }
            Self::Chat { message } => {
                umicraft_protocol::PlayServerbound::Chat(serverbound::Chat { message })
            }
        }
    }
}

/// Starts the network thread and returns its join handle.
///
/// The thread exits when the connection fails, the server disconnects, or the
/// command channel is closed (which happens when the game thread stops).
///
/// # Errors
/// [`ClientError::Network`] when the OS refuses to start the thread.
pub fn spawn(
    config: NetworkConfig,
    events: SyncSender<GameEvent>,
    commands: CommandReceiver<ClientCommand>,
) -> Result<JoinHandle<()>, ClientError> {
    let handle = std::thread::Builder::new()
        .name("umicraft-net".to_string())
        .spawn(move || run_thread(config, events, commands))
        .map_err(|error| ClientError::Network(format!("spawn network thread: {error}")))?;
    Ok(handle)
}

fn run_thread(
    config: NetworkConfig,
    events: SyncSender<GameEvent>,
    commands: CommandReceiver<ClientCommand>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = events.send(GameEvent::Net(NetEvent::Error(format!(
                "could not start network runtime: {error}"
            ))));
            return;
        }
    };
    runtime.block_on(run_session(config, events, commands));
}

/// The state the play loop keeps between packets.
struct PlayState {
    /// Whether the entry packets were already sent.
    entry_sent: bool,
    /// The view distance announced in the `Client Settings` packet.
    view_distance: u8,
    /// The last placement, used to resolve the relative flags of 0x08.
    placement: Option<PlayerPlacement>,
}

impl PlayState {
    fn new(view_distance: u8) -> Self {
        Self {
            entry_sent: false,
            view_distance,
            placement: None,
        }
    }

    /// Resolves the 1.8 relative-position flag bits against the last
    /// placement.
    ///
    /// Flags: X, Y, Z, yaw, pitch (bits 0-4); vanilla accumulates the
    /// relative components on the last accepted position.
    fn resolve(&self, packet: &PositionLook) -> PlayerPlacement {
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

/// What the play loop received.
enum Incoming {
    /// A command from the game thread; `None` means the game thread stopped.
    Command(Option<ClientCommand>),
    /// A packet from the server.
    Packet(Result<(u8, Vec<u8>), umicraft_net::NetError>),
}

async fn run_session(
    config: NetworkConfig,
    events: SyncSender<GameEvent>,
    mut commands: CommandReceiver<ClientCommand>,
) {
    let mut connection = match Connection::connect(&config.host, config.port).await {
        Ok(connection) => connection,
        Err(error) => {
            let _ = events.send(GameEvent::Net(NetEvent::Error(error.to_string())));
            return;
        }
    };
    let profile = match login_offline(&mut connection, &config.name, None).await {
        Ok(profile) => profile,
        Err(error) => {
            let _ = events.send(GameEvent::Net(NetEvent::Error(error.to_string())));
            return;
        }
    };
    tracing::info!(
        host = %config.host,
        port = config.port,
        username = %profile.username,
        "logged in"
    );

    let mut state = PlayState::new(config.view_distance);
    loop {
        // The read future borrows the connection, so the packet is handled
        // after the select released it.
        let incoming = tokio::select! {
            biased;
            command = commands.recv() => Incoming::Command(command),
            packet = connection.read_packet() => Incoming::Packet(packet),
        };
        match incoming {
            Incoming::Command(None) => return,
            Incoming::Command(Some(command)) => {
                if let Err(error) = connection.write_packet(&command.into_packet()).await {
                    let _ = events.send(GameEvent::Net(NetEvent::Error(error.to_string())));
                    return;
                }
            }
            Incoming::Packet(Err(error)) => {
                let _ = events.send(GameEvent::Net(NetEvent::Error(error.to_string())));
                return;
            }
            Incoming::Packet(Ok((id, body))) => {
                if let Err(error) =
                    handle_packet(&mut connection, id, &body, &mut state, &events).await
                {
                    let _ = events.send(GameEvent::Net(NetEvent::Error(error.to_string())));
                    return;
                }
            }
        }
    }
}

/// Handles one clientbound packet.
///
/// The keepalive is answered on the socket thread; everything else is
/// forwarded to the game thread.
async fn handle_packet(
    connection: &mut Connection,
    id: u8,
    body: &[u8],
    state: &mut PlayState,
    events: &SyncSender<GameEvent>,
) -> Result<(), umicraft_net::NetError> {
    match id {
        clientbound::KeepAlive::ID => {
            let packet = clientbound::KeepAlive::decode(&mut &body[..])?;
            connection
                .write_packet(&umicraft_protocol::PlayServerbound::KeepAlive(
                    serverbound::KeepAlive { id: packet.id },
                ))
                .await?;
        }
        clientbound::JoinGame::ID => {
            let packet = clientbound::JoinGame::decode(&mut &body[..])?;
            if !state.entry_sent {
                send_entry_packets(connection, state.view_distance).await?;
                state.entry_sent = true;
            }
            let info = JoinGameInfo {
                entity_id: packet.entity_id,
                gamemode: packet.gamemode,
                dimension: packet.dimension,
                difficulty: packet.difficulty,
                max_players: packet.max_players,
                level_type: packet.level_type,
                reduced_debug_info: packet.reduced_debug_info,
            };
            send(events, NetEvent::JoinGame(info));
        }
        clientbound::Chat::ID => {
            let packet = clientbound::Chat::decode(&mut &body[..])?;
            send(events, NetEvent::Chat(packet.json));
        }
        clientbound::PositionLook::ID => {
            let packet = clientbound::PositionLook::decode(&mut &body[..])?;
            let placement = state.resolve(&packet);
            state.placement = Some(placement.clone());
            send(events, NetEvent::Placement(placement));
        }
        clientbound::UpdateHealth::ID => {
            let packet = clientbound::UpdateHealth::decode(&mut &body[..])?;
            send(events, NetEvent::UpdateHealth(packet));
        }
        clientbound::HeldItemChange::ID => {
            let packet = clientbound::HeldItemChange::decode(&mut &body[..])?;
            send(events, NetEvent::HeldItemChange(packet.slot));
        }
        clientbound::Respawn::ID => {
            let packet = clientbound::Respawn::decode(&mut &body[..])?;
            send(events, NetEvent::Respawn(packet));
        }
        clientbound::EntityTeleport::ID => {
            let packet = clientbound::EntityTeleport::decode(&mut &body[..])?;
            send(events, NetEvent::EntityTeleport(packet));
        }
        clientbound::EntityEquipment::ID => {
            let packet = clientbound::EntityEquipment::decode(&mut &body[..])?;
            send(events, NetEvent::EntityEquipment(packet));
        }
        clientbound::SpawnPosition::ID => {
            let packet = clientbound::SpawnPosition::decode(&mut &body[..])?;
            send(events, NetEvent::SpawnPosition(packet));
        }
        clientbound::SetExperience::ID => {
            let packet = clientbound::SetExperience::decode(&mut &body[..])?;
            send(events, NetEvent::SetExperience(packet));
        }
        clientbound::ChunkData::ID => {
            let packet = clientbound::ChunkData::decode(&mut &body[..])?;
            send(events, NetEvent::ChunkData(Box::new(packet)));
        }
        clientbound::MapChunkBulk::ID => {
            let packet = clientbound::MapChunkBulk::decode(&mut &body[..])?;
            send(events, NetEvent::MapChunkBulk(Box::new(packet)));
        }
        clientbound::BlockChange::ID => {
            let packet = clientbound::BlockChange::decode(&mut &body[..])?;
            send(events, NetEvent::BlockChange(packet));
        }
        clientbound::MultiBlockChange::ID => {
            let packet = clientbound::MultiBlockChange::decode(&mut &body[..])?;
            send(events, NetEvent::MultiBlockChange(packet));
        }
        clientbound::Disconnect::ID => {
            let packet = clientbound::Disconnect::decode(&mut &body[..])?;
            send(
                events,
                NetEvent::Error(umicraft_net::status::disconnect_reason(&packet.reason)),
            );
            return Err(umicraft_net::NetError::KickedAtPlay {
                reason: umicraft_net::status::disconnect_reason(&packet.reason),
            });
        }
        other => {
            tracing::trace!(id = other, bytes = body.len(), "skipped play packet");
        }
    }
    Ok(())
}

/// Sends the two entry packets a real client sends after `Join Game`.
///
/// The order matters: sending them before the server switches to the play
/// state makes it log `Bad packet id 21`.
async fn send_entry_packets(
    connection: &mut Connection,
    view_distance: u8,
) -> Result<(), umicraft_net::NetError> {
    connection
        .write_packet(&umicraft_protocol::PlayServerbound::ClientSettings(
            ClientSettings {
                locale: "en_us".to_string(),
                view_distance,
                chat_mode: 0,
                chat_colors: true,
                skin_parts: 0b0111_1111,
            },
        ))
        .await?;
    let mut data = Vec::with_capacity(12);
    write_var_u32(&mut data, "umicraft".len() as u32)?;
    data.extend_from_slice(b"umicraft");
    connection
        .write_packet(&umicraft_protocol::PlayServerbound::Brand(Brand {
            channel: "MC|Brand".to_string(),
            data,
        }))
        .await?;
    Ok(())
}

/// Forwards an event to the game thread.
///
/// A full channel means the game thread is behind; the network thread blocks
/// in `send`, which slows the server down instead of losing world data.
fn send(events: &SyncSender<GameEvent>, event: NetEvent) {
    let _ = events.send(GameEvent::Net(event));
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::clientbound::KeepAlive;

    #[test]
    fn the_relative_flags_accumulate_on_the_last_placement() {
        let mut state = PlayState::new(8);
        state.placement = Some(PlayerPlacement {
            x: 10.0,
            y: 64.0,
            z: -5.0,
            yaw: 90.0,
            pitch: 10.0,
        });
        let packet = PositionLook {
            x: 1.5,
            y: 70.0,
            z: -2.5,
            yaw: 15.0,
            pitch: -20.0,
            flags: 0x01 | 0x04 | 0x08,
        };
        let placement = state.resolve(&packet);
        assert_eq!(placement.x, 11.5);
        assert_eq!(placement.y, 70.0);
        assert_eq!(placement.z, -7.5);
        assert_eq!(placement.yaw, 105.0);
        assert_eq!(placement.pitch, -20.0);
    }

    #[test]
    fn the_commands_map_to_the_expected_wire_ids() {
        let cases = vec![
            (
                ClientCommand::Input {
                    strafe: 0.0,
                    forward: 1.0,
                    jumping: true,
                    sneaking: false,
                },
                0x0C,
            ),
            (
                ClientCommand::PlayerPosition {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                    on_ground: true,
                },
                0x04,
            ),
            (
                ClientCommand::PlayerLook {
                    yaw: 90.0,
                    pitch: 10.0,
                    on_ground: false,
                },
                0x05,
            ),
            (
                ClientCommand::PositionLook {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                    yaw: 90.0,
                    pitch: 10.0,
                    on_ground: true,
                },
                0x06,
            ),
            (
                ClientCommand::PlayerDigging {
                    status: DiggingStatus::StartDestroyBlock,
                    x: 1,
                    y: 2,
                    z: 3,
                    face: Facing::Up,
                },
                0x07,
            ),
            (
                ClientCommand::PlayerBlockPlacement {
                    x: 1,
                    y: 2,
                    z: 3,
                    face: Facing::North,
                    item: ItemStack::EMPTY,
                    cursor_x: 8,
                    cursor_y: 8,
                    cursor_z: 0,
                },
                0x08,
            ),
            (ClientCommand::HeldItemChange { slot: 3 }, 0x09),
        ];
        for (command, id) in cases {
            let packet = command.into_packet();
            assert_eq!(packet.packet_id(), id);
        }
    }

    #[test]
    fn the_input_command_carries_the_raw_wire_values() {
        let command = ClientCommand::Input {
            strafe: -1.0,
            forward: 0.3,
            jumping: false,
            sneaking: true,
        };
        let packet = command.into_packet();
        let umicraft_protocol::PlayServerbound::Input(input) = packet else {
            panic!("the command must map to an input packet");
        };
        assert_eq!(input.strafe, -1.0);
        assert_eq!(input.forward, 0.3);
        assert!(!input.jumping);
        assert!(input.sneaking);
    }

    #[test]
    fn the_keepalive_is_answered_with_the_same_id() {
        use umicraft_protocol::EncodablePacket;
        let mut body = Vec::new();
        KeepAlive { id: 4242 }
            .encode_body(&mut body)
            .expect("encode");
        let packet = KeepAlive::decode(&mut &body[..]).expect("decode");
        assert_eq!(packet.id, 4242);
    }
}
