// SPDX-License-Identifier: MIT OR Apache-2.0

//! The handles of a running session and the start of the game thread.
//!
//! [`Session`] owns the channels between the network thread, the game
//! thread and the window; dropping it stops both threads.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;

use tokio::sync::mpsc::Sender as CommandSender;
use umicraft_assets::profile_textures::AccountSkinFetcher;
use umicraft_net::play::SessionCounters;
use umicraft_protocol::PlayServerbound;

use super::game_loop::run;
use super::messages::{ChunkEvent, GameEvent, RenderEvent};
use crate::error::ClientError;
use crate::input::InputUpdate;
use crate::network::{self, NetworkConfig};

/// Bounded inbox of the game thread.
///
/// The world packets are the bulk (a chunk is up to 100 KiB), so the queue is
/// sized like the M2 network queue: when it is full the network thread
/// blocks and the TCP backpressure slows the server down.
const INBOX_CAPACITY: usize = 256;

/// Bounded queue for dedicated client input updates.
const INPUT_CAPACITY: usize = 512;

/// Bounded queue of the frames the render thread polls.
const FRAME_CAPACITY: usize = 32;

/// Bounded queue of the chunk snapshots the render thread polls.
const CHUNK_CAPACITY: usize = 64;

/// The handles of a running client session.
pub struct Session {
    /// Dedicated bounded sender for player input updates.
    inputs: crossbeam_channel::Sender<InputUpdate>,
    /// The sender for the events the front end produces.
    events: crossbeam_channel::Sender<GameEvent>,
    /// The frames of the game thread.
    frames: Receiver<RenderEvent>,
    /// The chunk snapshots of the game thread.
    chunks: Receiver<ChunkEvent>,
    /// The stop flag of the session.
    stop: Arc<AtomicBool>,
    /// The live statistics of the play session.
    counters: Arc<SessionCounters>,
    /// Duration of the last displayed frame in microseconds; the game thread
    /// takes network events once per frame, like `Minecraft.runGameLoop`
    /// (phase E.9). Zero turns the pacing off.
    frame_micros: Arc<AtomicU32>,
    network: Option<JoinHandle<()>>,
    game: Option<JoinHandle<()>>,
}

/// The frame duration assumed until the window reports one (60 frames per
/// second): the headless client has no display loop.
const DEFAULT_FRAME_MICROS: u32 = 16_667;

/// The shortest frame the pacing honours (1 000 frames per second): a display
/// loop faster than this would not leave the server's packet limit anyway.
const MIN_FRAME_MICROS: u32 = 1_000;

/// The longest frame duration the pacing follows, so a stalled window does
/// not hold the network events back for seconds.
const MAX_FRAME_MICROS: u32 = 100_000;

impl Session {
    /// Starts the network thread and the game thread.
    ///
    /// # Errors
    /// [`ClientError::Network`] or [`ClientError::Worker`] when a thread
    /// cannot be spawned.
    pub fn start(config: NetworkConfig) -> Result<Self, ClientError> {
        let (inputs_tx, inputs_rx) = crossbeam_channel::bounded(INPUT_CAPACITY);
        let (events_tx, events_rx) = crossbeam_channel::bounded(INBOX_CAPACITY);
        let (frames_tx, frames_rx) = mpsc::sync_channel(FRAME_CAPACITY);
        let (chunks_tx, chunks_rx) = mpsc::sync_channel(CHUNK_CAPACITY);
        let (commands_tx, commands_rx) = tokio::sync::mpsc::channel(network::COMMAND_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let counters = Arc::new(SessionCounters::default());
        let own_name = config.login_name().to_owned();
        // The skin of the account, premium or not: a premium session names its
        // profile, an offline account only has a name to look up.
        let account_skin = config.account_skin.then(|| {
            let uuid = config.session.as_ref().map(|session| session.uuid.as_str());
            AccountSkinFetcher::spawn(config.login_name(), uuid)
        });

        let network = network::spawn(
            config,
            events_tx.clone(),
            commands_rx,
            Arc::clone(&counters),
        )?;
        let frame_micros = Arc::new(AtomicU32::new(DEFAULT_FRAME_MICROS));
        let game = spawn_game_thread(
            GameChannels {
                inputs: inputs_rx,
                events: events_rx,
                frames: frames_tx,
                chunks: chunks_tx,
                commands: commands_tx,
                frame_micros: Arc::clone(&frame_micros),
            },
            Arc::clone(&stop),
            own_name,
            account_skin,
        )?;

        Ok(Self {
            inputs: inputs_tx,
            events: events_tx,
            frames: frames_rx,
            chunks: chunks_rx,
            stop,
            counters,
            frame_micros,
            network: Some(network),
            game: Some(game),
        })
    }

    /// Reports the duration of the frame the window just displayed. The game
    /// thread takes network events at most once per such period (phase E.9).
    pub fn note_frame(&self, duration: std::time::Duration) {
        let micros = u32::try_from(duration.as_micros()).unwrap_or(u32::MAX);
        self.frame_micros.store(
            micros.clamp(MIN_FRAME_MICROS, MAX_FRAME_MICROS),
            Ordering::Relaxed,
        );
    }

    /// The number of serverbound packets the socket thread wrote.
    ///
    /// The 1.8 server limits the packets a player may send (PandaSpigot
    /// kicks above 500 packets in seven seconds), so the rate belongs into
    /// the acceptance report.
    #[must_use]
    pub fn packets_sent(&self) -> u64 {
        self.counters.packets_sent()
    }

    /// Sends an input update to the game thread on the dedicated high-priority input channel.
    ///
    /// This channel never competes with network packets, guaranteeing zero-latency delivery.
    pub fn send_input(&self, update: InputUpdate) -> bool {
        self.inputs.try_send(update).is_ok()
    }

    /// The sender for the events the front end produces.
    ///
    /// The front end uses it to send the input updates; the send never
    /// blocks, a full inbox means the game thread is behind and the input is
    /// repeated on the next update.
    #[must_use]
    pub fn events(&self) -> &crossbeam_channel::Sender<GameEvent> {
        &self.events
    }

    /// The frames of the game thread.
    #[must_use]
    pub fn frames(&self) -> &Receiver<RenderEvent> {
        &self.frames
    }

    /// The chunk snapshots of the game thread.
    #[must_use]
    pub fn chunks(&self) -> &Receiver<ChunkEvent> {
        &self.chunks
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // The stop flag ends the game thread, which drops the command sender,
        // which ends the network thread.
        self.stop.store(true, Ordering::Relaxed);
        if let Some(game) = self.game.take() {
            let _ = game.join();
        }
        if let Some(network) = self.network.take() {
            let _ = network.join();
        }
    }
}

/// The communication channels of the game thread.
pub(super) struct GameChannels {
    pub(super) inputs: crossbeam_channel::Receiver<InputUpdate>,
    pub(super) events: crossbeam_channel::Receiver<GameEvent>,
    pub(super) frames: SyncSender<RenderEvent>,
    pub(super) chunks: SyncSender<ChunkEvent>,
    pub(super) commands: CommandSender<PlayServerbound>,
    /// See [`Session::note_frame`]; zero means "take events at once".
    pub(super) frame_micros: Arc<AtomicU32>,
}

/// Starts the game thread.
fn spawn_game_thread(
    channels: GameChannels,
    stop: Arc<AtomicBool>,
    own_name: String,
    account_skin: Option<AccountSkinFetcher>,
) -> Result<JoinHandle<()>, ClientError> {
    std::thread::Builder::new()
        .name("umicraft-game".to_string())
        .spawn(move || {
            // The ticks must start on time (`thread_priority`).
            crate::thread_priority::set_current(crate::thread_priority::Priority::AboveNormal);
            run(channels, stop, own_name, account_skin);
        })
        .map_err(|error| ClientError::Worker(format!("spawn game thread: {error}")))
}
