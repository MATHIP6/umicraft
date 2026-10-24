// SPDX-License-Identifier: MIT OR Apache-2.0

//! The network thread: the owner of the socket during play.
//!
//! The thread runs a Tokio current-thread runtime, logs in and hands the
//! connection to [`umicraft_net::play::drive`], which answers the keepalives,
//! sends the entry packets and decodes the clientbound play packets
//! (ADR-0010). Every event is forwarded to the game thread over its bounded
//! inbox. The game thread answers with [`ClientCommand`]s, which it converts
//! into wire packets before they reach this thread.

use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::thread::JoinHandle;

use tokio::sync::mpsc::Receiver as CommandReceiver;
use umicraft_net::login::login_offline;
use umicraft_net::play::{drive, PlayOptions, PlaySession, SessionCounters, SessionEnd};
use umicraft_net::{Connection, NetError};
use umicraft_protocol::packets::play::serverbound::{
    self, DiggingStatus, HeldItemChange as ServerHeldItemChange, Input, PlayerBlockPlacement,
    PlayerDigging, PlayerLook, PlayerPosition, PositionLook as ServerPositionLook,
};
use umicraft_protocol::packets::play::{Facing, ItemStack};
use umicraft_protocol::PlayServerbound;

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
    /// `Player Position and Look` (0x06): a combined position and look report.
    ///
    /// The acknowledgement of a server placement uses the same wire packet
    /// but a separate variant: it must not be dropped, the server expects it
    /// before it accepts the next position.
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
    /// `Player Position and Look` (0x06): the acknowledgement of a server
    /// placement.
    AckPosition {
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
    pub fn into_packet(self) -> PlayServerbound {
        match self {
            Self::Input {
                strafe,
                forward,
                jumping,
                sneaking,
            } => PlayServerbound::Input(Input {
                strafe,
                forward,
                jumping,
                sneaking,
            }),
            Self::PlayerPosition { x, y, z, on_ground } => {
                PlayServerbound::PlayerPosition(PlayerPosition { x, y, z, on_ground })
            }
            Self::PlayerLook {
                yaw,
                pitch,
                on_ground,
            } => PlayServerbound::PlayerLook(PlayerLook {
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
            } => PlayServerbound::PositionLook(ServerPositionLook {
                x,
                y,
                z,
                yaw,
                pitch,
                on_ground,
            }),
            Self::AckPosition {
                x,
                y,
                z,
                yaw,
                pitch,
                on_ground,
            } => PlayServerbound::PositionLook(ServerPositionLook {
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
            } => PlayServerbound::PlayerDigging(PlayerDigging {
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
            } => PlayServerbound::PlayerBlockPlacement(PlayerBlockPlacement {
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
                PlayServerbound::HeldItemChange(ServerHeldItemChange { slot })
            }
            Self::Chat { message } => PlayServerbound::Chat(serverbound::Chat { message }),
        }
    }
}

/// Starts the network thread and returns its join handle.
///
/// The thread exits when the connection fails, the server disconnects, or the
/// command channel is closed (which happens when the game thread stops). A
/// failure or a kick reaches the game thread as [`GameEvent::Disconnected`].
///
/// # Errors
/// [`ClientError::Network`] when the OS refuses to start the thread.
pub fn spawn(
    config: NetworkConfig,
    events: SyncSender<GameEvent>,
    commands: CommandReceiver<PlayServerbound>,
    counters: Arc<SessionCounters>,
) -> Result<JoinHandle<()>, ClientError> {
    let handle = std::thread::Builder::new()
        .name("umicraft-net".to_string())
        .spawn(move || run_thread(config, events, commands, &counters))
        .map_err(|error| ClientError::Network(format!("spawn network thread: {error}")))?;
    Ok(handle)
}

fn run_thread(
    config: NetworkConfig,
    events: SyncSender<GameEvent>,
    commands: CommandReceiver<PlayServerbound>,
    counters: &SessionCounters,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = events.send(GameEvent::Disconnected(format!(
                "could not start network runtime: {error}"
            )));
            return;
        }
    };
    let ended = runtime.block_on(run_session(&config, &events, commands, counters));
    if let Some(reason) = ended {
        let _ = events.send(GameEvent::Disconnected(reason));
    }
}

/// Connects, logs in and drives the play state.
///
/// Returns the message for the game thread, or `None` when the game thread
/// itself ended the session by dropping the command sender.
async fn run_session(
    config: &NetworkConfig,
    events: &SyncSender<GameEvent>,
    mut commands: CommandReceiver<PlayServerbound>,
    counters: &SessionCounters,
) -> Option<String> {
    let mut connection = match Connection::connect(&config.host, config.port).await {
        Ok(connection) => connection,
        Err(error) => return Some(error.to_string()),
    };
    let profile = match login_offline(&mut connection, &config.name, None).await {
        Ok(profile) => profile,
        Err(error) => return Some(error.to_string()),
    };
    tracing::info!(
        host = %config.host,
        port = config.port,
        username = %profile.username,
        "logged in"
    );

    let mut session = PlaySession::new(PlayOptions {
        view_distance: config.view_distance,
        ..PlayOptions::default()
    });
    // A full inbox blocks this thread in `send`, which slows the server down
    // through TCP instead of losing world data. A closed inbox means the game
    // thread is gone; the command channel closes with it and ends the loop.
    let forward = |event| {
        let _ = events.send(GameEvent::Net(event));
    };
    match drive(
        &mut connection,
        &mut session,
        &mut commands,
        counters,
        forward,
    )
    .await
    {
        Ok(SessionEnd::CommandsClosed) => None,
        Ok(SessionEnd::Kicked(reason)) => Some(NetError::KickedAtPlay { reason }.to_string()),
        Err(error) => Some(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let PlayServerbound::Input(input) = packet else {
            panic!("the command must map to an input packet");
        };
        assert_eq!(input.strafe, -1.0);
        assert_eq!(input.forward, 0.3);
        assert!(!input.jumping);
        assert!(input.sneaking);
    }
}
