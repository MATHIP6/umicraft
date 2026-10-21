// SPDX-License-Identifier: MIT OR Apache-2.0

//! Background network thread.
//!
//! A dedicated Tokio current-thread runtime owns the connection and the
//! [`PlaySession`] state machine. It forwards decoded world packets over a
//! bounded standard channel; the renderer main thread never blocks on the
//! socket.

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::JoinHandle;

use umicraft_net::client::{JoinGameInfo, PlayEvent, PlayOptions, PlaySession, PlayerPlacement};
use umicraft_net::login::login_offline;
use umicraft_net::Connection;
use umicraft_protocol::packets::play::clientbound::{
    BlockChange, ChunkData, MapChunkBulk, MultiBlockChange,
};

use crate::error::ClientError;

/// Bounded hand-off between the socket thread and the render thread. One
/// packet can be a full chunk (~100 KiB), so 256 slots cap worst-case queued
/// world data at roughly 25 MiB; when full, the network thread blocks and TCP
/// backpressure slows the server instead of growing memory without bound.
const EVENT_CAPACITY: usize = 256;

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

/// Decoded event sent from the network thread to the main thread.
#[derive(Debug)]
pub enum NetEvent {
    /// The server entered play state; start a fresh world for this dimension.
    JoinGame(JoinGameInfo),
    /// Absolute player placement after relative-flag resolution.
    Placement(PlayerPlacement),
    /// Raw chat JSON (counted by the client).
    Chat(String),
    /// One chunk column.
    ChunkData(Box<ChunkData>),
    /// Initial chunk batch.
    MapChunkBulk(Box<MapChunkBulk>),
    /// One block change.
    BlockChange(BlockChange),
    /// Batched block changes.
    MultiBlockChange(MultiBlockChange),
    /// Fatal connection/session error, already formatted for the overlay.
    Error(String),
}

/// Starts the network thread and returns its join handle.
///
/// The thread exits when the connection fails, the server disconnects, or the
/// receiver is dropped.
///
/// # Errors
/// [`ClientError::Network`] when the OS refuses to start the thread.
pub fn spawn(config: NetworkConfig) -> Result<(JoinHandle<()>, Receiver<NetEvent>), ClientError> {
    let (event_sender, event_receiver) = mpsc::sync_channel(EVENT_CAPACITY);
    let handle = std::thread::Builder::new()
        .name("umicraft-net".to_string())
        .spawn(move || run_thread(config, event_sender))
        .map_err(|error| ClientError::Network(format!("spawn network thread: {error}")))?;
    Ok((handle, event_receiver))
}

fn run_thread(config: NetworkConfig, sender: SyncSender<NetEvent>) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = sender.send(NetEvent::Error(format!(
                "could not start network runtime: {error}"
            )));
            return;
        }
    };
    runtime.block_on(run_session(config, sender));
}

async fn run_session(config: NetworkConfig, sender: SyncSender<NetEvent>) {
    let mut connection = match Connection::connect(&config.host, config.port).await {
        Ok(connection) => connection,
        Err(error) => {
            let _ = sender.send(NetEvent::Error(error.to_string()));
            return;
        }
    };
    let profile = match login_offline(&mut connection, &config.name, None).await {
        Ok(profile) => profile,
        Err(error) => {
            let _ = sender.send(NetEvent::Error(error.to_string()));
            return;
        }
    };
    tracing::info!(
        host = %config.host,
        port = config.port,
        username = %profile.username,
        "logged in"
    );

    let options = PlayOptions {
        view_distance: config.view_distance,
        ..PlayOptions::default()
    };
    let mut session = PlaySession::new(options);
    loop {
        let event = match session.next(&mut connection).await {
            Ok(event) => event,
            Err(error) => {
                let _ = sender.send(NetEvent::Error(error.to_string()));
                return;
            }
        };
        let outgoing = match event {
            PlayEvent::JoinGame(info) => NetEvent::JoinGame(info),
            PlayEvent::Placement(placement) => NetEvent::Placement(placement),
            PlayEvent::Chat(json) => NetEvent::Chat(json),
            PlayEvent::ChunkData(packet) => NetEvent::ChunkData(Box::new(packet)),
            PlayEvent::MapChunkBulk(packet) => NetEvent::MapChunkBulk(Box::new(packet)),
            PlayEvent::BlockChange(packet) => NetEvent::BlockChange(packet),
            PlayEvent::MultiBlockChange(packet) => NetEvent::MultiBlockChange(packet),
        };
        if sender.send(outgoing).is_err() {
            return;
        }
    }
}
