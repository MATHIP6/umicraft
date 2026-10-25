// SPDX-License-Identifier: MIT OR Apache-2.0

//! The game thread: the owner of the world, of the player state and of the
//! fixed 20 Hz tick (ADR-0005).
//!
//! The thread owns the two pieces of mutable state of a playing client: the
//! [`umicraft_world::World`] and the [`umicraft_game::GameState`]. Nothing
//! else touches them, so the collision path never takes a lock (spec §5.3).
//!
//! It receives two kinds of messages on one bounded inbox:
//!
//! * the decoded clientbound packets of the network thread, and
//! * the input updates of an input source (the window or the headless script).
//!
//! It answers with two bounded channels: the *frames* (camera, HUD, status)
//! and the *chunk snapshots* the mesher needs. Both are polled by the render
//! thread; the game thread never blocks on them, and a snapshot that does not
//! fit into the queue stays in the dirty set and is derived again later.

use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use glam::{DVec3, Vec3};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::Sender as CommandSender;
use umicraft_game::entity::{EntityFlags, EntityFrameData, EntityId, EntityStore};
use umicraft_game::input::RawInput;
use umicraft_game::raycast::{look_direction, raycast, RayHit, CREATIVE_REACH, SURVIVAL_REACH};
use umicraft_game::shapes::Aabb;
use umicraft_game::{GameState, RenderState, ServerCorrection};
use umicraft_mesh::ChunkSnapshot;
use umicraft_net::play::{PlayEvent, SessionCounters};
use umicraft_protocol::packets::play::clientbound::{ChunkData, MapChunkBulk};
use umicraft_protocol::packets::play::serverbound::DiggingStatus;
use umicraft_protocol::packets::play::{Facing, ItemStack as WireItemStack};
use umicraft_protocol::PlayServerbound;
use umicraft_render::{HudState, ItemStack};
use umicraft_world::{BlockPos, BlockState, ChunkPayload, ChunkPos, World};

use crate::error::ClientError;
use crate::input::InputUpdate;
use crate::network::{self, ClientCommand, NetworkConfig};

/// The tick period of the simulation: 20 Hz, as the 1.8 server (spec GAM-1).
pub const TICK: Duration = Duration::from_millis(50);

/// Bounded inbox of the game thread.
///
/// The world packets are the bulk (a chunk is up to 100 KiB), so the queue is
/// sized like the M2 network queue: when it is full the network thread
/// blocks and the TCP backpressure slows the server down.
const INBOX_CAPACITY: usize = 256;

/// Bounded queue of the frames the render thread polls.
const FRAME_CAPACITY: usize = 32;

/// Bounded queue of the chunk snapshots the render thread polls.
const CHUNK_CAPACITY: usize = 64;

/// The number of chat lines the HUD keeps.
const CHAT_HISTORY: usize = 10;

/// The number of ticks between two status updates.
const STATUS_PERIOD: u64 = 20;

/// The number of ticks between the start and the stop of a block break.
///
/// The server destroys the block when the stop packet arrives and the
/// accumulated break progress `hardness * (ticks + 1)` reaches `0.7`
/// (MCP `ItemInWorldManager.func_180785_a`). The effective hardness of the
/// softest block is 0.054 for a player without the right tool, so the stop
/// needs about thirteen server ticks. **Measured** against the local 1.8
/// server: a stop after six ticks did not destroy the block, a stop after
/// twelve ticks did (see `docs/sources.md`).
const BREAK_TICKS: u64 = 12;

/// The largest pitch the view may reach, in radians.
const MAX_PITCH: f32 = 89.0_f32.to_radians();

/// The squared distance above which the position counts as changed.
///
/// MCP `EntityPlayerSP.onUpdateWalkingPlayer` compares the squared distance
/// with `9.0E-4`, i.e. three centimetres.
const MOVE_EPSILON: f64 = 9.0e-4;

/// The armour points of every 1.8 armour piece.
///
/// MCP `ItemArmor.ArmorMaterial` lists the damage reduction of the four
/// pieces as {head, chest, legs, feet}; the item ids are the registration
/// order of MCP `Item.java` (leather 298-301, chainmail 302-305, iron
/// 306-309, diamond 310-313, gold 314-317).
const ARMOR_POINTS: [u8; 20] = [
    1, 3, 2, 1, // 298 leather helmet, chestplate, leggings, boots
    2, 5, 4, 1, // 302 chainmail
    2, 6, 5, 2, // 306 iron
    3, 8, 6, 3, // 310 diamond
    2, 5, 3, 1, // 314 gold
];

/// The armour points of an item, `None` when the item is not armour.
#[must_use]
pub fn armor_points(item_id: i16) -> Option<u8> {
    let index = i32::from(item_id) - 298;
    if (0..ARMOR_POINTS.len() as i32).contains(&index) {
        Some(ARMOR_POINTS[index as usize])
    } else {
        None
    }
}

/// The camera angles of a player state, in radians.
///
/// The game logic uses the Minecraft angles (yaw zero looks towards +Z, a
/// positive pitch looks down), while the renderer camera uses the opposite
/// signs: the yaw is turned by half a turn and the pitch is flipped.
#[must_use]
pub fn camera_angles(state: &RenderState) -> (f32, f32) {
    (state.yaw + std::f32::consts::PI, -state.pitch)
}

/// A message the game thread receives.
#[derive(Debug)]
pub enum GameEvent {
    /// A decoded clientbound packet.
    Net(PlayEvent),
    /// The network session ended: a failure or a kick, already formatted for
    /// the overlay.
    Disconnected(String),
    /// An input update from an input source.
    Input(InputUpdate),
    /// A chat line the player typed.
    Chat(String),
    /// Request to re-mesh every loaded chunk column (e.g. after a resource pack reload).
    ReMeshAll,
}

/// One simulation frame published to the render thread.
#[derive(Debug, Clone)]
pub struct Frame {
    /// The player state after the tick.
    pub state: RenderState,
    /// The instant of the tick that produced the state.
    ///
    /// The render thread interpolates between the two newest frames, so it
    /// needs to know when the tick happened (GAM-8).
    pub tick_time: Instant,
    /// The signed time of day in ticks.
    ///
    /// Negative values mean the server froze the day/night cycle; the render
    /// thread must not advance the clock locally (ADR-0008 §4).
    pub world_time: i64,
    /// The current dimension (`0` overworld, `-1` Nether, `1` End).
    pub dimension: i8,
    /// Per-entity interpolation data for the render thread.
    pub entities: Arc<[EntityFrameData]>,
}

/// The statistics the debug overlay shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameStatus {
    /// The human-readable connection status.
    pub status: String,
    /// The number of loaded chunk columns.
    pub chunks: usize,
    /// The number of chunk packets applied.
    pub chunk_packets: u64,
    /// The number of block updates applied.
    pub block_updates: u64,
    /// The number of chat messages received.
    pub chat_messages: u64,
    /// The number of simulated ticks.
    pub ticks: u64,
    /// The number of serverbound position reports.
    pub position_reports: u64,
    /// The number of `Player Position and Look` packets the server sent after
    /// the initial placement; a healthy session has none.
    pub resyncs: u64,
    /// The number of block breaks the server confirmed.
    pub breaks_acknowledged: u64,
    /// The number of block placements the server confirmed.
    pub placements_acknowledged: u64,
    /// The gamemode of the local player; `1` is creative.
    pub gamemode: u8,
}

/// A message the game thread sends to the render thread.
#[derive(Debug)]
pub enum RenderEvent {
    /// A simulation frame.
    Frame(Frame),
    /// The HUD state changed.
    Hud(Box<HudState>),
    /// The statistics changed.
    Status(Box<GameStatus>),
    /// The world was replaced; the render thread drops its meshes.
    Reset,
    /// The session ended.
    Ended(String),
}

/// A chunk whose geometry changed.
#[derive(Debug)]
pub struct ChunkEvent {
    /// The chunk column.
    pub pos: ChunkPos,
    /// The immutable snapshot the mesher needs.
    pub snapshot: ChunkSnapshot,
}

/// The handles of a running client session.
pub struct Session {
    /// The sender for the events the front end produces.
    events: SyncSender<GameEvent>,
    /// The frames of the game thread.
    frames: Receiver<RenderEvent>,
    /// The chunk snapshots of the game thread.
    chunks: Receiver<ChunkEvent>,
    /// The stop flag of the session.
    stop: Arc<AtomicBool>,
    /// The live statistics of the play session.
    counters: Arc<SessionCounters>,
    network: Option<JoinHandle<()>>,
    game: Option<JoinHandle<()>>,
}

impl Session {
    /// Starts the network thread and the game thread.
    ///
    /// # Errors
    /// [`ClientError::Network`] or [`ClientError::Worker`] when a thread
    /// cannot be spawned.
    pub fn start(config: NetworkConfig) -> Result<Self, ClientError> {
        let (events_tx, events_rx) = mpsc::sync_channel(INBOX_CAPACITY);
        let (frames_tx, frames_rx) = mpsc::sync_channel(FRAME_CAPACITY);
        let (chunks_tx, chunks_rx) = mpsc::sync_channel(CHUNK_CAPACITY);
        let (commands_tx, commands_rx) = tokio::sync::mpsc::channel(network::COMMAND_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let counters = Arc::new(SessionCounters::default());

        let network = network::spawn(
            config,
            events_tx.clone(),
            commands_rx,
            Arc::clone(&counters),
        )?;
        let game = spawn_game_thread(
            events_rx,
            frames_tx,
            chunks_tx,
            commands_tx,
            Arc::clone(&stop),
        )?;

        Ok(Self {
            events: events_tx,
            frames: frames_rx,
            chunks: chunks_rx,
            stop,
            counters,
            network: Some(network),
            game: Some(game),
        })
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

    /// The sender for the events the front end produces.
    ///
    /// The front end uses it to send the input updates; the send never
    /// blocks, a full inbox means the game thread is behind and the input is
    /// repeated on the next update.
    #[must_use]
    pub fn events(&self) -> &SyncSender<GameEvent> {
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

/// Starts the game thread.
fn spawn_game_thread(
    events: Receiver<GameEvent>,
    frames: SyncSender<RenderEvent>,
    chunks: SyncSender<ChunkEvent>,
    commands: CommandSender<PlayServerbound>,
    stop: Arc<AtomicBool>,
) -> Result<JoinHandle<()>, ClientError> {
    std::thread::Builder::new()
        .name("umicraft-game".to_string())
        .spawn(move || run(events, frames, chunks, commands, stop))
        .map_err(|error| ClientError::Worker(format!("spawn game thread: {error}")))
}

/// The state of a block-breaking interaction.
#[derive(Debug, Clone, Copy)]
struct Digging {
    /// The block that is being broken.
    block: BlockPos,
    /// The face of the block that was hit.
    face: Facing,
    /// The tick at which the stop packet must be sent.
    stop_at: u64,
}

/// The game state owned by the game thread.
struct Game {
    /// The loaded world; `None` before `Join Game`.
    world: Option<World>,
    /// The chunks whose geometry changed and still need a snapshot.
    dirty: BTreeSet<ChunkPos>,
    /// The human-readable status.
    status: String,
    /// The entity id of the local player.
    entity_id: i32,
    /// The gamemode of the local player; `1` is creative.
    gamemode: u8,
    /// The current dimension; `0` is the overworld.
    dimension: i8,
    /// The latest signed time of day from `Time Update`.
    ///
    /// It is never advanced locally: the server sends updates regularly and a
    /// negative value means the clock is frozen.
    world_time: i64,
    /// Whether the server is in the play state.
    playing: bool,
    /// The world spawn point, as the server announced it.
    spawn: Option<(i32, i32, i32)>,
    /// The player state; `None` before the first placement.
    state: Option<GameState>,
    /// The view angle in radians; yaw zero looks towards +Z (south).
    yaw: f32,
    /// The view angle in radians; a positive pitch looks down.
    pitch: f32,
    /// The latest key state.
    input: crate::input::InputState,
    /// The health points.
    health: f32,
    /// The food points.
    food: f32,
    /// The armour points.
    armor: f32,
    /// The experience bar fill, `0..=1`.
    experience: f32,
    /// The experience level.
    level: i32,
    /// The item the player holds, as the server described it.
    held_item: Option<WireItemStack>,
    /// The equipped armour pieces, indexed by the 1.8 equipment slot.
    equipment: [Option<WireItemStack>; 5],
    /// The selected hotbar slot.
    selected_slot: u8,
    /// The chat lines, oldest first.
    chat: VecDeque<String>,
    /// The last reported position.
    last_position: Option<(f64, f64, f64)>,
    /// The last reported view angles, in radians.
    last_rotation: (f32, f32),
    /// The ticks since the last position report.
    ticks_since_report: u32,
    /// The block that is being broken.
    digging: Option<Digging>,
    /// The last position the client acknowledged to the server.
    last_acknowledged: Option<(f64, f64, f64)>,
    /// The block whose break the server is expected to confirm.
    expected_break: Option<BlockPos>,
    /// The block whose placement the server is expected to confirm.
    expected_placement: Option<BlockPos>,
    /// The statistics of the session.
    status_data: GameStatus,
    /// Whether the HUD has to be published again.
    hud_dirty: bool,
    /// Entities in the world (mobs, players, objects).
    entities: EntityStore,
}

impl Game {
    fn new() -> Self {
        Self {
            world: None,
            dirty: BTreeSet::new(),
            status: "CONNECTING".to_string(),
            entity_id: 0,
            gamemode: 0,
            dimension: 0,
            world_time: 0,
            playing: false,
            spawn: None,
            state: None,
            yaw: 0.0,
            pitch: 0.0,
            input: crate::input::InputState::default(),
            health: 20.0,
            food: 20.0,
            armor: 0.0,
            experience: 0.0,
            level: 0,
            held_item: None,
            equipment: std::array::from_fn(|_| None),
            selected_slot: 0,
            chat: VecDeque::new(),
            last_position: None,
            last_rotation: (0.0, 0.0),
            ticks_since_report: 0,
            digging: None,
            last_acknowledged: None,
            expected_break: None,
            expected_placement: None,
            status_data: GameStatus::default(),
            hud_dirty: true,
            entities: EntityStore::new(),
        }
    }

    /// The reach of the block interaction, in blocks.
    ///
    /// MCP `PlayerControllerMP.java:340-343`: 4.5 blocks in survival and 5.0
    /// in creative.
    fn reach(&self) -> f64 {
        if self.gamemode == 1 {
            CREATIVE_REACH
        } else {
            SURVIVAL_REACH
        }
    }

    /// The eye position of the player.
    fn eye(&self) -> Option<DVec3> {
        self.state
            .as_ref()
            .map(|state| state.player().eye_position())
    }

    /// The block the player looks at.
    fn target(&self) -> Option<RayHit> {
        let eye = self.eye()?;
        let direction = look_direction(self.yaw, self.pitch);
        raycast(eye, direction, self.reach(), self.world.as_ref()?)
    }

    /// True when the chunks around the player are loaded.
    ///
    /// The simulation must not run on an incomplete world: the collision
    /// solver would see air, the player would fall through the floor and the
    /// server would reject the position.
    fn world_ready(&self) -> bool {
        let Some(world) = self.world.as_ref() else {
            return false;
        };
        let Some(state) = self.state.as_ref() else {
            return false;
        };
        let position = state.position();
        let chunk = ChunkPos::new(
            (position.x as i32).div_euclid(16),
            (position.z as i32).div_euclid(16),
        );
        for dx in -1..=1 {
            for dz in -1..=1 {
                if world
                    .chunk(ChunkPos::new(chunk.x + dx, chunk.z + dz))
                    .is_none()
                {
                    return false;
                }
            }
        }
        true
    }

    /// The HUD state of the session.
    fn hud(&self) -> HudState {
        let mut hotbar = [None; 9];
        if let Some(item) = self.held_item.as_ref() {
            if item.item_id >= 0 {
                hotbar[(self.selected_slot as usize).min(8)] = Some(ItemStack {
                    item_id: i32::from(item.item_id),
                    damage: item.damage,
                    count: u8::try_from(item.stack_size).unwrap_or(0),
                });
            }
        }
        HudState {
            health: self.health,
            food: self.food,
            armor: self.armor,
            experience: self.experience,
            level: self.level,
            hotbar,
            selected_slot: self.selected_slot,
            crosshair: true,
            chat_lines: self.chat.iter().cloned().collect(),
            debug_lines: Vec::new(),
        }
    }
}

/// The commands that could not be handed to the network thread yet.
///
/// The input packet and the position report are *coalesced*: only the newest
/// one of each matters, and a backlog of them would burst on the socket.
/// The 1.8 server counts the packets of a player and kicks above 500 in
/// seven seconds (PandaSpigot), so a burst of a few hundred position
/// reports would end the session.
#[derive(Default)]
struct CommandQueue {
    /// The commands that keep their order (the acknowledgement of a
    /// placement, the dig, the placement, the chat).
    ordered: VecDeque<ClientCommand>,
    /// The newest input packet.
    input: Option<ClientCommand>,
    /// The newest position report.
    position: Option<ClientCommand>,
    sender: Option<CommandSender<PlayServerbound>>,
}

impl CommandQueue {
    /// Queues a command; the socket thread picks it up.
    fn push(&mut self, command: ClientCommand) {
        match command {
            ClientCommand::Input { .. } => self.input = Some(command),
            ClientCommand::PlayerPosition { .. }
            | ClientCommand::PlayerLook { .. }
            | ClientCommand::PositionLook { .. } => self.position = Some(command),
            other => self.ordered.push_back(other),
        }
    }

    /// Retries the queued commands.
    ///
    /// A command is converted into its wire packet only once the channel
    /// reserved a slot for it, so a full channel keeps the command (and its
    /// coalescing slot) for the next tick.
    fn flush(&mut self) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        while let Some(command) = self.ordered.pop_front() {
            if let Err(command) = try_send(sender, command) {
                self.ordered.push_front(command);
                break;
            }
        }
        if let Some(command) = self.position.take() {
            self.position = try_send(sender, command).err();
        }
        if let Some(command) = self.input.take() {
            self.input = try_send(sender, command).err();
        }
    }
}

/// Sends a command to the network thread without blocking.
///
/// Gives the command back when the channel is full; drops it when the network
/// thread is gone, since nothing will ever read it.
fn try_send(
    sender: &CommandSender<PlayServerbound>,
    command: ClientCommand,
) -> Result<(), ClientCommand> {
    match sender.try_reserve() {
        Ok(permit) => {
            permit.send(command.into_packet());
            Ok(())
        }
        Err(TrySendError::Full(())) => Err(command),
        Err(TrySendError::Closed(())) => Ok(()),
    }
}

/// The game thread loop.
fn run(
    events: Receiver<GameEvent>,
    frames: SyncSender<RenderEvent>,
    chunks: SyncSender<ChunkEvent>,
    commands: CommandSender<PlayServerbound>,
    stop: Arc<AtomicBool>,
) {
    let mut game = Game::new();
    let mut queue = CommandQueue {
        ordered: VecDeque::new(),
        input: None,
        position: None,
        sender: Some(commands),
    };
    let mut next_tick = Instant::now();
    let mut ended: Option<String> = None;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        // Drain everything that is pending before looking at the clock.
        while let Ok(event) = events.try_recv() {
            if handle_event(&mut game, event, &mut queue, &frames) {
                ended = Some(game.status.clone());
                break;
            }
        }
        if ended.is_some() {
            break;
        }
        queue.flush();

        let now = Instant::now();
        if now >= next_tick {
            tick(&mut game, &mut queue, &frames, &chunks, next_tick);
            next_tick += TICK;
            if next_tick <= now {
                // The thread fell behind; skip the missed ticks instead of
                // running a burst of them.
                next_tick = now + TICK;
            }
            continue;
        }

        match events.recv_timeout(next_tick - now) {
            Ok(event) => {
                if handle_event(&mut game, event, &mut queue, &frames) {
                    ended = Some(game.status.clone());
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    if let Some(reason) = ended {
        let _ = frames.send(RenderEvent::Ended(reason));
    }
}

/// Handles one message of the inbox.
///
/// Returns `true` when the session must end.
fn handle_event(
    game: &mut Game,
    event: GameEvent,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) -> bool {
    match event {
        GameEvent::Net(event) => {
            handle_net(game, event, queue, frames);
            false
        }
        GameEvent::Disconnected(message) => {
            tracing::error!(%message, "network session ended");
            game.status = format!("ERROR {message}");
            game.status_data.status = game.status.clone();
            publish_status(game, frames);
            true
        }
        GameEvent::Input(update) => {
            let before = (game.yaw, game.pitch);
            handle_input(game, update, queue);
            if (game.yaw, game.pitch) != before {
                // The view changed between two ticks: publish the frame right
                // away, so the mouse look does not wait for the next tick.
                publish_frame(game, frames, Instant::now());
            }
            false
        }
        GameEvent::Chat(message) => {
            if !message.is_empty() && message.len() <= 100 {
                queue.push(ClientCommand::Chat { message });
            }
            false
        }
        GameEvent::ReMeshAll => {
            if let Some(world) = game.world.as_ref() {
                for (pos, _) in world.chunks() {
                    game.dirty.insert(pos);
                }
            }
            false
        }
    }
}

/// Publishes the current player state as a frame.
///
/// The frame queue is polled by the render thread, so the game thread never
/// blocks on it: a full queue drops the frame and the render thread keeps the
/// one it has.
fn publish_frame(game: &Game, frames: &SyncSender<RenderEvent>, tick_time: Instant) {
    let Some(state) = game.state.as_ref() else {
        return;
    };
    let _ = frames.try_send(RenderEvent::Frame(Frame {
        state: state.interpolated(1.0),
        tick_time,
        world_time: game.world_time,
        dimension: game.dimension,
        entities: game.entities.frame_data().into(),
    }));
}

/// Handles one decoded clientbound packet.
fn handle_net(
    game: &mut Game,
    event: PlayEvent,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    match event {
        PlayEvent::JoinGame(info) => {
            game.world = Some(World::new(info.dimension));
            game.dirty.clear();
            game.entities.clear();
            game.entity_id = info.entity_id;
            game.gamemode = info.gamemode;
            game.dimension = info.dimension;
            // The world clock of the new session is unknown until the first
            // `Time Update`; start at the beginning of the cycle.
            game.world_time = 0;
            game.status_data.gamemode = info.gamemode;
            game.playing = true;
            game.status = "PLAYING".to_string();
            game.status_data.status = game.status.clone();
            let _ = frames.try_send(RenderEvent::Reset);
            publish_status(game, frames);
        }
        PlayEvent::Placement(placement) => {
            apply_placement(game, &placement, queue);
        }
        PlayEvent::Chat(json) => {
            game.status_data.chat_messages += 1;
            game.chat.push_back(crate::chat::plain_text(&json));
            while game.chat.len() > CHAT_HISTORY {
                game.chat.pop_front();
            }
            game.hud_dirty = true;
        }
        PlayEvent::ChunkData(packet) => {
            game.status_data.chunk_packets += 1;
            if let Some(world) = game.world.as_mut() {
                let (pos, payload) = chunk_payload(&packet);
                match world.apply_chunk(pos, payload) {
                    Ok(dirty) => game.dirty.extend(dirty),
                    Err(error) => {
                        tracing::warn!(%error, "chunk data rejected");
                        game.status = format!("WORLD ERROR {error}");
                    }
                }
            }
        }
        PlayEvent::MapChunkBulk(packet) => {
            game.status_data.chunk_packets += 1;
            if let Some(world) = game.world.as_mut() {
                match world.apply_chunk_bulk(packet.sky_light, &bulk_payloads(&packet)) {
                    Ok(dirty) => game.dirty.extend(dirty),
                    Err(error) => {
                        tracing::warn!(%error, "bulk chunk rejected");
                        game.status = format!("WORLD ERROR {error}");
                    }
                }
            }
        }
        PlayEvent::BlockChange(packet) => {
            game.status_data.block_updates += 1;
            let (x, y, z) = packet.position();
            let position = BlockPos::new(x, y, z);
            if let Some(world) = game.world.as_mut() {
                let state = BlockState::from_wire(packet.block_state);
                game.dirty.extend(world.apply_block_change(position, state));
            }
            acknowledge_block(game, position, packet.block_state);
        }
        PlayEvent::MultiBlockChange(packet) => {
            game.status_data.block_updates += packet.records.len() as u64;
            if let Some(world) = game.world.as_mut() {
                let chunk = ChunkPos::new(packet.chunk_x, packet.chunk_z);
                let changes = packet.records.iter().map(|record| {
                    (
                        record.local_position(),
                        BlockState::from_wire(record.block_state),
                    )
                });
                game.dirty.extend(world.apply_block_changes(chunk, changes));
            }
            // The server confirms a dig or a placement with a single block
            // change, but a batch carries the same information.
            for record in &packet.records {
                let (x, y, z) = record.local_position();
                let position = BlockPos::new(
                    packet.chunk_x * 16 + i32::from(x),
                    i32::from(y),
                    packet.chunk_z * 16 + i32::from(z),
                );
                acknowledge_block(game, position, record.block_state);
            }
        }
        PlayEvent::UpdateHealth(packet) => {
            game.health = packet.health;
            game.food = packet.food as f32;
            game.hud_dirty = true;
        }
        PlayEvent::HeldItemChange(slot) => {
            if (0..9).contains(&slot) {
                game.selected_slot = slot as u8;
                game.hud_dirty = true;
            }
        }
        PlayEvent::Respawn(packet) => {
            // The server keeps the chunks of the dimension, so only a change
            // of the dimension replaces the world.
            let dimension = packet.dimension as i8;
            let changed = game
                .world
                .as_ref()
                .is_none_or(|world| world.dimension() != dimension);
            if changed {
                game.world = Some(World::new(dimension));
                game.dirty.clear();
                game.entities.clear();
                // The new dimension has its own clock; the first `Time
                // Update` of the new world arrives right after the respawn.
                game.world_time = 0;
                let _ = frames.try_send(RenderEvent::Reset);
            }
            // A same-dimension respawn keeps the current world clock.
            game.dimension = dimension;
            game.state = None;
            game.status = "RESPAWN".to_string();
            // The player comes back with a full health (MCP
            // `NetHandlerPlayClient.handleRespawn`).
            game.health = 20.0;
            game.food = 20.0;
            game.hud_dirty = true;
            game.status_data.status = game.status.clone();
            publish_status(game, frames);
        }
        PlayEvent::EntityTeleport(packet) => {
            if packet.entity_id == game.entity_id {
                let correction = ServerCorrection::new(
                    f64::from(packet.x) / 32.0,
                    f64::from(packet.y) / 32.0,
                    f64::from(packet.z) / 32.0,
                    f32::from(packet.yaw) * 360.0 / 256.0,
                    f32::from(packet.pitch) * 360.0 / 256.0,
                    packet.on_ground,
                );
                apply_correction(game, &correction, queue);
            } else {
                let pos = DVec3::new(
                    f64::from(packet.x) / 32.0,
                    f64::from(packet.y) / 32.0,
                    f64::from(packet.z) / 32.0,
                );
                let yaw = f32::from(packet.yaw) * 360.0 / 256.0;
                let pitch = f32::from(packet.pitch) * 360.0 / 256.0;
                game.entities.teleport(
                    EntityId(packet.entity_id),
                    pos,
                    yaw,
                    pitch,
                    packet.on_ground,
                );
            }
        }
        PlayEvent::EntityEquipment(packet) => {
            if packet.entity_id == game.entity_id {
                let slot = usize::try_from(packet.slot).unwrap_or(usize::MAX);
                if slot < game.equipment.len() {
                    game.equipment[slot] = Some(packet.item.clone());
                    if slot == 0 {
                        game.held_item = Some(packet.item.clone());
                    }
                    game.armor = f32::from(total_armor(&game.equipment));
                    game.hud_dirty = true;
                }
            } else if packet.slot == 0 {
                let item_id = if packet.item.item_id > 0 {
                    Some(packet.item.item_id as u16)
                } else {
                    None
                };
                game.entities
                    .set_held_item(EntityId(packet.entity_id), item_id);
            }
        }
        PlayEvent::SpawnPlayer(p) => {
            let pos = DVec3::new(
                f64::from(p.x) / 32.0,
                f64::from(p.y) / 32.0,
                f64::from(p.z) / 32.0,
            );
            let yaw = f32::from(p.yaw) * 360.0 / 256.0;
            let pitch = f32::from(p.pitch) * 360.0 / 256.0;
            let held = if p.held_item > 0 {
                Some(p.held_item as u16)
            } else {
                None
            };
            let flags = EntityFlags::new(
                p.metadata.get_byte(0).unwrap_or(0),
                p.metadata.get_byte(10).unwrap_or(0),
            );
            game.entities
                .spawn_player(EntityId(p.entity_id), p.uuid, pos, yaw, pitch, held, flags);
        }
        PlayEvent::SpawnMob(m) => {
            let pos = DVec3::new(
                f64::from(m.x) / 32.0,
                f64::from(m.y) / 32.0,
                f64::from(m.z) / 32.0,
            );
            let yaw = f32::from(m.yaw) * 360.0 / 256.0;
            let pitch = f32::from(m.pitch) * 360.0 / 256.0;
            let head_yaw = f32::from(m.head_yaw) * 360.0 / 256.0;
            let vel = Vec3::new(
                f32::from(m.velocity_x) / 8000.0,
                f32::from(m.velocity_y) / 8000.0,
                f32::from(m.velocity_z) / 8000.0,
            );
            let flags = EntityFlags::new(
                m.metadata.get_byte(0).unwrap_or(0),
                m.metadata.get_byte(10).unwrap_or(0),
            );
            game.entities.spawn_mob(
                EntityId(m.entity_id),
                m.mob_type,
                pos,
                yaw,
                pitch,
                head_yaw,
                vel,
                flags,
            );
        }
        PlayEvent::SpawnObject(o) => {
            let pos = DVec3::new(
                f64::from(o.x) / 32.0,
                f64::from(o.y) / 32.0,
                f64::from(o.z) / 32.0,
            );
            let pitch = f32::from(o.pitch) * 360.0 / 256.0;
            let yaw = f32::from(o.yaw) * 360.0 / 256.0;
            let vel = Vec3::new(
                f32::from(o.velocity_x) / 8000.0,
                f32::from(o.velocity_y) / 8000.0,
                f32::from(o.velocity_z) / 8000.0,
            );
            game.entities
                .spawn_object(EntityId(o.entity_id), o.object_type, pos, pitch, yaw, vel);
        }
        PlayEvent::DestroyEntities(d) => {
            game.entities.destroy(&d.entity_ids);
        }
        PlayEvent::EntityRelativeMove(m) => {
            let delta = DVec3::new(
                f64::from(m.dx) / 32.0,
                f64::from(m.dy) / 32.0,
                f64::from(m.dz) / 32.0,
            );
            game.entities
                .move_relative(EntityId(m.entity_id), delta, m.on_ground);
        }
        PlayEvent::EntityLook(l) => {
            let yaw = f32::from(l.yaw) * 360.0 / 256.0;
            let pitch = f32::from(l.pitch) * 360.0 / 256.0;
            game.entities
                .look(EntityId(l.entity_id), yaw, pitch, l.on_ground);
        }
        PlayEvent::EntityLookMove(lm) => {
            let delta = DVec3::new(
                f64::from(lm.dx) / 32.0,
                f64::from(lm.dy) / 32.0,
                f64::from(lm.dz) / 32.0,
            );
            let yaw = f32::from(lm.yaw) * 360.0 / 256.0;
            let pitch = f32::from(lm.pitch) * 360.0 / 256.0;
            game.entities
                .look_move(EntityId(lm.entity_id), delta, yaw, pitch, lm.on_ground);
        }
        PlayEvent::EntityHeadLook(h) => {
            let head_yaw = f32::from(h.head_yaw) * 360.0 / 256.0;
            game.entities.set_head_yaw(EntityId(h.entity_id), head_yaw);
        }
        PlayEvent::EntityVelocity(v) => {
            let vel = Vec3::new(
                f32::from(v.velocity_x) / 8000.0,
                f32::from(v.velocity_y) / 8000.0,
                f32::from(v.velocity_z) / 8000.0,
            );
            game.entities.set_velocity(EntityId(v.entity_id), vel);
        }
        PlayEvent::EntityMetadata(m) => {
            let flags = EntityFlags::new(
                m.metadata.get_byte(0).unwrap_or(0),
                m.metadata.get_byte(10).unwrap_or(0),
            );
            game.entities.set_flags(EntityId(m.entity_id), flags);
        }
        PlayEvent::AttachEntity(_) | PlayEvent::EntityStatus(_) | PlayEvent::Animation(_) => {
            // Unused or visual-only 1.8 packets.
        }
        PlayEvent::SpawnPosition(packet) => {
            game.spawn = Some((packet.x, packet.y, packet.z));
        }
        PlayEvent::SetExperience(packet) => {
            game.experience = packet.experience;
            game.level = packet.level;
            game.hud_dirty = true;
        }
        PlayEvent::TimeUpdate(packet) => {
            // The signed value is kept as-is: a negative time means the
            // server froze the cycle. The client never advances the clock
            // between packets (ADR-0008 §4).
            game.world_time = packet.time_of_day;
        }
    }
}

/// The column and payload of a `Chunk Data` packet.
fn chunk_payload(packet: &ChunkData) -> (ChunkPos, ChunkPayload<'_>) {
    (
        ChunkPos::new(packet.chunk_x, packet.chunk_z),
        ChunkPayload {
            primary_bit_mask: packet.primary_bit_mask,
            ground_up: packet.ground_up,
            data: &packet.data,
        },
    )
}

/// The columns and payloads of a `Map Chunk Bulk` packet; every bulk entry
/// is a ground-up column.
fn bulk_payloads(packet: &MapChunkBulk) -> Vec<(ChunkPos, ChunkPayload<'_>)> {
    packet
        .chunks
        .iter()
        .map(|chunk| {
            (
                ChunkPos::new(chunk.chunk_x, chunk.chunk_z),
                ChunkPayload {
                    primary_bit_mask: chunk.primary_bit_mask,
                    ground_up: true,
                    data: &chunk.data,
                },
            )
        })
        .collect()
}

/// The sum of the armour points of the four armour pieces.
fn total_armor(equipment: &[Option<WireItemStack>; 5]) -> u8 {
    equipment
        .iter()
        .skip(1)
        .filter_map(|item| item.as_ref())
        .filter_map(|item| armor_points(item.item_id))
        .sum()
}

/// Counts a block change the server confirmed.
fn acknowledge_block(game: &mut Game, position: BlockPos, block_state: u16) {
    tracing::debug!(?position, block_state, "block change applied");
    if game.expected_break == Some(position) {
        game.expected_break = None;
        game.status_data.breaks_acknowledged += 1;
        tracing::info!(
            ?position,
            block_state,
            "the server confirmed the block break"
        );
    }
    if game.expected_placement == Some(position) {
        game.expected_placement = None;
        game.status_data.placements_acknowledged += 1;
        tracing::info!(
            ?position,
            block_state,
            "the server confirmed the block placement"
        );
    }
}

/// Applies a server placement.
fn apply_placement(
    game: &mut Game,
    placement: &umicraft_net::play::PlayerPlacement,
    queue: &mut CommandQueue,
) {
    let yaw = placement.yaw.to_radians();
    let pitch = placement.pitch.to_radians();
    let position = DVec3::new(placement.x, placement.y, placement.z);

    match game.state.as_mut() {
        None => {
            game.state = Some(GameState::new(position));
            game.yaw = yaw;
            game.pitch = pitch;
        }
        Some(state) => {
            let correction = ServerCorrection::new(
                placement.x,
                placement.y,
                placement.z,
                yaw,
                pitch,
                state.player().on_ground,
            );
            let predicted = state.position();
            if let Some(world) = game.world.as_ref() {
                state.apply_correction(&correction, world);
            }
            game.yaw = state.player().yaw;
            game.pitch = state.player().pitch;
            game.status_data.resyncs += 1;
            tracing::debug!(
                ?predicted,
                authoritative = ?position,
                delta = ?(predicted - position),
                "the server corrected the position",
            );
        }
    }

    // The server expects the acknowledgement of the placement.
    //
    // The acknowledgement is a position packet itself, and the server
    // answers every position packet it does not accept with a resync. A
    // server that keeps resyncing the same position (for example because the
    // player stands inside a block) would therefore get an acknowledgement
    // per resync, and the two would feed each other until the packet limiter
    // of the server kicks the client. The client acknowledges a position only
    // once.
    let acknowledged = game.last_acknowledged.is_some_and(|(x, y, z)| {
        (x - placement.x).abs() < 1e-6
            && (y - placement.y).abs() < 1e-6
            && (z - placement.z).abs() < 1e-6
    });
    if !acknowledged {
        game.last_acknowledged = Some((placement.x, placement.y, placement.z));
        queue.push(ClientCommand::AckPosition {
            x: placement.x,
            y: placement.y,
            z: placement.z,
            yaw: placement.yaw,
            pitch: placement.pitch,
            on_ground: true,
        });
    }
    game.last_position = None;
}

/// Applies an authoritative correction from the server.
fn apply_correction(game: &mut Game, correction: &ServerCorrection, queue: &mut CommandQueue) {
    if let Some(state) = game.state.as_mut() {
        if let Some(world) = game.world.as_ref() {
            state.apply_correction(correction, world);
        }
        game.yaw = state.player().yaw;
        game.pitch = state.player().pitch;
    }
    queue.push(ClientCommand::AckPosition {
        x: correction.x,
        y: correction.y,
        z: correction.z,
        yaw: correction.yaw.to_degrees(),
        pitch: correction.pitch.to_degrees(),
        on_ground: correction.on_ground,
    });
    game.last_position = None;
}

/// Handles one input update.
fn handle_input(game: &mut Game, update: InputUpdate, queue: &mut CommandQueue) {
    game.input = update.state;
    game.yaw = (game.yaw + update.look_yaw).rem_euclid(std::f32::consts::TAU);
    game.pitch = (game.pitch + update.look_pitch).clamp(-MAX_PITCH, MAX_PITCH);

    if let Some(slot) = update.select_slot {
        if slot < 9 && slot != game.selected_slot {
            game.selected_slot = slot;
            game.hud_dirty = true;
            queue.push(ClientCommand::HeldItemChange {
                slot: i16::from(slot),
            });
        }
    }

    if update.break_block {
        start_break(game, queue);
    }
    if update.place_block {
        place_block(game, queue);
    }
}

/// Starts breaking the targeted block.
fn start_break(game: &mut Game, queue: &mut CommandQueue) {
    let Some(hit) = game.target() else {
        return;
    };
    let face = facing(hit.face);
    queue.push(ClientCommand::PlayerDigging {
        status: DiggingStatus::StartDestroyBlock,
        x: hit.block.x,
        y: hit.block.y,
        z: hit.block.z,
        face,
    });
    game.digging = Some(Digging {
        block: hit.block,
        face,
        stop_at: game.status_data.ticks + BREAK_TICKS,
    });
    game.expected_break = Some(hit.block);
    let distance = game.spawn.map_or(0.0, |(x, _, z)| {
        let dx = f64::from(hit.block.x - x);
        let dz = f64::from(hit.block.z - z);
        (dx * dx + dz * dz).sqrt()
    });
    tracing::info!(block = ?hit.block, ?face, distance, "breaking a block");
}

/// Places a block against the targeted face.
fn place_block(game: &mut Game, queue: &mut CommandQueue) {
    let Some(hit) = game.target() else {
        return;
    };
    let face = facing(hit.face);
    let offset = face_offset(face);
    let target = BlockPos::new(
        hit.block.x + offset.x,
        hit.block.y + offset.y,
        hit.block.z + offset.z,
    );
    // A block inside the player would suffocate it (the server applies the
    // suffocation damage), so the placement is skipped.
    if let Some(state) = game.state.as_ref() {
        let player = state.player().bounding_box();
        let block = Aabb::new(
            DVec3::new(
                f64::from(target.x),
                f64::from(target.y),
                f64::from(target.z),
            ),
            DVec3::new(
                f64::from(target.x) + 1.0,
                f64::from(target.y) + 1.0,
                f64::from(target.z) + 1.0,
            ),
        );
        if player.intersects(&block) {
            tracing::debug!(?target, "the placement would hit the player");
            return;
        }
    }
    let item = game.held_item.clone().unwrap_or(WireItemStack::EMPTY);
    queue.push(ClientCommand::PlayerBlockPlacement {
        x: hit.block.x,
        y: hit.block.y,
        z: hit.block.z,
        face,
        item,
        cursor_x: cursor(hit.position.x - f64::from(hit.block.x)),
        cursor_y: cursor(hit.position.y - f64::from(hit.block.y)),
        cursor_z: cursor(hit.position.z - f64::from(hit.block.z)),
    });
    game.expected_placement = Some(target);
    tracing::info!(
        block = ?hit.block,
        ?face,
        ?target,
        held = ?game.held_item,
        target_block = ?game.world.as_ref().map(|world| world.block(target)),
        "placing a block",
    );
}

/// The offset of a face, in blocks.
fn face_offset(face: Facing) -> BlockPos {
    match face {
        Facing::Down => BlockPos::new(0, -1, 0),
        Facing::Up => BlockPos::new(0, 1, 0),
        Facing::North => BlockPos::new(0, 0, -1),
        Facing::South => BlockPos::new(0, 0, 1),
        Facing::West => BlockPos::new(-1, 0, 0),
        Facing::East => BlockPos::new(1, 0, 0),
    }
}

/// The click offset in sixteenths of a block.
fn cursor(offset: f64) -> u8 {
    let value = (offset * 16.0).floor();
    if value <= 0.0 {
        0
    } else if value >= 15.0 {
        15
    } else {
        value as u8
    }
}

/// The wire face of a ray-cast face.
fn facing(face: umicraft_game::raycast::Face) -> Facing {
    match face {
        umicraft_game::raycast::Face::West => Facing::West,
        umicraft_game::raycast::Face::East => Facing::East,
        umicraft_game::raycast::Face::Down => Facing::Down,
        umicraft_game::raycast::Face::Up => Facing::Up,
        umicraft_game::raycast::Face::North => Facing::North,
        umicraft_game::raycast::Face::South => Facing::South,
    }
}

/// Runs one simulation tick.
fn tick(
    game: &mut Game,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
    chunks: &SyncSender<ChunkEvent>,
    tick_time: Instant,
) {
    game.status_data.ticks += 1;

    // The intent of the tick: the raw key state with the yaw and the pitch
    // the player is looking at.
    let intent = RawInput {
        forward: game.input.forward,
        strafe: game.input.strafe,
        jump: game.input.jump,
        sneak: game.input.sneak,
        sprint: game.input.sprint,
        yaw: game.yaw,
        pitch: game.pitch,
    }
    .to_intent();

    // The simulation only runs on a loaded world.
    let ready = game.world_ready();
    if ready {
        if let (Some(state), Some(world)) = (game.state.as_mut(), game.world.as_ref()) {
            state.tick(&intent, world);
        }
    }
    game.entities.tick();
    // A player inside a block is pushed out sideways by the simulation
    // itself, like the vanilla client (`umicraft_game::physics`); it is
    // never lifted.

    // The input packet carries the raw wire values; the 0.98 damping of
    // `EntityLivingBase.java:2020` is applied by the game logic and by the
    // server, so it must not be applied twice. It is only sent once the
    // server is in the play state and the player is placed: a play packet
    // that arrives during the login makes the server log `Bad packet id`.
    if game.playing && game.state.is_some() {
        queue.push(ClientCommand::Input {
            strafe: intent.strafe,
            forward: intent.forward,
            jumping: intent.jump,
            sneaking: intent.sneak,
        });
    }

    report_position(game, queue);
    finish_break(game, queue);
    publish_chunks(game, chunks);

    publish_frame(game, frames, tick_time);
    if game.hud_dirty {
        // The HUD is only published again when the last one was accepted, so
        // a full queue does not lose a change.
        if frames
            .try_send(RenderEvent::Hud(Box::new(game.hud())))
            .is_ok()
        {
            game.hud_dirty = false;
        }
    }
    if game.status_data.ticks.is_multiple_of(STATUS_PERIOD) {
        publish_status(game, frames);
    }
}

/// Sends the position report of the tick.
///
/// The report follows MCP `EntityPlayerSP.onUpdateWalkingPlayer`: the
/// position is sent when it moved by more than three centimetres or when
/// twenty ticks passed, and the view angles are sent when they changed.
/// Walking covers about 0.22 blocks per tick, so the delta of a report stays
/// well below the 0.25 blocks the server tolerates (ADR-0005 §A.5).
fn report_position(game: &mut Game, queue: &mut CommandQueue) {
    let Some(state) = game.state.as_ref() else {
        return;
    };
    let position = state.position();
    let on_ground = state.player().on_ground;

    let moved = match game.last_position {
        Some((x, y, z)) => {
            let dx = position.x - x;
            let dy = position.y - y;
            let dz = position.z - z;
            dx * dx + dy * dy + dz * dz > MOVE_EPSILON
        }
        None => true,
    };
    let looked = game.last_rotation.0 != game.yaw || game.last_rotation.1 != game.pitch;
    let timed_out = game.ticks_since_report >= 20;

    if moved || timed_out {
        if looked {
            queue.push(ClientCommand::PositionLook {
                x: position.x,
                y: position.y,
                z: position.z,
                yaw: game.yaw.to_degrees(),
                pitch: game.pitch.to_degrees(),
                on_ground,
            });
        } else {
            queue.push(ClientCommand::PlayerPosition {
                x: position.x,
                y: position.y,
                z: position.z,
                on_ground,
            });
        }
        game.last_position = Some((position.x, position.y, position.z));
        game.last_rotation = (game.yaw, game.pitch);
        game.ticks_since_report = 0;
        game.status_data.position_reports += 1;
    } else if looked {
        queue.push(ClientCommand::PlayerLook {
            yaw: game.yaw.to_degrees(),
            pitch: game.pitch.to_degrees(),
            on_ground,
        });
        game.last_rotation = (game.yaw, game.pitch);
    }
    game.ticks_since_report += 1;
}

/// Sends the stop packet of a running block break.
fn finish_break(game: &mut Game, queue: &mut CommandQueue) {
    let Some(digging) = game.digging else {
        return;
    };
    if game.status_data.ticks < digging.stop_at {
        return;
    }
    game.digging = None;
    queue.push(ClientCommand::PlayerDigging {
        status: DiggingStatus::StopDestroyBlock,
        x: digging.block.x,
        y: digging.block.y,
        z: digging.block.z,
        face: digging.face,
    });
}

/// Sends the snapshots of the chunks whose geometry changed.
fn publish_chunks(game: &mut Game, chunks: &SyncSender<ChunkEvent>) {
    if game.dirty.is_empty() {
        return;
    }
    let Some(world) = game.world.as_ref() else {
        game.dirty.clear();
        return;
    };
    let dirty: Vec<ChunkPos> = game.dirty.iter().copied().collect();
    for pos in dirty {
        if world.chunk(pos).is_none() {
            game.dirty.remove(&pos);
            continue;
        }
        let snapshot = ChunkSnapshot::from_world(world, pos);
        match chunks.try_send(ChunkEvent { pos, snapshot }) {
            Ok(()) => {
                game.dirty.remove(&pos);
            }
            Err(mpsc::TrySendError::Full(_)) => {
                // The render thread is behind: the chunk stays dirty and the
                // snapshot is derived again on a later tick.
                break;
            }
            Err(mpsc::TrySendError::Disconnected(_)) => break,
        }
    }
}

/// Publishes the statistics of the session.
fn publish_status(game: &mut Game, frames: &SyncSender<RenderEvent>) {
    game.status_data.status = game.status.clone();
    game.status_data.chunks = game.world.as_ref().map_or(0, World::chunk_count);
    let _ = frames.try_send(RenderEvent::Status(Box::new(game.status_data.clone())));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::InputState;
    use umicraft_game::raycast::Face;

    #[test]
    fn the_armour_points_follow_the_reference_table() {
        // Leather helmet, chainmail chestplate, iron leggings, diamond boots,
        // gold helmet.
        assert_eq!(armor_points(298), Some(1));
        assert_eq!(armor_points(303), Some(5));
        assert_eq!(armor_points(308), Some(5));
        assert_eq!(armor_points(313), Some(3));
        assert_eq!(armor_points(314), Some(2));
        assert_eq!(armor_points(297), None);
        assert_eq!(armor_points(-1), None);
    }

    #[test]
    fn the_equipment_adds_up_to_the_armour_value() {
        let mut equipment: [Option<WireItemStack>; 5] = std::array::from_fn(|_| None);
        equipment[1] = Some(WireItemStack {
            item_id: 310,
            stack_size: 1,
            damage: 0,
            nbt: None,
        });
        equipment[2] = Some(WireItemStack {
            item_id: 311,
            stack_size: 1,
            damage: 0,
            nbt: None,
        });
        assert_eq!(total_armor(&equipment), 11);
    }

    #[test]
    fn the_cursor_is_clamped_to_a_block() {
        assert_eq!(cursor(-0.5), 0);
        assert_eq!(cursor(0.0), 0);
        assert_eq!(cursor(0.5), 8);
        assert_eq!(cursor(1.5), 15);
    }

    #[test]
    fn the_raycast_faces_map_to_the_wire_faces() {
        assert_eq!(facing(Face::Up), Facing::Up);
        assert_eq!(facing(Face::Down), Facing::Down);
        assert_eq!(facing(Face::North), Facing::North);
        assert_eq!(facing(Face::South), Facing::South);
        assert_eq!(facing(Face::West), Facing::West);
        assert_eq!(facing(Face::East), Facing::East);
    }

    #[test]
    fn the_face_offsets_point_away_from_the_block() {
        assert_eq!(face_offset(Facing::Up), BlockPos::new(0, 1, 0));
        assert_eq!(face_offset(Facing::West), BlockPos::new(-1, 0, 0));
    }

    #[test]
    fn the_position_report_follows_the_vanilla_rule() {
        let mut game = Game::new();
        game.state = Some(GameState::new(DVec3::new(0.0, 4.0, 0.0)));
        let mut queue = CommandQueue::default();

        // The first report always happens.
        report_position(&mut game, &mut queue);
        assert_eq!(game.status_data.position_reports, 1);

        // Standing still does not report again before the twenty ticks are
        // over.
        for _ in 0..19 {
            report_position(&mut game, &mut queue);
        }
        assert_eq!(game.status_data.position_reports, 1);
        report_position(&mut game, &mut queue);
        assert_eq!(game.status_data.position_reports, 2);
    }

    #[test]
    fn a_moving_player_reports_every_tick() {
        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let world = umicraft_world::World::new(0);
        let intent = RawInput {
            forward: 1.0,
            ..RawInput::default()
        }
        .to_intent();
        let mut state = GameState::new(DVec3::new(0.0, 4.0, 0.0));
        game.state = Some(state.clone());
        // The first tick does not move the player: the motion epsilon drops
        // the zero gravity motion.
        state.tick(&intent, &world);
        game.state = Some(state.clone());
        report_position(&mut game, &mut queue);
        let baseline = game.status_data.position_reports;
        for _ in 0..10 {
            state.tick(&intent, &world);
            game.state = Some(state.clone());
            report_position(&mut game, &mut queue);
        }
        assert_eq!(game.status_data.position_reports, baseline + 10);
    }

    #[test]
    fn the_input_update_turns_the_view() {
        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        handle_input(
            &mut game,
            InputUpdate {
                state: InputState {
                    forward: 1.0,
                    ..InputState::default()
                },
                look_yaw: 0.5,
                look_pitch: 0.25,
                ..InputUpdate::default()
            },
            &mut queue,
        );
        assert!((game.yaw - 0.5).abs() < 1e-6);
        assert!((game.pitch - 0.25).abs() < 1e-6);
        assert_eq!(game.input.forward, 1.0);
    }

    #[test]
    fn the_pitch_is_clamped_to_the_vanilla_limit() {
        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        for _ in 0..100 {
            handle_input(
                &mut game,
                InputUpdate {
                    look_pitch: 0.5,
                    ..InputUpdate::default()
                },
                &mut queue,
            );
        }
        assert!(game.pitch <= MAX_PITCH);
    }

    #[test]
    fn the_selected_slot_is_announced_once() {
        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        handle_input(
            &mut game,
            InputUpdate {
                select_slot: Some(3),
                ..InputUpdate::default()
            },
            &mut queue,
        );
        assert_eq!(game.selected_slot, 3);
        assert_eq!(queue.ordered.len(), 1);
        handle_input(
            &mut game,
            InputUpdate {
                select_slot: Some(3),
                ..InputUpdate::default()
            },
            &mut queue,
        );
        assert_eq!(queue.ordered.len(), 1);
    }

    #[test]
    fn a_full_channel_keeps_the_commands_for_the_next_flush() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let mut queue = CommandQueue {
            sender: Some(sender),
            ..CommandQueue::default()
        };
        queue.push(ClientCommand::Chat {
            message: "first".to_string(),
        });
        queue.push(ClientCommand::Chat {
            message: "second".to_string(),
        });
        queue.push(ClientCommand::PlayerPosition {
            x: 1.0,
            y: 64.0,
            z: 2.0,
            on_ground: true,
        });

        queue.flush();
        assert_eq!(queue.ordered.len(), 1, "the second chat waits");
        assert!(queue.position.is_some(), "the report waits");
        let Ok(PlayServerbound::Chat(chat)) = receiver.try_recv() else {
            panic!("the first chat is sent as its wire packet");
        };
        assert_eq!(chat.message, "first");

        queue.flush();
        let Ok(PlayServerbound::Chat(chat)) = receiver.try_recv() else {
            panic!("the second chat follows in order");
        };
        assert_eq!(chat.message, "second");
        assert!(queue.position.is_some(), "the channel is full again");

        queue.flush();
        assert!(matches!(
            receiver.try_recv(),
            Ok(PlayServerbound::PlayerPosition(_))
        ));
        assert!(queue.ordered.is_empty() && queue.position.is_none());
    }

    /// A grass floor (top at y = 64) with one block of `plant` standing on
    /// it at (0, 64, 0), and a player whose feet are at y = 64 there.
    fn game_on_floor_with(plant: BlockState) -> Game {
        let mut world = World::new(0);
        let payload = vec![0u8; umicraft_world::chunk::payload_len(0b1000, true, true)];
        world
            .apply_chunk(
                ChunkPos::new(0, 0),
                umicraft_world::ChunkPayload {
                    primary_bit_mask: 0b1000,
                    ground_up: true,
                    data: &payload,
                },
            )
            .expect("an empty column");
        for x in 0..16 {
            for z in 0..16 {
                world.set_block(BlockPos::new(x, 63, z), BlockState::new(2, 0));
            }
        }
        world.set_block(BlockPos::new(0, 64, 0), plant);
        let mut game = Game::new();
        game.world = Some(world);
        game.state = Some(GameState::new(DVec3::new(0.5, 64.0, 0.5)));
        game
    }

    /// Regression: every tick lifted a player standing in a plant, in water
    /// or under leaves onto the top of the block, which put players on
    /// flowers and on top of trees.
    #[test]
    fn a_player_in_plants_or_water_is_never_lifted() {
        for plant in [
            BlockState::new(31, 1),
            BlockState::new(38, 0),
            BlockState::new(9, 0),
        ] {
            let mut game = game_on_floor_with(plant);
            let mut queue = CommandQueue::default();
            let (frames, _frames_rx) = mpsc::sync_channel(64);
            let (chunks, _chunks_rx) = mpsc::sync_channel(64);
            for _ in 0..10 {
                tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
            }
            let feet = game.state.as_ref().expect("state").position();
            assert_eq!(feet.y, 64.0, "lifted out of {plant:?}");
        }
    }

    #[test]
    fn the_position_reports_are_coalesced() {
        let mut queue = CommandQueue::default();
        for step in 0..10 {
            queue.push(ClientCommand::PlayerPosition {
                x: f64::from(step),
                y: 0.0,
                z: 0.0,
                on_ground: true,
            });
        }
        // Only the newest report is kept: a backlog of them would burst on
        // the socket and the server would kick the client.
        let Some(ClientCommand::PlayerPosition { x, .. }) = queue.position else {
            panic!("the report must be a position packet");
        };
        assert_eq!(x, 9.0);
    }

    #[test]
    fn the_input_packets_are_coalesced() {
        let mut queue = CommandQueue::default();
        for step in 0..10 {
            queue.push(ClientCommand::Input {
                strafe: 0.0,
                forward: step as f32,
                jumping: false,
                sneaking: false,
            });
        }
        let Some(ClientCommand::Input { forward, .. }) = queue.input else {
            panic!("the input must be kept");
        };
        assert_eq!(forward, 9.0);
    }

    #[test]
    fn a_time_update_sets_the_world_clock_and_the_frame_carries_it() {
        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, received) = mpsc::sync_channel(4);
        game.state = Some(GameState::new(DVec3::new(0.0, 64.0, 0.0)));

        handle_net(
            &mut game,
            PlayEvent::TimeUpdate(umicraft_protocol::packets::play::clientbound::TimeUpdate {
                world_age: 72_770,
                time_of_day: -6_000,
            }),
            &mut queue,
            &frames,
        );
        assert_eq!(game.world_time, -6_000);

        publish_frame(&game, &frames, Instant::now());
        let RenderEvent::Frame(frame) = received.try_recv().expect("a frame") else {
            panic!("the published event must be a frame");
        };
        assert_eq!(frame.world_time, -6_000);
        assert_eq!(frame.dimension, 0);
        // `Frame` is cheap to clone and send across threads without taking locks.
        fn assert_send_sync<T: Send + Sync + Clone + 'static>() {}
        assert_send_sync::<Frame>();
    }

    #[test]
    fn joining_and_changing_dimension_reset_the_world_clock() {
        use umicraft_net::play::JoinGameInfo;
        use umicraft_protocol::packets::play::clientbound::{Respawn, TimeUpdate};

        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        let join = |dimension| JoinGameInfo {
            entity_id: 1,
            gamemode: 0,
            dimension,
            difficulty: 0,
            max_players: 20,
            level_type: "default".to_string(),
            reduced_debug_info: false,
        };
        let respawn = |dimension| Respawn {
            dimension,
            difficulty: 0,
            game_mode: 0,
            world_type: "default".to_string(),
        };

        handle_net(&mut game, PlayEvent::JoinGame(join(0)), &mut queue, &frames);
        assert_eq!(game.world_time, 0);
        assert_eq!(game.dimension, 0);

        handle_net(
            &mut game,
            PlayEvent::TimeUpdate(TimeUpdate {
                world_age: 1,
                time_of_day: -6_000,
            }),
            &mut queue,
            &frames,
        );
        assert_eq!(game.world_time, -6_000);

        // A same-dimension respawn keeps the frozen clock.
        handle_net(
            &mut game,
            PlayEvent::Respawn(respawn(0)),
            &mut queue,
            &frames,
        );
        assert_eq!(game.world_time, -6_000);
        assert_eq!(game.dimension, 0);

        // A dimension change starts a new clock.
        handle_net(
            &mut game,
            PlayEvent::Respawn(respawn(-1)),
            &mut queue,
            &frames,
        );
        assert_eq!(game.world_time, 0);
        assert_eq!(game.dimension, -1);
    }
}
