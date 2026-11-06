// SPDX-License-Identifier: MIT OR Apache-2.0

//! The network thread body: runtime, connect, login, play.

use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::thread::JoinHandle;

use tokio::sync::mpsc::Receiver as CommandReceiver;
use umicraft_auth::HttpSessionServer;
use umicraft_net::login::{login_offline, login_online};
use umicraft_net::play::{drive, PlayOptions, PlaySession, SessionCounters, SessionEnd};
use umicraft_net::Connection;
use umicraft_protocol::PlayServerbound;
use umicraft_ui::menus::ConnectStage;

use super::NetworkConfig;
use crate::connection_end::{ConnectionEnd, EndPhase};
use crate::error::ClientError;
use crate::game::GameEvent;

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
            let _ = events.send(GameEvent::Disconnected(ConnectionEnd::other(
                EndPhase::Connecting,
                format!("could not start network runtime: {error}"),
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
) -> Option<ConnectionEnd> {
    let mut connection = match Connection::connect(&config.host, config.port).await {
        Ok(connection) => connection,
        Err(error) => return Some(ConnectionEnd::from_error(EndPhase::Connecting, &error)),
    };
    // `GuiConnecting` switches from `connect.connecting` to
    // `connect.authorizing` as soon as the socket is open.
    let _ = events.send(GameEvent::ConnectStage(ConnectStage::LoggingIn));
    let logged_in = match &config.session {
        Some(session) => {
            login_online(
                &mut connection,
                session,
                &HttpSessionServer::new(),
                config.handshake_host.as_deref(),
            )
            .await
        }
        None => {
            login_offline(
                &mut connection,
                &config.name,
                config.handshake_host.as_deref(),
            )
            .await
        }
    };
    let profile = match logged_in {
        Ok(profile) => profile,
        Err(error) => return Some(ConnectionEnd::from_error(EndPhase::Connecting, &error)),
    };
    tracing::info!(
        host = %config.host,
        port = config.port,
        username = %profile.username,
        "logged in"
    );

    let mut session = PlaySession::new(PlayOptions {
        view_distance: config.view_distance,
        chat_mode: config.chat_mode,
        chat_colors: config.chat_colors,
        skin_parts: config.skin_parts,
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
        Ok(SessionEnd::Kicked(reason)) => Some(ConnectionEnd::kicked(reason)),
        Err(error) => Some(ConnectionEnd::from_error(EndPhase::Playing, &error)),
    }
}
