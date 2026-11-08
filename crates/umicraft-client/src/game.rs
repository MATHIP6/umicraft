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
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::entity_render::fishing::first_person_tip;
use crate::pack_offer::ServerPackRequest;
use crate::social_view;
use glam::{DVec3, Vec3};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::Sender as CommandSender;
use umicraft_game::chat::{self as chat_rules, commands, Position};
use umicraft_game::combat::{hurt_roll, ClickViewer, HitStar};
use umicraft_game::digging::{
    enchantment_level, Digger, AQUA_AFFINITY, DEPTH_STRIDER, HASTE, MINING_FATIGUE,
};
use umicraft_game::entity::leash::{leash_entities, LeashFrame};
use umicraft_game::entity::light::{eye_light, light_entities};
use umicraft_game::entity::nametag::mob_tag;
use umicraft_game::entity::painting::light_paintings;
use umicraft_game::entity::rail::rail_entities;
use umicraft_game::entity::shadow::shadow_entities;
use umicraft_game::entity::{
    EntityFlags, EntityFrameData, EntityId, EntityItem, EntityKind, EntityRecord, EntityStore,
};
use umicraft_game::gamemode::{GameMode, GAMEMODE_REASON};
use umicraft_game::hand::{HandPose, HandState, SwingEffects};
use umicraft_game::hud::boss_bar::BossBar;
use umicraft_game::hud::team_names::{player_label, sees_team_ghost};
use umicraft_game::input::RawInput;
use umicraft_game::interaction::{AttackTick, BlockAck, Interaction, Outgoing, PositionAck};
use umicraft_game::inventory::creative::CreativeScreen;
use umicraft_game::inventory::{WindowKind, WindowManager, WindowView};
use umicraft_game::item_use::{drop_item, DoorToggle, ItemUse, UseAction, UseTick};
use umicraft_game::movement::movement_stats;
use umicraft_game::particles::{EffectContext, SpawnFilter};
use umicraft_game::physics::{DEFAULT_FLY_SPEED, DEFAULT_WALK_SPEED, SPRINT_FOOD_LIMIT};
use umicraft_game::pick::{pick_block as pick_in, picked_item};
use umicraft_game::piston::{motion, MovingBlocks};
use umicraft_game::player_list::PlayerList;
use umicraft_game::raycast::{look_direction, raycast, RayHit};
use umicraft_game::scoreboard::Scoreboard;
use umicraft_game::server::block_entities::EntityTable;
use umicraft_game::server::block_entities::SignTable;
use umicraft_game::server::chest_lids::ChestLids;
use umicraft_game::server::maps::MapStore;
use umicraft_game::server::ServerState;
use umicraft_game::shapes::selection_shape;
use umicraft_game::shapes::Aabb;
use umicraft_game::sound::cue::SoundCue;
use umicraft_game::title::{ActionBar, TitleState};
use umicraft_game::traffic::{ServerView, SteerInput, TickPacket, TickSnapshot};
use umicraft_game::view::{camera_pose, Perspective};
use umicraft_game::{GameState, InputIntent, RenderState, ServerCorrection};
use umicraft_mesh::ChunkSnapshot;
use umicraft_net::play::{PlayEvent, SessionCounters};
use umicraft_protocol::packets::play::clientbound::player_list_item::PlayerListItem;
use umicraft_protocol::packets::play::clientbound::{
    BlockBreakAnim, ChunkData, Explosion, MapChunkBulk, UseBed,
};
use umicraft_protocol::packets::play::serverbound::ResourcePackResult;
use umicraft_protocol::packets::play::serverbound::UseEntityAction;
use umicraft_protocol::packets::play::Metadata;
use umicraft_protocol::PlayServerbound;
use umicraft_render::{BorderView, ChatLineView, FogContext, HudState, OutlineBox};
use umicraft_world::{BlockPos, BlockState, ChunkPayload, ChunkPos, LightLevel, World};

use crate::connection_end::ConnectionEnd;
use crate::cracks::Crack;
use crate::creative_input::CreativeEvent;
use crate::creative_view::CreativeView;
use crate::error::ClientError;
use crate::fog_context::fog_context;
use crate::input::InputUpdate;
use crate::inventory_input::{ContainerAction, InventoryAction};
use crate::network::{self, ClientCommand, NetworkConfig};
use crate::profiles::profile_from_properties;
use umicraft_game::chat::lang::LangTable;
use umicraft_game::chat::unformatted_text;
use umicraft_game::editors::book::{BookPacket, WRITABLE_BOOK, WRITTEN_BOOK};
use umicraft_game::inventory::payload;
use umicraft_protocol::packets::play::ItemStack;
use umicraft_ui::menus::ConnectStage;

#[path = "combat.rs"]
mod combat;
#[path = "creative_events.rs"]
mod creative_events;
use creative_events::apply_creative_event;
#[path = "container_events.rs"]
mod container_events;
use container_events::{apply_container_action, apply_trade_list};
#[path = "inventory_events.rs"]
mod inventory_events;
use inventory_events::apply_inventory_action;

/// The tick period of the simulation: 20 Hz, as the 1.8 server (spec GAM-1).
pub const TICK: Duration = Duration::from_millis(50);

/// Bounded inbox of the game thread.
///
/// The world packets are the bulk (a chunk is up to 100 KiB), so the queue is
/// sized like the M2 network queue: when it is full the network thread
/// blocks and the TCP backpressure slows the server down.
const INBOX_CAPACITY: usize = 256;

/// The barrier item (id 166), whose holder in creative sees barrier blocks.
const BARRIER_ITEM: i16 = 166;

/// The `Effect` id of a block breaking (`RenderGlobal.playAuxSFX`, case 2001).
const BLOCK_BREAK_EFFECT: i32 = 2001;

/// The seed of the particle random source; particles are cosmetic, so a fixed
/// seed only makes a session repeatable.
const PARTICLE_SEED: u64 = 0x5041_5254;

/// Bounded queue of the frames the render thread polls.
const FRAME_CAPACITY: usize = 32;

/// Bounded queue of the chunk snapshots the render thread polls.
const CHUNK_CAPACITY: usize = 64;

/// The number of chat messages kept (MCP GuiNewChat keeps 100). The HUD
/// wraps them and shows the newest rows that fit.
const CHAT_HISTORY: usize = 100;

/// The arrow item id (MCP `Items.arrow`): a bow needs one to be drawn.
const ARROW: i16 = 262;

/// The `Entity Status` that ends the use of an item
/// (MCP `EntityPlayer.handleStatusUpdate`, id 9).
const ITEM_USE_FINISHED: i8 = 9;

/// The block ids of water (MCP `Blocks.flowing_water`, `Blocks.water`).
const WATER_FLOWING: u16 = 8;
const WATER_STILL: u16 = 9;

/// A `Block Break Animation` stage that removes the cracks.
const CLEAR_BREAK_STAGE: u8 = 255;

/// The ticks the death animation runs before it stops
/// (MCP `EntityLivingBase.onDeathUpdate`: `deathTime == 20`).
const DEATH_TICKS: u32 = 20;

/// The data watcher index of the player score (MCP `EntityPlayer` line 160).
const SCORE_WATCHER: u8 = 18;
const ABSORPTION_WATCHER: u8 = 17;

/// Data watcher of the stack of a dropped item (MCP `EntityItem`, watcher 10).
const DROPPED_ITEM_WATCHER: u8 = 10;

/// Data watcher of the stack inside an item frame (MCP `EntityItemFrame`, 8).
const FRAME_ITEM_WATCHER: u8 = 8;

/// Data watcher of the rotation step inside an item frame (MCP
/// `EntityItemFrame`, 9); only the low three bits are meaningful.
const FRAME_ROTATION_WATCHER: u8 = 9;

/// The wire units of `Entity Velocity` per block per tick
/// (MCP `handleEntityVelocity`: `/ 8000`).
const VELOCITY_UNITS: f64 = 8000.0;

/// The number of ticks between two status updates.
const STATUS_PERIOD: u64 = 20;

/// The largest pitch the view may reach, in radians.
const MAX_PITCH: f32 = 89.0_f32.to_radians();

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
    Disconnected(ConnectionEnd),
    /// The connection attempt reached a new stage (`connect.authorizing`
    /// once the socket is open).
    ConnectStage(ConnectStage),
    /// An input update from an input source.
    Input(InputUpdate),
    /// A chat line the player typed.
    Chat(String),
    /// The player pressed Tab: ask the server to complete this text.
    TabComplete(String),
    /// How the player or the download handled the server's resource pack
    /// offer; becomes `Resource Pack Status`.
    ServerPackStatus {
        /// The hash of the offer, as the server sent it.
        hash: String,
        /// How it went.
        result: ResourcePackResult,
    },
    /// The player changed an option the server must know: the render
    /// distance, the chat visibility, the chat colours or the skin layers.
    ClientSettings {
        /// The render distance in chunks.
        view_distance: u8,
        /// The chat visibility byte.
        chat_mode: u8,
        /// Whether the chat shows colours.
        chat_colors: bool,
        /// The skin-parts byte.
        skin_parts: u8,
    },
    /// The player changed the particle option (`0`, `1` or `2`).
    ParticleSetting(u8),
    /// A line the client shows in its own chat (the completion list).
    LocalChat(String),
    /// Request to re-mesh every loaded chunk column (e.g. after a resource pack reload).
    ReMeshAll,
    /// The client closes a container or the player inventory (E / Escape).
    CloseInventory,
    /// A slot interaction of the open inventory or container screen.
    Inventory(InventoryAction),
    /// A click on a button or text change of a container screen.
    Container(ContainerAction),
    /// The player finished editing a sign.
    SignEdited {
        /// Block position of the sign.
        position: (i32, i32, i32),
        /// The four lines as chat-component JSON.
        lines: [String; 4],
    },
    /// The player finished with a book: the packet to send, if any.
    BookEdited(BookPacket),
    /// An interaction with the creative inventory screen.
    Creative(CreativeEvent),
    /// The player pressed "Respawn" on the death screen.
    Respawn,
    /// The statistics screen opened: ask the server for the counters.
    RequestStats,
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
    /// Whether the world is a superflat one (`WorldType.FLAT`), which moves the
    /// horizon of the sky and the void fog.
    pub flat_world: bool,
    /// The game mode of the local player; the music depends on it.
    pub gamemode: GameMode,
    /// The world spawn point the server announced, which the compass shows.
    pub spawn: Option<(i32, i32, i32)>,
    /// Per-entity interpolation data for the render thread.
    pub entities: Arc<[EntityFrameData]>,
    /// The body of the local player, always present: the inventory screen
    /// draws it (`GuiInventory.drawEntityOnScreen`).
    pub local_body: Arc<EntityFrameData>,
    /// The light at the eye of the local player, which lights the
    /// first-person item (MCP `ItemRenderer.func_178109_a`).
    pub hand_light: LightLevel,
    /// World-space boxes of the block selection outline; empty when the
    /// player looks at no block or is a spectator.
    pub selection: Arc<[OutlineBox]>,
    /// The cracks of every block being dug, by this player and by others.
    pub cracks: Arc<[Crack]>,
    /// The maps the server sent (copy-on-write pixels, so cloning is cheap).
    pub maps: Arc<MapStore>,
    /// The lid angles of the chests (P7c).
    pub chest_lids: Arc<ChestLids>,
    /// The blocks pistons are moving (P7d).
    pub moving_blocks: Arc<MovingBlocks>,
    /// The updates of skulls, banners, beacons, spawners and flower pots.
    pub block_entity_data: Arc<EntityTable>,
    /// The text of the signs, shared with the server state until one changes.
    pub signs: Arc<SignTable>,
    /// Base temperature of the biome at the player, which tints the sky.
    pub sky_temperature: f32,
    /// The medium at the eye and the effects that shape the fog (P7g).
    pub fog: FogContext,
    /// The world border of the dimension (P7g).
    pub border: BorderView,
    /// Rain, thunder, the lightning flash and the columns rain falls on (P7f).
    pub weather: crate::weather_view::WeatherView,
    /// The live particles with the light where they stand (P8).
    pub particles: Arc<[crate::particle_view::ParticleDraw]>,
    /// What the screen overlays need to decide what shows (P8-f).
    pub overlays: crate::overlay_state::OverlayInputs,
    /// The world facts the F3 screen prints (P9-h).
    pub debug: Arc<crate::debug_world::DebugWorld>,
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
    /// The server placed the player (the first `Player Position and Look`
    /// was applied). The window leaves the connecting screen on this.
    pub placed: bool,
}

/// What the mouse look changed since the newest tick.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    /// The view of the player: it replaces the newest frame's view without
    /// restarting the interpolation.
    pub view: RenderState,
    /// The pitch of the local body in degrees. `Entity.setAngles` moves the
    /// pitch and its previous value together, every frame, so the body looks
    /// where the mouse points at once; the yaw of the body and the head is
    /// the one of the tick, interpolated.
    pub body_pitch: f32,
}

/// The newest [`Look`], shared by the game thread and the render thread.
///
/// A mouse look overwrites it instead of queueing an event: the mouse sends
/// hundreds of looks a second, and as events they filled the frame queue until
/// the tick frames, which carry the body of the player and the entities, were
/// dropped.
pub type LookSlot = Arc<Mutex<Option<Look>>>;

/// A message the game thread sends to the render thread.
#[derive(Debug)]
pub enum RenderEvent {
    /// A simulation frame.
    Frame(Box<Frame>),
    /// The slot the mouse look goes to, sent once at the start of the session.
    LookSlot(LookSlot),
    /// The sounds the tick produced (footsteps, digging, rain, server
    /// effects), oldest first; the render thread hands them to the audio
    /// thread. Never empty.
    Sounds(Vec<SoundCue>),
    /// The HUD state changed.
    Hud(Box<HudState>),
    /// The inventory/container state changed.
    Inventory(Box<WindowView>),
    /// The creative inventory screen changed.
    Creative(Box<CreativeView>),
    /// The server or the held item asks for a book or sign screen.
    TextScreen(Box<TextScreenRequest>),
    /// The server offers a resource pack (`Resource Pack Send`).
    ServerPack(Box<ServerPackRequest>),
    /// The statistics changed.
    Status(Box<GameStatus>),
    /// The server answered a statistics request (`Statistics`, 0x37): every
    /// `(name, value)` pair it knows, by name.
    Stats(Vec<(String, i32)>),
    /// The connection attempt reached a new stage.
    Connect(ConnectStage),
    /// The world was replaced; the render thread drops its meshes.
    Reset,
    /// The session ended.
    Ended(ConnectionEnd),
}

/// A screen for editing text that the game thread asks the window to open.
#[derive(Debug, Clone, PartialEq)]
pub enum TextScreenRequest {
    /// The sign editor (`Sign Editor Open`).
    Sign {
        /// Block position of the sign.
        position: (i32, i32, i32),
        /// What the four lines read now.
        lines: [String; 4],
    },
    /// The book screen of a book and quill or a written book.
    Book {
        /// The held stack.
        stack: ItemStack,
        /// The name of the local player, the author of a signed book.
        author: String,
    },
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
        let own_name = config.name.clone();

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
            own_name,
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
    own_name: String,
) -> Result<JoinHandle<()>, ClientError> {
    std::thread::Builder::new()
        .name("umicraft-game".to_string())
        .spawn(move || run(events, frames, chunks, commands, stop, own_name))
        .map_err(|error| ClientError::Worker(format!("spawn game thread: {error}")))
}

/// The game state owned by the game thread.
struct Game {
    /// The loaded world; `None` before `Join Game`.
    world: Option<World>,
    /// The client-side particles (P8).
    particles: umicraft_game::particles::ParticleEngine,
    /// The ambient sources of the particles (blocks, rain, sprint dust).
    ambient: crate::particle_ambient::AmbientSpawner,
    /// How many particles are created (`GameSettings.particleSetting`).
    particle_setting: umicraft_game::particles::ParticleSetting,
    /// What the screen overlays need to know of the player (P8-f).
    overlays: crate::overlay_state::OverlayTracker,
    /// The chunks whose geometry changed and still need a snapshot.
    dirty: BTreeSet<ChunkPos>,
    /// The human-readable status.
    status: String,
    /// Why the session ended, once the network thread said so.
    end: Option<ConnectionEnd>,
    /// The entity id of the local player.
    entity_id: i32,
    /// The gamemode of the local player.
    gamemode: GameMode,
    /// The current dimension; `0` is the overworld.
    dimension: i8,
    /// Whether the world is a superflat one, from the world type the server
    /// names in `Join Game` and in the `Respawn` that changes the dimension.
    flat_world: bool,
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
    /// The score of the death screen (data watcher 18 of the local player).
    score: i32,
    /// The ticks since the death, up to [`DEATH_TICKS`]
    /// (MCP `EntityLivingBase.deathTime`).
    death_time: u32,
    /// The selected hotbar slot.
    selected_slot: u8,
    /// The chat messages, oldest first.
    chat: VecDeque<ChatLineView>,
    /// The chat visibility the server was told (`ClientSettings` byte).
    chat_mode: u8,
    /// Counts the tab-completion answers.
    completions_serial: u64,
    /// What the server was last told about the player's movement.
    server_view: ServerView,
    /// The vehicle the player rides (`Attach Entity`), `None` on foot.
    vehicle: Option<i32>,
    /// The dig, placement and acknowledgement rules.
    interaction: Interaction,
    /// The use of the held item (eating, blocking, drawing a bow).
    item_use: ItemUse,
    /// The use button went down since the last tick.
    use_pressed: bool,
    /// Where the camera sits (`F5`).
    perspective: Perspective,
    /// The attack button went down since the last tick.
    attack_pressed: bool,
    /// The animation state of the first-person hand.
    hand: HandState,
    /// The body of the local player, animated like any other player; drawn
    /// in the third-person perspectives.
    body: EntityRecord,
    /// Where the mouse look is published to the render thread.
    look: LookSlot,
    /// The statistics of the session.
    status_data: GameStatus,
    /// Whether the HUD has to be published again.
    hud_dirty: bool,
    /// The bursts of critical stars still going (`EntityParticleEmitter`).
    hit_emitters: Vec<umicraft_game::combat::HitEmitter>,
    /// The absorption hearts of the local player (entry 17).
    absorption: f32,
    /// Whether the world is hardcore (`Join Game`).
    hardcore: bool,
    /// The status rows, bars and held-item name of the HUD.
    hud_view: crate::hud_view::HudTracker,
    /// Entities in the world (mobs, players, objects).
    entities: EntityStore,
    /// The inventory and container window model.
    windows: WindowManager,
    /// A book the player used this tick, whose screen the window opens.
    pending_book: Option<ItemStack>,
    /// The player's own name; it picks the team sidebar of the scoreboard and
    /// the team rules of name tags.
    own_name: String,
    /// The tab list of the server.
    player_list: PlayerList,
    /// The scoreboard of the current world.
    scoreboard: Scoreboard,
    /// The title and subtitle.
    title: TitleState,
    /// The message above the hotbar.
    action_bar: ActionBar,
    /// The boss bar (`BossStatus`).
    boss_bar: BossBar,
    /// The completions the server offered for the open chat line.
    completions: Vec<String>,
    /// The state of the creative inventory screen.
    creative: CreativeScreen,
    /// The session state kept for later consumers (ADR-0020 section 3).
    server: ServerState,
}

impl Game {
    fn new() -> Self {
        Self {
            world: None,
            particles: umicraft_game::particles::ParticleEngine::new(PARTICLE_SEED),
            ambient: crate::particle_ambient::AmbientSpawner::default(),
            particle_setting: umicraft_game::particles::ParticleSetting::default(),
            overlays: crate::overlay_state::OverlayTracker::default(),
            dirty: BTreeSet::new(),
            status: "CONNECTING".to_string(),
            end: None,
            entity_id: 0,
            gamemode: GameMode::Survival,
            dimension: 0,
            flat_world: false,
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
            score: 0,
            death_time: 0,
            selected_slot: 0,
            chat: VecDeque::new(),
            chat_mode: 0,
            completions_serial: 0,
            server_view: ServerView::default(),
            vehicle: None,
            interaction: Interaction::new(),
            item_use: ItemUse::new(),
            pending_book: None,
            use_pressed: false,
            perspective: Perspective::default(),
            attack_pressed: false,
            hand: HandState::new(),
            body: EntityRecord::new(EntityId(0), EntityKind::Player, DVec3::ZERO, 0.0, 0.0),
            look: LookSlot::default(),
            status_data: GameStatus::default(),
            hud_dirty: true,
            hit_emitters: Vec::new(),
            absorption: 0.0,
            hardcore: false,
            hud_view: crate::hud_view::HudTracker::default(),
            entities: EntityStore::new(),
            windows: WindowManager::new(),
            own_name: String::new(),
            player_list: PlayerList::new(),
            scoreboard: Scoreboard::new(),
            title: TitleState::new(),
            action_bar: ActionBar::new(),
            boss_bar: BossBar::new(),
            completions: Vec::new(),
            creative: CreativeScreen::new(),
            server: ServerState::new(),
        }
    }

    /// The reach of the block interaction, in blocks.
    ///
    /// MCP `PlayerControllerMP.java:340-343`: 4.5 blocks in survival and 5.0
    /// in creative.
    fn reach(&self) -> f64 {
        self.gamemode.reach()
    }

    /// Whether the local player is dead (MCP `EntityLivingBase.getHealth() <= 0`).
    fn dead(&self) -> bool {
        self.health <= 0.0
    }

    /// The entity the view follows (`Camera`, 0x43), if it is another one
    /// than the local player.
    fn camera_entity(&self) -> Option<EntityId> {
        self.server.camera_target().map(EntityId)
    }

    /// True while the view follows another entity.
    fn spectating(&self) -> bool {
        self.camera_entity()
            .is_some_and(|id| self.entities.camera_view(id, 1.0).is_some())
    }

    /// The eye position of the player.
    fn eye(&self) -> Option<DVec3> {
        self.state
            .as_ref()
            .map(|state| state.player().eye_position())
    }

    /// The block the crosshair points at, entities aside.
    fn block_target(&self) -> Option<RayHit> {
        let eye = self.eye()?;
        let direction = look_direction(self.yaw, self.pitch);
        raycast(eye, direction, self.reach(), self.world.as_ref()?)
    }

    /// The block the crosshair points at: none when an entity is nearer
    /// (MCP `objectMouseOver`, see [`combat::pointed`]).
    fn target(&self) -> Option<RayHit> {
        combat::pointed(self).block()
    }

    /// The boxes of the selection outline of the targeted block.
    ///
    /// MCP `EntityRenderer.isDrawBlockOutline` hides the outline for a
    /// spectator unless a container is targeted (not modelled: spectators
    /// never get an outline) and, without `allowEdit` (adventure), unless the
    /// held item's `CanDestroy`/`CanPlaceOn` lists the block (item NBT is not
    /// read yet: adventure never gets an outline). `RenderGlobal.drawSelectionBox`
    /// draws the block's selected bounding box, offset to its position.
    fn selection_outline(&self) -> Vec<OutlineBox> {
        if matches!(self.gamemode, GameMode::Spectator | GameMode::Adventure) {
            return Vec::new();
        }
        let (Some(hit), Some(world)) = (self.target(), self.world.as_ref()) else {
            return Vec::new();
        };
        let origin = DVec3::new(
            f64::from(hit.block.x),
            f64::from(hit.block.y),
            f64::from(hit.block.z),
        );
        let state = world.block(hit.block);
        let shape = selection_shape(state, world, hit.block);
        shape
            .boxes()
            .iter()
            .map(|aabb| OutlineBox {
                min: (aabb.min + origin).as_vec3().to_array(),
                max: (aabb.max + origin).as_vec3().to_array(),
            })
            .collect()
    }

    /// The cracks of the blocks being dug (`Block Break Animation`, and the
    /// local dig which is recorded the same way).
    ///
    /// A block that is already air has no shape and no cracks.
    fn cracks(&self) -> Vec<Crack> {
        let Some(world) = self.world.as_ref() else {
            return Vec::new();
        };
        let mut cracks = Vec::new();
        for (_, progress) in self.server.block_entities().break_animations() {
            let (x, y, z) = progress.pos;
            let position = BlockPos::new(x, y, z);
            let origin = DVec3::new(f64::from(x), f64::from(y), f64::from(z));
            let shape = selection_shape(world.block(position), world, position);
            cracks.extend(shape.boxes().iter().map(|aabb| Crack {
                bounds: OutlineBox {
                    min: (aabb.min + origin).as_vec3().to_array(),
                    max: (aabb.max + origin).as_vec3().to_array(),
                },
                stage: progress.stage,
            }));
        }
        cracks
    }

    /// True when the chunk the player stands in is loaded.
    ///
    /// The simulation must not run without it: the collision solver would see
    /// air, the player would fall through the floor and the server would reject
    /// the position. Only that chunk counts (MCP `EntityPlayerSP.onUpdate`,
    /// `isBlockLoaded(posX, 0, posZ)`): a server that sends a single chunk, such
    /// as a proxy's waiting world, still sees the player fall.
    fn world_ready(&self) -> bool {
        let (Some(world), Some(state)) = (self.world.as_ref(), self.state.as_ref()) else {
            return false;
        };
        let position = state.position();
        let chunk = ChunkPos::new(
            (position.x.floor() as i32).div_euclid(16),
            (position.z.floor() as i32).div_euclid(16),
        );
        world.chunk(chunk).is_some()
    }

    /// The HUD state of the session.
    fn hud(&self) -> HudState {
        HudState {
            health: self.health,
            food: self.food,
            armor: self.armor,
            level: self.level,
            score: self.score,
            selected_slot: self.selected_slot,
            crosshair: true,
            chat: self.chat.iter().cloned().collect(),
            action_bar: social_view::action_bar_view(&self.action_bar),
            title: social_view::title_view(&self.title),
            sidebar: social_view::sidebar_view(&self.scoreboard, &self.own_name),
            tab_list: social_view::tab_list_view(&self.player_list, &self.scoreboard, true),
            show_tab_list: false,
            completions: self.completions.clone(),
            completions_serial: self.completions_serial,
            boss: social_view::boss_view(&self.boss_bar),
            vitals: self.hud_view.views().vitals.clone(),
            bar: self.hud_view.views().bar,
            held_name: self.hud_view.views().held_name.clone(),
            ..HudState::default()
        }
    }
}

/// The HUD a session would show after the server sent `events`, with the
/// title and the action bar advanced by `ticks` game ticks.
///
/// The server-less `--hud-demo` uses it: the packets go through the same
/// [`handle_net`] and the same views as in a real session, so the demo shows
/// what the player would see rather than a hand-built picture.
pub(crate) fn hud_after(events: Vec<PlayEvent>, own_name: &str, ticks: u32) -> HudState {
    let mut game = Game::new();
    game.own_name = own_name.to_string();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    for event in events {
        handle_net(&mut game, event, &mut queue, &frames);
    }
    social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
    for _ in 0..ticks {
        game.title.tick();
        game.action_bar.tick();
        social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
    }
    game.hud()
}

/// A chat message from the chat JSON of the server, in the language of the
/// downloaded `.lang` file.
fn chat_line(json: &str) -> ChatLineView {
    crate::chat::line_from_runs(
        &chat_rules::parse(json, crate::chat::lang().table()),
        Instant::now(),
    )
}

/// A line the client writes itself. It is never mistaken for JSON (a player
/// named `null` or `123`).
fn local_chat_line(text: &str) -> ChatLineView {
    crate::chat::line_from_runs(&chat_rules::local(text), Instant::now())
}

/// Shows a chat packet unless the chat visibility the player chose filters it
/// out (`GuiNewChat.drawChat` draws nothing when hidden; "commands only"
/// keeps the system messages).
fn show_chat(game: &mut Game, json: &str, position: Position) {
    game.status_data.chat_messages += 1;
    if chat_rules::shows(game.chat_mode, position) {
        let line = chat_line(json);
        push_chat(game, line);
    }
}

/// Adds a message to the chat, dropping the oldest beyond [`CHAT_HISTORY`].
fn push_chat(game: &mut Game, line: ChatLineView) {
    game.chat.push_back(line);
    while game.chat.len() > CHAT_HISTORY {
        game.chat.pop_front();
    }
    game.hud_dirty = true;
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
            ClientCommand::Player { .. }
            | ClientCommand::PlayerPosition { .. }
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
    own_name: String,
) {
    let mut game = Game::new();
    game.own_name = own_name;
    // The first event of the queue, which is still empty: the render thread
    // reads the mouse look from this slot, never from the queue.
    let _ = frames.try_send(RenderEvent::LookSlot(Arc::clone(&game.look)));
    let mut queue = CommandQueue {
        ordered: VecDeque::new(),
        input: None,
        position: None,
        sender: Some(commands),
    };
    let mut next_tick = Instant::now();
    let mut ended: Option<ConnectionEnd> = None;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        // Drain everything that is pending before looking at the clock.
        while let Ok(event) = events.try_recv() {
            if handle_event(&mut game, event, &mut queue, &frames) {
                ended = game.end.take();
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
                    ended = game.end.take();
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
/// Applies the item-carrying data watchers of dropped items and item frames.
fn apply_item_metadata(entities: &mut EntityStore, id: EntityId, metadata: &Metadata) {
    match entities.kind(id) {
        Some(EntityKind::Item) => {
            if let Some(stack) = metadata.get_item_stack(DROPPED_ITEM_WATCHER) {
                entities.set_item(id, EntityItem::from_stack(&stack));
            }
        }
        Some(EntityKind::ItemFrame) => {
            if let Some(stack) = metadata.get_item_stack(FRAME_ITEM_WATCHER) {
                entities.set_item(id, EntityItem::from_stack(&stack));
            }
            if let Some(rotation) = metadata.get_byte(FRAME_ROTATION_WATCHER) {
                entities.set_frame_rotation(id, rotation);
            }
        }
        _ => {}
    }
}

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
        GameEvent::Disconnected(end) => {
            tracing::error!(%end, "network session ended");
            game.status = format!("ERROR {end}");
            game.status_data.status = game.status.clone();
            game.end = Some(end);
            publish_status(game, frames);
            true
        }
        GameEvent::ConnectStage(stage) => {
            let _ = frames.try_send(RenderEvent::Connect(stage));
            false
        }
        GameEvent::Input(update) => {
            let before = (game.yaw, game.pitch);
            handle_input(game, update, queue);
            if (game.yaw, game.pitch) != before {
                // The view changed between two ticks: publish it right away,
                // so the mouse look does not wait for the next tick.
                publish_look(game);
            }
            false
        }
        GameEvent::Chat(message) => {
            // The server limit counts characters, not bytes.
            if !message.is_empty() && message.chars().count() <= commands::MAX_LEN {
                queue.push(ClientCommand::Chat { message });
            }
            false
        }
        GameEvent::TabComplete(message) => {
            if message.chars().count() <= commands::MAX_LEN {
                queue.push(ClientCommand::TabComplete { message });
            }
            false
        }
        GameEvent::ServerPackStatus { hash, result } => {
            queue.push(ClientCommand::ResourcePackStatus { hash, result });
            false
        }
        GameEvent::ClientSettings {
            view_distance,
            chat_mode,
            chat_colors,
            skin_parts,
        } => {
            game.chat_mode = chat_mode;
            queue.push(ClientCommand::ClientSettings {
                view_distance,
                chat_mode,
                chat_colors,
                skin_parts,
            });
            false
        }
        GameEvent::ParticleSetting(index) => {
            game.particle_setting = umicraft_game::particles::ParticleSetting::from_index(index);
            false
        }
        GameEvent::LocalChat(text) => {
            let line = local_chat_line(&text);
            push_chat(game, line);
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
        GameEvent::CloseInventory => {
            let window_id = game.windows.container().map_or(0_i8, |c| c.window_id as i8);
            game.windows.close_window(window_id as u8);
            queue.push(ClientCommand::CloseWindow { window_id });
            publish_inventory(game, frames);
            false
        }
        GameEvent::Inventory(action) => {
            apply_inventory_action(game, action, queue, frames);
            false
        }
        GameEvent::SignEdited { position, lines } => {
            queue.push(ClientCommand::UpdateSign { position, lines });
            false
        }
        GameEvent::BookEdited(packet) => {
            queue.push(ClientCommand::PluginMessage {
                channel: packet.channel,
                data: packet.data,
            });
            false
        }
        GameEvent::Container(action) => {
            apply_container_action(game, action, queue, frames);
            false
        }
        GameEvent::Creative(event) => {
            apply_creative_event(game, event, queue, frames);
            false
        }
        GameEvent::RequestStats => {
            // MCP `GuiStats` / `GuiIngameMenu` button 6 send `Client Status`
            // (REQUEST_STATS); the answer is `Statistics` (0x37).
            queue.push(ClientCommand::RequestStats);
            false
        }
        GameEvent::Respawn => {
            // MCP `GuiGameOver.actionPerformed`: `respawnPlayer` sends
            // `Client Status` (PERFORM_RESPAWN); the server answers with
            // `Respawn` and a new health.
            if game.dead() {
                queue.push(ClientCommand::Respawn);
            }
            false
        }
    }
}

/// The view of the player at the end of the current tick, and whether it is
/// the player's own (`false` while the camera follows another entity).
///
/// `None` until the first state exists.
fn player_view(game: &Game) -> Option<(RenderState, bool)> {
    let state = game.state.as_ref()?;
    let followed = game
        .camera_entity()
        .and_then(|id| game.entities.camera_view(id, 1.0));
    let mut view = followed.unwrap_or_else(|| third_person_view(game, state.interpolated(1.0)));
    if followed.is_none() && game.dead() {
        view.effects = view.effects.with_death(game.death_time as f32);
    }
    if followed.is_none() {
        view.effects = view
            .effects
            .with_hurt(hurt_roll(game.body.animation.hurt_time(), 1.0));
    }
    // MCP `EntityRenderer.renderHand`: only the first-person view of a
    // player who plays shows the hand.
    let hand_visible = followed.is_none()
        && game.perspective == Perspective::FirstPerson
        && game.gamemode != GameMode::Spectator;
    view.effects.hand = HandPose {
        visible: hand_visible,
        shows_arm: game.hand.pose().shows_arm && hand_visible,
        item_use: game.item_use.view(),
        ..game.hand.pose()
    };
    Some((view, followed.is_none()))
}

/// Publishes only the view of the player, for a look that changed between two
/// ticks.
///
/// A full [`Frame`] would restart the render thread's interpolation (its
/// `tick_time` is the arrival) and, at mouse rate, fill the frame queue until
/// tick frames are dropped: entities and the camera then stutter while moving.
fn publish_look(game: &Game) {
    if let Some((view, _)) = player_view(game) {
        let look = Look {
            view,
            body_pitch: game.pitch.to_degrees(),
        };
        if let Ok(mut slot) = game.look.lock() {
            *slot = Some(look);
        }
    }
}

/// Publishes the sound cues the tick produced, once per tick.
///
/// Cues travel as their own event, not inside the [`Frame`]: a frame is skipped
/// when the queue is full, and a footstep should not disappear with it. When
/// even this event does not fit, the render thread is far behind and the cues
/// are dropped (a sound that late is worthless); the game thread never waits.
fn publish_sounds(game: &mut Game, frames: &SyncSender<RenderEvent>) {
    let Some(state) = game.state.as_mut() else {
        return;
    };
    let cues = state.drain_sound_cues();
    if !cues.is_empty() {
        let _ = frames.try_send(RenderEvent::Sounds(cues));
    }
}

/// Publishes the current player state as a frame.
///
/// The frame queue is polled by the render thread, so the game thread never
/// blocks on it: a full queue drops the frame and the render thread keeps the
/// one it has.
fn publish_frame(game: &Game, frames: &SyncSender<RenderEvent>, tick_time: Instant) {
    let Some((view, first_person)) = player_view(game) else {
        return;
    };
    let mut entities = game.entities.frame_data();
    if let Some(vehicle) = game.vehicle {
        for frame in &mut entities {
            frame.ridden |= frame.id.0 == vehicle;
        }
    }
    if first_person && game.perspective.shows_player() {
        entities.push(game.body.frame_data());
    }
    // The line of the bobber of the local player ties to their rod, which is
    // not among the frames from the first person.
    if first_person && !game.perspective.shows_player() {
        let body = game.body.frame_data();
        let feet = view.position - DVec3::Y * umicraft_game::physics::EYE_HEIGHT;
        let tip = feet
            + first_person_tip(
                game.yaw.to_degrees(),
                game.pitch.to_degrees(),
                game.body.animation.swing_progress_at(1.0),
                body.flags.is_sneaking(),
            )
            .as_dvec3();
        for bobber in &mut entities {
            if bobber.kind == EntityKind::FishHook && bobber.object_data == game.entity_id {
                bobber.rod_tip = Some(tip);
            }
        }
    }
    // Items are lit by the block they are in; before the first chunk the
    // default (full sky) stays.
    // The local player holds leashes too, and is not among the frames from the
    // first person.
    let eye_height = umicraft_game::physics::EYE_HEIGHT as f32;
    let feet = view.position - DVec3::Y * umicraft_game::physics::EYE_HEIGHT;
    let me = LeashFrame {
        prev_position: feet,
        position: feet,
        prev_yaw: (view.yaw - std::f32::consts::PI).to_degrees(),
        yaw: (view.yaw - std::f32::consts::PI).to_degrees(),
        prev_pitch: (-view.pitch).to_degrees(),
        pitch: (-view.pitch).to_degrees(),
        eye_height,
        hanging: false,
    };
    leash_entities(&mut entities, Some((EntityId(game.entity_id), me)));
    let hand_light = game.world.as_ref().map_or(LightLevel::FULL_SKY, |world| {
        rail_entities(world, &mut entities);
        light_entities(world, &mut entities);
        light_paintings(world, &mut entities);
        shadow_entities(world, &mut entities, game.world_time);
        eye_light(world, view.position)
    });
    let _ = frames.try_send(RenderEvent::Frame(Box::new(Frame {
        state: view,
        tick_time,
        world_time: game.world_time,
        dimension: game.dimension,
        flat_world: game.flat_world,
        gamemode: game.gamemode,
        spawn: game.spawn,
        entities: entities.into(),
        local_body: Arc::new(game.body.frame_data()),
        hand_light,
        selection: game.selection_outline().into(),
        cracks: game.cracks().into(),
        maps: Arc::new(game.server.maps().clone()),
        chest_lids: Arc::new(game.server.chest_lids().clone()),
        moving_blocks: Arc::new(game.server.moving_blocks().clone()),
        block_entity_data: game.server.block_entities().entity_table(),
        signs: game.server.block_entities().sign_table(),
        sky_temperature: sky_temperature(game, view.position),
        fog: fog_context(
            game.world.as_ref(),
            game.server.effects(),
            view.position,
            game.dimension,
            game.world_time,
        ),
        border: crate::border_view::border_view(game.server.border()),
        weather: crate::weather_view::WeatherView::new(
            game.server.weather(),
            game.world.as_ref(),
            view.position.x.floor() as i32,
            view.position.z.floor() as i32,
        ),
        particles: crate::particle_view::collect(&game.particles, game.world.as_ref()).into(),
        overlays: game
            .overlays
            .inputs(game.perspective == Perspective::FirstPerson),
        debug: Arc::new(debug_world(game)),
    })));
}

/// The world facts of the F3 screen, empty before the first chunk.
fn debug_world(game: &Game) -> crate::debug_world::DebugWorld {
    let (Some(world), Some(state)) = (game.world.as_ref(), game.state.as_ref()) else {
        return crate::debug_world::DebugWorld::default();
    };
    let feet = state.position();
    crate::debug_world::collect(
        world,
        &crate::debug_world::DebugInputs {
            loaded_entities: game.entities.len(),
            particles: game.particles.len(),
            feet: [feet.x, feet.y, feet.z],
            target: game.target().map(|hit| hit.block),
            // A client that has not seen the packet assumes normal.
            difficulty: game.server.difficulty().unwrap_or(2),
            world_time: game.world_time,
        },
    )
}

/// The base temperature of the biome at `position`, which the sky takes its
/// colour from (`World.getSkyColor` reads the biome at the camera). The
/// default plains value applies without a loaded column.
fn sky_temperature(game: &Game, position: DVec3) -> f32 {
    let block = BlockPos::new(position.x.floor() as i32, 0, position.z.floor() as i32);
    game.world
        .as_ref()
        .and_then(|world| world.biome(block))
        .map_or(
            umicraft_render::RenderEnvironment::PLAINS_TEMPERATURE,
            umicraft_mesh::tint::biome_temperature,
        )
}

/// Moves the first-person view of the player to the camera of the chosen
/// perspective (MCP `EntityRenderer.orientCamera`).
fn third_person_view(game: &Game, first_person: RenderState) -> RenderState {
    let Some(world) = game.world.as_ref() else {
        return first_person;
    };
    let eye = first_person.position;
    let pose = camera_pose(game.perspective, eye, game.yaw, game.pitch, world);
    // The same angles from the eye of the previous tick: the wall rays of
    // `orientCamera` start from the interpolated eye every frame, so the
    // renderer blends this orbit with the one above over the tick.
    let start = game
        .state
        .as_ref()
        .map_or(eye, |state| state.interpolated(0.0).position);
    let start_pose = camera_pose(game.perspective, start, game.yaw, game.pitch, world);
    RenderState {
        position: pose.position,
        eye: first_person.position,
        orbit_start: (start_pose.position - start).as_vec3(),
        yaw: pose.yaw,
        pitch: pose.pitch,
        ..first_person
    }
}

/// Moves the body of the local player to where the tick left the player.
///
/// The body is animated every tick, also in first person, so that a switch
/// to the third person does not start from a standing pose.
fn follow_body(game: &mut Game) {
    let Some(state) = game.state.as_ref() else {
        return;
    };
    // The same bits the server sends for a remote player: sneaking 0x02 and
    // using the held item 0x10 (the sword blocks, the bow is aimed).
    let sneaking = if state.player().sneaking { 0x02 } else { 0 }
        | if game.item_use.is_using() { 0x10 } else { 0 };
    game.body.id = EntityId(game.entity_id);
    game.body.local = true;
    // The skin and the cape of the local player come from its own tab list
    // entry, which is found by name; until it arrives the body wears the
    // default skin.
    if game.body.profile.is_none() {
        let own = game
            .server
            .player_list()
            .iter()
            .find(|info| info.name == game.own_name);
        if let Some(info) = own {
            game.body.uuid = Some(info.uuid.to_be_bytes());
            game.body.profile = Some(std::sync::Arc::new(profile_from_properties(
                &info.name,
                &info.properties,
            )));
        }
    }
    // The local stack comes from the windows (ADR-0025), not from `Entity
    // Equipment`, so the body shows the item in the second and third person.
    game.body.held = game
        .windows
        .selected_item()
        .and_then(EntityItem::from_stack);
    game.body.armor = game
        .windows
        .worn_armor()
        .map(|piece| piece.and_then(EntityItem::from_stack));
    game.body.follow(
        state.position(),
        game.yaw.to_degrees(),
        game.pitch.to_degrees(),
        EntityFlags::new(sneaking, 0x7F),
    );
    // The swing of the third person arm advances with the tick, as the
    // animation of every tracked entity does in `EntityStore::tick`.
    game.body.animation.tick();
}

/// Potion id of Nausea.
const NAUSEA: u8 = 9;

/// Advances the overlay state of the local player by one tick.
fn tick_overlays(game: &mut Game) {
    let (Some(state), Some(world)) = (game.state.as_ref(), game.world.as_ref()) else {
        return;
    };
    let facts = crate::overlay_state::PlayerFacts {
        world,
        feet: state.position(),
        eye: state.player().eye_position(),
        spectator: game.gamemode == GameMode::Spectator,
        riding: game.vehicle.is_some(),
        nausea_ticks: game
            .server
            .effects()
            .get(NAUSEA)
            .map(|effect| u32::try_from(effect.duration).unwrap_or(0)),
        helmet: game.windows.helmet().map(|stack| stack.item_id),
        border: game.server.border(),
    };
    game.overlays.tick(&facts);
}

/// The name tag text of an entity under the team rules, if it has one.
///
/// A player's label may hold a second line, the below-name score; the two
/// lines are one tag, split by `place_tags`.
fn entity_tag(scoreboard: &Scoreboard, local: &str, entity: &EntityRecord) -> Option<String> {
    if entity.kind == EntityKind::Player {
        let name = &entity.profile.as_ref()?.name;
        return player_label(scoreboard, local, name, entity.flags.is_invisible());
    }
    mob_tag(
        entity.meta.custom_name.as_deref(),
        entity.meta.custom_name_visible,
    )
}

/// Whether the local player sees an invisible player as a ghost (MCP
/// `Entity.isInvisibleToPlayer`).
fn is_team_ghost(scoreboard: &Scoreboard, local: &str, entity: &EntityRecord) -> bool {
    entity
        .profile
        .as_ref()
        .filter(|_| entity.kind == EntityKind::Player)
        .is_some_and(|profile| sees_team_ghost(scoreboard, local, &profile.name))
}

/// Block id of the bed (MCP `Blocks.bed`).
const BED_BLOCK: u16 = 26;

/// Lays a player in the bed of a `Use Bed` packet, facing the way the bed
/// block's metadata says (MCP `EntityPlayer.trySleep`).
fn put_to_bed(game: &mut Game, packet: &UseBed) {
    let facing = game
        .world
        .as_ref()
        .map(|world| world.block(BlockPos::new(packet.x, packet.y, packet.z)))
        .filter(|state| state.id() == BED_BLOCK)
        .map_or(0, |state| state.metadata() & 3);
    let bed = DVec3::new(
        f64::from(packet.x),
        f64::from(packet.y),
        f64::from(packet.z),
    );
    game.entities.sleep(EntityId(packet.entity_id), bed, facing);
}

/// The game mode a tab list entry has for a player in spectator mode.
const SPECTATOR_WIRE: i32 = 3;

/// Handles one decoded clientbound packet.
fn handle_net(
    game: &mut Game,
    event: PlayEvent,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    // What a name tag shows depends on the teams, the tab list and the
    // metadata of the entity.
    let changes_name_tags = matches!(
        event,
        PlayEvent::Teams(_)
            | PlayEvent::ScoreboardObjective(_)
            | PlayEvent::UpdateScore(_)
            | PlayEvent::DisplayScoreboard(_)
            | PlayEvent::PlayerListItem(_)
            | PlayEvent::SpawnPlayer(_)
            | PlayEvent::SpawnMob(_)
            | PlayEvent::EntityMetadata(_)
    );
    let answers_stats = matches!(event, PlayEvent::Statistics(_));
    let offers_pack = matches!(event, PlayEvent::ResourcePackSend(_));
    apply_net_event(game, event, queue, frames);
    if offers_pack {
        publish_server_pack(game, frames);
    }
    if answers_stats {
        publish_stats(game, frames);
    }
    if changes_name_tags {
        let players = &game.player_list;
        game.entities.refresh_spectators(|uuid| {
            players
                .get(&u128::from_be_bytes(*uuid))
                .is_some_and(|entry| entry.game_mode == SPECTATOR_WIRE)
        });
        let (scoreboard, own_name) = (&game.scoreboard, game.own_name.as_str());
        game.entities
            .refresh_name_tags(|entity| entity_tag(scoreboard, own_name, entity));
        game.entities
            .refresh_ghosts(|entity| is_team_ghost(scoreboard, own_name, entity));
    }
}

fn apply_net_event(
    game: &mut Game,
    event: PlayEvent,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    game.hud_view.observe(&event, game.entity_id);
    match event {
        PlayEvent::JoinGame(info) => {
            game.world = Some(World::new(info.dimension));
            game.dirty.clear();
            game.entities.clear();
            game.entity_id = info.entity_id;
            game.hardcore = info.gamemode & 8 != 0;
            game.absorption = 0.0;
            game.server.on_join(info.entity_id);
            game.server_view = ServerView::default();
            game.vehicle = None;
            // `handleJoinGame` makes a new `PlayerControllerMP` and a new
            // player, and closes the screen: nothing of the old session's dig,
            // item use, pending click or open window survives a server switch.
            game.interaction = Interaction::new();
            game.item_use = ItemUse::new();
            game.attack_pressed = false;
            game.use_pressed = false;
            game.pending_book = None;
            game.windows = WindowManager::new();
            publish_inventory(game, frames);
            set_gamemode(game, GameMode::from_wire(info.gamemode));
            game.dimension = info.dimension;
            game.flat_world = is_flat(&info.level_type);
            // The world clock of the new session is unknown until the first
            // `Time Update`; start at the beginning of the cycle.
            game.world_time = 0;
            game.playing = true;
            game.status = "PLAYING".to_string();
            game.status_data.status = game.status.clone();
            game.player_list = PlayerList::new();
            game.scoreboard = Scoreboard::new();
            game.title = TitleState::new();
            game.action_bar = ActionBar::new();
            game.boss_bar = BossBar::new();
            game.completions.clear();
            game.hud_dirty = true;
            let _ = frames.try_send(RenderEvent::Reset);
            publish_status(game, frames);
        }
        PlayEvent::Placement(placement) => {
            let first = !game.status_data.placed;
            apply_placement(game, &placement, queue);
            if first {
                // The window leaves the connecting screen on this flag, so it
                // must not wait for the periodic status.
                publish_status(game, frames);
            }
        }
        PlayEvent::Chat(json) => show_chat(game, &json, Position::Chat),
        PlayEvent::SystemChat(json) => show_chat(game, &json, Position::System),
        PlayEvent::ActionBar(json) => {
            game.action_bar.show(&json);
            game.hud_dirty = true;
        }
        PlayEvent::PlayerListItem(packet) => {
            game.player_list.apply(&packet);
            game.server.apply_player_list(&packet);
            if let PlayerListItem::AddPlayer(entries) = &packet {
                for entry in entries {
                    let profile = profile_from_properties(&entry.name, &entry.properties);
                    // A skin plugin re-sends the own entry once the skin is
                    // applied, so the local body must follow the update.
                    if entry.name == game.own_name {
                        game.body.uuid = Some(entry.uuid.to_be_bytes());
                        game.body.profile = Some(std::sync::Arc::new(profile.clone()));
                    }
                    game.entities.set_profile(entry.uuid.to_be_bytes(), profile);
                }
            }
            game.hud_dirty = true;
        }
        PlayEvent::PlayerListHeaderFooter(packet) => {
            game.player_list.set_header_footer(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::TabComplete(packet) => {
            game.completions = packet.matches;
            game.completions_serial += 1;
            game.hud_dirty = true;
        }
        PlayEvent::ScoreboardObjective(packet) => {
            game.scoreboard.apply_objective(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::UpdateScore(packet) => {
            game.scoreboard.apply_score(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::DisplayScoreboard(packet) => {
            game.scoreboard.apply_display(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::Teams(packet) => {
            game.scoreboard.apply_teams(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::Title(packet) => {
            game.title.apply(&packet);
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
                game.windows.set_selected_slot(slot as u8);
                game.hud_dirty = true;
            }
        }
        PlayEvent::Respawn(packet) => {
            // MCP `handleRespawn` sets the gamemode of the new player.
            set_gamemode(game, GameMode::from_wire(packet.game_mode));
            // A respawned player stands on foot and has told the server
            // nothing yet (MCP `handleRespawn` creates a new player entity,
            // whose sprint and sneak states start off).
            game.vehicle = None;
            game.overlays.reset();
            game.server_view = ServerView::default();
            // The server keeps the chunks of the dimension, so only a change
            // of the dimension replaces the world.
            let dimension = packet.dimension as i8;
            let changed = game
                .world
                .as_ref()
                .is_none_or(|world| world.dimension() != dimension);
            game.server.on_respawn(changed);
            if changed {
                // `handleRespawn` builds the new world with the type of the
                // packet; a same-dimension respawn keeps the old one.
                game.flat_world = is_flat(&packet.world_type);
                game.world = Some(World::new(dimension));
                game.dirty.clear();
                game.entities.clear();
                game.particles.clear();
                // A new world has its own scoreboard; the tab list belongs to
                // the connection and stays.
                game.scoreboard = Scoreboard::new();
                game.hud_dirty = true;
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
                // The local player's items come from the inventory windows
                // (ADR-0025), never from `Entity Equipment`.
            } else if packet.slot == 0 {
                game.entities.set_held(
                    EntityId(packet.entity_id),
                    EntityItem::from_stack(&packet.item),
                );
            } else {
                game.entities.set_armor(
                    EntityId(packet.entity_id),
                    packet.slot,
                    EntityItem::from_stack(&packet.item),
                );
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
            // `Spawn Player` carries an item id only: no damage, count or NBT.
            let held = (p.held_item > 0).then_some(EntityItem {
                item_id: p.held_item,
                damage: 0,
                count: 1,
                nbt: None,
            });
            let id = EntityId(p.entity_id);
            game.entities
                .spawn_player(id, p.uuid, pos, yaw, pitch, held, EntityFlags::default());
            game.entities.apply_metadata(id, &p.metadata);
            let listed = game
                .server
                .player_list()
                .get(u128::from_be_bytes(p.uuid))
                .map(|info| profile_from_properties(&info.name, &info.properties));
            if let Some(profile) = listed {
                game.entities.set_profile(p.uuid, profile);
            }
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
            let id = EntityId(m.entity_id);
            game.entities.spawn_mob(
                id,
                m.mob_type,
                pos,
                yaw,
                pitch,
                head_yaw,
                vel,
                EntityFlags::default(),
            );
            game.entities.apply_metadata(id, &m.metadata);
            if let Some(health) = m.metadata.get_float(6) {
                game.entities.set_health(id, health);
            }
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
            game.entities.spawn_object(
                EntityId(o.entity_id),
                o.object_type,
                pos,
                pitch,
                yaw,
                vel,
                o.data,
            );
        }
        // MCP `handleCollectItem`: the collected entity flies to the collector
        // in three ticks and leaves the world. The pop is not played.
        PlayEvent::CollectItem(c) => {
            let local = game
                .state
                .as_ref()
                .map_or(DVec3::ZERO, |state| state.position());
            game.entities.collect(
                EntityId(c.collected_entity_id),
                EntityId(c.collector_entity_id),
                local,
            );
        }
        // MCP `handleSpawnGlobalEntity`: type 1 is a lightning bolt, the only
        // global entity of 1.8.
        PlayEvent::SpawnGlobalEntity(g) if g.entity_type == 1 => {
            let pos = DVec3::new(
                f64::from(g.x) / 32.0,
                f64::from(g.y) / 32.0,
                f64::from(g.z) / 32.0,
            );
            game.entities.spawn_lightning(EntityId(g.entity_id), pos);
        }
        PlayEvent::SpawnXpOrb(o) => {
            let pos = DVec3::new(
                f64::from(o.x) / 32.0,
                f64::from(o.y) / 32.0,
                f64::from(o.z) / 32.0,
            );
            game.entities
                .spawn_xp_orb(EntityId(o.entity_id), pos, o.count);
        }
        PlayEvent::SpawnPainting(p) => {
            game.entities.spawn_painting(
                EntityId(p.entity_id),
                &p.title,
                [p.x, p.y, p.z],
                p.facing,
            );
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
        PlayEvent::EntityVelocity(v) if v.entity_id == game.entity_id => {
            // MCP `handleEntityVelocity` sets the motion of the local player:
            // the knockback of a hit, a fishing rod, a piston.
            if let Some(state) = game.state.as_mut() {
                state.set_velocity(DVec3::new(
                    f64::from(v.velocity_x) / VELOCITY_UNITS,
                    f64::from(v.velocity_y) / VELOCITY_UNITS,
                    f64::from(v.velocity_z) / VELOCITY_UNITS,
                ));
            }
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
            if m.entity_id == game.entity_id {
                // MCP EntityPlayer.getScore: data watcher 18.
                if let Some(score) = m.metadata.get_int(SCORE_WATCHER) {
                    game.score = score;
                    game.hud_dirty = true;
                }
                // MCP `EntityPlayer.entityInit`: the absorption hearts.
                if let Some(absorption) = m.metadata.get_float(ABSORPTION_WATCHER) {
                    game.absorption = absorption;
                    game.hud_dirty = true;
                }
                game.overlays.apply_metadata(&m.metadata);
            }
            let id = EntityId(m.entity_id);
            game.entities.apply_metadata(id, &m.metadata);
            apply_item_metadata(&mut game.entities, id, &m.metadata);
            if let Some(health) = m.metadata.get_float(6) {
                game.entities.set_health(id, health);
            }
        }
        PlayEvent::AttachEntity(packet) => {
            // MCP `NetHandlerPlayClient.handleEntityAttach`: leash 0 mounts
            // (vehicle -1 dismounts); only the local player's riding changes
            // the serverbound traffic. Passengers of other entities are P5.
            if packet.leash == 0 && packet.entity_id == game.entity_id {
                game.vehicle = (packet.vehicle_id != -1).then_some(packet.vehicle_id);
            }
            // Leash 1 ties the entity to the holder (-1 frees it).
            if packet.leash == 1 {
                let holder = (packet.vehicle_id != -1).then_some(EntityId(packet.vehicle_id));
                game.entities.set_leash(EntityId(packet.entity_id), holder);
            }
            if packet.leash == 0 {
                let vehicle = (packet.vehicle_id != -1).then_some(EntityId(packet.vehicle_id));
                game.entities.attach(EntityId(packet.entity_id), vehicle);
            }
        }
        PlayEvent::EntityStatus(status) => {
            // MCP `EntityPlayer.handleStatusUpdate`: 9 finishes the use of
            // the item; 2 and 3 hurt and kill an entity.
            if status.status == ITEM_USE_FINISHED && status.entity_id == game.entity_id {
                game.item_use.finish();
            }
            // `handleStatusUpdate(2)` of the local player: the hurt shake of the
            // camera and the resistance that flashes the hearts.
            if status.entity_id == game.entity_id && status.status == 2 {
                game.body.animation.hurt_by_status();
            }
            // Only the local player hears its own status: other entities'
            // sounds arrive as Sound Effect packets (`sound::entity_sounds`).
            if status.entity_id == game.entity_id {
                if let Some(state) = game.state.as_mut() {
                    state.on_local_player_status(status.status);
                }
            }
            game.entities
                .status(EntityId(status.entity_id), status.status);
        }
        PlayEvent::Animation(packet) => {
            // `performHurtAnimation` of the local player.
            if packet.entity_id == game.entity_id && packet.animation == 1 {
                game.body.animation.hurt();
            }
            // `handleAnimation` 4 and 5: the stars of a critical hit on that
            // entity.
            if let Some(star) = HitStar::from_animation(packet.animation) {
                combat::emit_stars(game, EntityId(packet.entity_id), star);
            }
            game.entities
                .play_animation(EntityId(packet.entity_id), packet.animation);
        }
        PlayEvent::UseBed(bed) => put_to_bed(game, &bed),
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
        PlayEvent::OpenWindow(packet) => {
            if let Some(kind) = WindowKind::from_type_string(&packet.window_type) {
                game.windows
                    .open_window(packet.window_id, kind, packet.title, packet.slot_count);
                publish_inventory(game, frames);
            } else {
                tracing::warn!(
                    window_type = %packet.window_type,
                    "unknown container type, ignoring"
                );
            }
        }
        PlayEvent::CloseWindow { window_id } => {
            game.windows.close_window(window_id);
            publish_inventory(game, frames);
        }
        PlayEvent::SetSlot(packet) => {
            game.windows
                .apply_set_slot(packet.window_id, packet.slot, packet.item);
            sync_armor(game);
            publish_inventory(game, frames);
        }
        PlayEvent::WindowItems(packet) => {
            game.windows
                .apply_window_items(packet.window_id, &packet.items);
            sync_armor(game);
            publish_inventory(game, frames);
        }
        PlayEvent::WindowProperty(packet) => {
            game.windows
                .apply_property(packet.window_id, packet.property, packet.value);
            publish_inventory(game, frames);
        }
        PlayEvent::ConfirmTransaction(packet) => {
            answer_transaction(game, &packet, queue);
            // The prediction is confirmed or rolled back.
            game.windows.confirm_transaction(
                packet.window_id,
                packet.action_number,
                packet.accepted,
            );
            sync_armor(game);
            publish_inventory(game, frames);
        }
        PlayEvent::Explosion(packet) => apply_explosion(game, &packet),
        PlayEvent::Particle(packet) => spawn_particles(game, &packet),
        PlayEvent::Effect(packet) => {
            if let Some(state) = game.state.as_mut() {
                state.on_effect(&packet);
            }
            apply_effect(game, &packet);
        }
        PlayEvent::SoundEffect(packet) => {
            if let Some(state) = game.state.as_mut() {
                state.on_sound_effect(&packet);
            }
        }
        PlayEvent::PlayerAbilities(packet) => {
            game.server.apply_abilities(&packet);
            apply_mode_rules(game);
        }
        PlayEvent::ChangeGameState(packet) => {
            game.server.apply_game_state(&packet);
            if packet.reason == GAMEMODE_REASON {
                set_gamemode(game, GameMode::from_state_value(packet.value));
            }
        }
        PlayEvent::BlockAction(packet) => {
            if let Some(world) = game.world.as_mut() {
                let dirty = motion::start(world, game.server.moving_blocks_mut(), &packet);
                game.dirty.extend(dirty);
            }
            game.server.apply_block_action(&packet);
        }
        PlayEvent::PluginMessage(packet) if packet.channel == payload::TRADE_LIST => {
            apply_trade_list(game, &packet.data, frames);
        }
        other => apply_server_state(&mut game.server, &other),
    }
}

/// Switches the gamemode of the local player.
///
/// The server announces the abilities of the new mode in a separate
/// `Player Abilities` packet; the no-clip flag only depends on the mode.
fn set_gamemode(game: &mut Game, mode: GameMode) {
    game.gamemode = mode;
    game.status_data.gamemode = mode.id();
    game.hud_dirty = true;
    apply_mode_rules(game);
}

/// Hands the abilities of the server and the rules of the gamemode to the
/// player simulation.
///
/// MCP `PlayerCapabilities` is what the server last sent; a spectator
/// always may fly and never collides (`EntityPlayerSP.onLivingUpdate`).
fn apply_mode_rules(game: &mut Game) {
    let Some(state) = game.state.as_mut() else {
        return;
    };
    let abilities = game.server.abilities();
    let allow_flying = abilities.is_some_and(|a| a.allow_flying()) || game.gamemode.allows_flight();
    let flying = abilities.is_some_and(|a| a.flying());
    let fly_speed = abilities.map_or(DEFAULT_FLY_SPEED, |a| a.fly_speed);
    state.set_abilities(allow_flying, flying, fly_speed);
    state.set_no_clip(game.gamemode.no_clip());
}

/// The Haste and Mining Fatigue effects that change the arm swing speed.
fn swing_effects(game: &Game) -> SwingEffects {
    let effects = game.server.effects();
    let amplifier = |id: u8| effects.get(id).map(|effect| effect.amplifier);
    SwingEffects {
        haste: amplifier(HASTE),
        fatigue: amplifier(MINING_FATIGUE),
    }
}

/// Advances the first-person hand by one tick (arm swing, equip animation
/// and sway).
fn tick_hand(game: &mut Game) {
    let effects = swing_effects(game);
    let held = game.windows.selected_item();
    game.hand.tick(
        held,
        game.yaw.to_degrees(),
        game.pitch.to_degrees(),
        effects,
    );
}

/// Hands the speed attribute, the effects and the food level of the server
/// state to the simulation (`umicraft_game::movement`).
fn apply_movement_stats(game: &mut Game) {
    let Some(state) = game.state.as_mut() else {
        return;
    };
    let walk_speed = game
        .server
        .abilities()
        .map_or(DEFAULT_WALK_SPEED, |abilities| abilities.walk_speed);
    let stats = movement_stats(
        game.server.attributes(),
        game.server.effects(),
        f64::from(walk_speed),
    );
    state.set_movement(stats, walk_speed, game.food > SPRINT_FOOD_LIMIT);
    let strider = game
        .windows
        .boots()
        .and_then(|boots| boots.nbt.as_deref())
        .map_or(0, |nbt| enchantment_level(nbt, DEPTH_STRIDER));
    state.set_depth_strider(u8::try_from(strider).unwrap_or(u8::MAX));
    state.set_bow_draw(
        game.item_use
            .view()
            .filter(|using| using.action == UseAction::Bow)
            .map(|using| using.ticks_used),
    );
}

/// Tells the server about a change of the flying state.
///
/// MCP `EntityPlayerSP.sendPlayerAbilities` (`C13PacketPlayerAbilities`)
/// repeats the capabilities with the new `isFlying`.
fn report_flight(game: &Game, was_flying: bool, queue: &mut CommandQueue) {
    let (Some(state), Some(abilities)) = (game.state.as_ref(), game.server.abilities()) else {
        return;
    };
    let flying = state.player().flying;
    if flying != was_flying {
        queue.push(ClientCommand::PlayerAbilities {
            flags: abilities.flags,
            flying,
            fly_speed: abilities.fly_speed,
            walk_speed: abilities.walk_speed,
        });
    }
}

/// Removes the blocks of an explosion from the world.
///
/// The particles the world makes around the player this tick.
fn spawn_ambient_particles(game: &mut Game) {
    let (Some(state), Some(world)) = (game.state.as_ref(), game.world.as_ref()) else {
        return;
    };
    let player = state.player();
    let viewer = crate::particle_ambient::Viewer {
        position: player.position,
        motion: player.velocity,
        running: player.sprinting && !player.in_water && player.on_ground,
        sees_barriers: game.gamemode == GameMode::Creative
            && game
                .windows
                .selected_item()
                .is_some_and(|stack| stack.item_id == BARRIER_ITEM),
    };
    let rain = game.ambient.tick(
        &mut game.particles,
        world,
        &viewer,
        game.server.weather().rain_strength(),
        i32::try_from(game.status_data.ticks).unwrap_or(0),
        game.particle_setting,
    );
    if let Some(state) = game.state.as_mut() {
        state.queue_sound_cues(rain);
    }
}

/// Creates the particles a `Particle` packet asks for, seen from the player
/// (MCP `NetHandlerPlayClient.handleParticles`). Before the player is placed
/// there is no viewer, so the packet is dropped.
fn spawn_particles(
    game: &mut Game,
    packet: &umicraft_protocol::packets::play::clientbound::Particle,
) {
    if let Some(filter) = particle_filter(game) {
        game.particles.spawn_from_packet(packet, &filter);
    }
}

/// The spawn filter for the local viewer; `None` before the player is placed.
fn particle_filter(game: &Game) -> Option<SpawnFilter> {
    Some(SpawnFilter {
        viewer: game.state.as_ref()?.position(),
        setting: game.particle_setting,
    })
}

/// Whether the world type the server names is `flat` (`WorldType.FLAT`;
/// `WorldType.parseWorldType` compares the names without regard to case).
fn is_flat(world_type: &str) -> bool {
    world_type.eq_ignore_ascii_case("flat")
}

/// `Effect` 2001 (`RenderGlobal.playAuxSFX`): a block broke somewhere, so its
/// fragments fly. The data is the broken state, `id | metadata << 12`.
fn apply_effect(game: &mut Game, packet: &umicraft_protocol::packets::play::clientbound::Effect) {
    if packet.effect_id != BLOCK_BREAK_EFFECT {
        spawn_effect_particles(game, packet);
        return;
    }
    let (Ok(id), Ok(metadata)) = (
        u16::try_from(packet.data & 0xFFF),
        u8::try_from((packet.data >> 12) & 0xFF),
    ) else {
        return;
    };
    game.particles.add_block_destroy_effects(
        BlockPos::new(packet.x, packet.y, packet.z),
        BlockState::new(id, metadata),
        [1.0; 3],
    );
}

/// The particles of the other `Effect` ids (smoke, potion splash, ender eye,
/// spawner, bone meal); the rest are sounds.
fn spawn_effect_particles(
    game: &mut Game,
    packet: &umicraft_protocol::packets::play::clientbound::Effect,
) {
    let Some(filter) = particle_filter(game) else {
        return;
    };
    let pos = BlockPos::new(packet.x, packet.y, packet.z);
    let block_max_y = game.world.as_ref().and_then(|world| {
        let state = world.block(pos);
        (state.id() != 0).then(|| {
            umicraft_game::shapes::selection_shape(state, world, pos)
                .boxes()
                .iter()
                .map(|bounds| bounds.max.y)
                .fold(0.0, f64::max)
        })
    });
    let context = EffectContext::new(packet.data, block_max_y);
    game.particles
        .spawn_effect(packet.effect_id, pos, packet.data, &context, &filter);
}

/// Chips the face being dug while the dig goes on (MCP
/// `PlayerControllerMP.onPlayerDamageBlock` calls `addBlockHitEffects` on every
/// tick of a dig).
fn chip_dug_block(game: &mut Game, hit: Option<&RayHit>) {
    let (Some(hit), Some(world)) = (hit, game.world.as_ref()) else {
        return;
    };
    if game.interaction.digging_block() != Some(hit.block) {
        return;
    }
    let state = world.block(hit.block);
    let shape = umicraft_game::shapes::selection_shape(state, world, hit.block);
    let bounds = shape
        .boxes()
        .iter()
        .copied()
        .reduce(|a, b| Aabb::new(a.min.min(b.min), a.max.max(b.max)))
        .unwrap_or_else(|| Aabb::new(DVec3::ZERO, DVec3::ONE));
    game.particles
        .add_block_hit_effects(hit.block, state, bounds, hit.face, [1.0; 3]);
}

/// MCP `NetHandlerPlayClient.handleExplosion` runs `Explosion.doExplosionB`
/// on the client, which sets every affected block to air before the server's
/// block changes arrive. The player's knockback from the packet is P2.
fn apply_explosion(game: &mut Game, packet: &Explosion) {
    // MCP `handleExplosion`: the push is added to the motion of the player.
    if let Some(state) = game.state.as_mut() {
        state.add_velocity(DVec3::new(
            f64::from(packet.player_motion_x),
            f64::from(packet.player_motion_y),
            f64::from(packet.player_motion_z),
        ));
    }
    let Some(world) = game.world.as_mut() else {
        return;
    };
    let affected: Vec<(i32, i32, i32)> = packet.affected_blocks().collect();
    for &(x, y, z) in &affected {
        game.dirty
            .extend(world.apply_block_change(BlockPos::new(x, y, z), BlockState::AIR));
    }
    if let Some(filter) = particle_filter(game) {
        game.particles.explode(
            DVec3::new(packet.x.into(), packet.y.into(), packet.z.into()),
            packet.strength,
            &affected,
            &filter,
        );
    }
}

/// Keeps the state of the packets whose consumers arrive in later
/// milestones (ADR-0020 §3); purely visual or audible events are dropped
/// here until their milestone (P5 entities, P8 particles, P11 audio).
fn apply_server_state(server: &mut ServerState, event: &PlayEvent) {
    match event {
        PlayEvent::EntityEffect(packet) => server.apply_entity_effect(packet),
        PlayEvent::RemoveEntityEffect(packet) => server.apply_remove_entity_effect(packet),
        PlayEvent::EntityProperties(packet) => server.apply_entity_properties(packet),
        PlayEvent::ServerDifficulty(packet) => server.apply_difficulty(packet),
        PlayEvent::WorldBorder(packet) => server.apply_world_border(packet),
        PlayEvent::PlayerListItem(packet) => server.apply_player_list(packet),
        PlayEvent::PlayerListHeaderFooter(packet) => server.apply_header_footer(packet),
        PlayEvent::ScoreboardObjective(packet) => server.apply_objective(packet),
        PlayEvent::UpdateScore(packet) => server.apply_score(packet),
        PlayEvent::DisplayScoreboard(packet) => server.apply_display(packet),
        PlayEvent::Teams(packet) => server.apply_team(packet),
        PlayEvent::Title(packet) => server.apply_title(packet),
        PlayEvent::UpdateSign(packet) => server.apply_sign(packet),
        PlayEvent::BlockAction(packet) => server.apply_block_action(packet),
        PlayEvent::UpdateBlockEntity(packet) => server.apply_block_entity(packet),
        PlayEvent::BlockBreakAnim(packet) => server.apply_block_break_anim(packet),
        PlayEvent::Maps(packet) => server.apply_map(packet),
        PlayEvent::Statistics(packet) => server.apply_statistics(packet),
        PlayEvent::Camera(packet) => server.apply_camera(packet),
        PlayEvent::PluginMessage(packet) => server.apply_plugin_message(packet),
        PlayEvent::CombatEvent(packet) => server.apply_combat_event(packet),
        PlayEvent::SignEditorOpen(packet) => server.apply_sign_editor_open(packet),
        PlayEvent::ResourcePackSend(packet) => server.apply_resource_pack_offer(packet),
        // Use Bed, Collect Item, the painting/orb/lightning spawns, Effect,
        // Sound Effect, Particle, Block Action, Tab-Complete and Update
        // Entity NBT are events, not state: their consumers render or play
        // them when they arrive.
        _ => {}
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

/// Recomputes the armour bar from the armour slots of the inventory.
fn sync_armor(game: &mut Game) {
    game.armor = f32::from(game.windows.armor_value());
    game.hud_dirty = true;
}

/// Counts a block change the server confirmed.
fn acknowledge_block(game: &mut Game, position: BlockPos, block_state: u16) {
    tracing::debug!(?position, block_state, "block change applied");
    match game.interaction.acknowledge_block(position) {
        Some(BlockAck::Break) => {
            game.status_data.breaks_acknowledged += 1;
            tracing::info!(
                ?position,
                block_state,
                "the server confirmed the block break"
            );
        }
        Some(BlockAck::Placement) => {
            game.status_data.placements_acknowledged += 1;
            tracing::info!(
                ?position,
                block_state,
                "the server confirmed the block placement"
            );
        }
        None => {}
    }
}

/// Answers a `Confirm Transaction` the server did not accept.
///
/// Vanilla answers on the game thread, in the order of the packets
/// (`NetHandlerPlayClient.handleConfirmTransaction`). GrimAC sends a
/// transaction before and after a teleport and needs the replies in that
/// order: the position reply must leave between the two. The session thread
/// used to answer at once, so the second transaction overtook the position
/// reply and Grim flagged `BadPacketsN` (observed 2026-11-02).
fn answer_transaction(
    game: &Game,
    packet: &umicraft_protocol::packets::play::clientbound::ConfirmTransaction,
    queue: &mut CommandQueue,
) {
    if !packet.accepted && game.windows.answers_transaction(packet.window_id) {
        queue.push(ClientCommand::ConfirmTransaction {
            window_id: packet.window_id as i8,
            action_number: packet.action_number,
        });
    }
}

/// Applies a server placement.
fn apply_placement(
    game: &mut Game,
    placement: &umicraft_net::play::PlayerPlacement,
    queue: &mut CommandQueue,
) {
    // Relative flags apply to where the player is now, not to the last
    // placement the session saw.
    let resolved = game.state.as_ref().map(|state| {
        let position = state.position();
        placement.resolved_against(
            [position.x, position.y, position.z],
            state.player().yaw.to_degrees(),
            state.player().pitch.to_degrees(),
        )
    });
    let placement = resolved.as_ref().unwrap_or(placement);
    let yaw = placement.yaw.to_radians();
    let pitch = placement.pitch.to_radians();
    let position = DVec3::new(placement.x, placement.y, placement.z);

    match game.state.as_mut() {
        None => {
            game.state = Some(GameState::new(position));
            game.status_data.placed = true;
            game.yaw = yaw;
            game.pitch = pitch;
            apply_mode_rules(game);
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
            // A `Player Position and Look` is a teleport, not a correction to
            // reconcile: nothing is replayed (see `GameState::teleport`).
            let flags = placement.raw.as_deref().map_or(0, |raw| raw.flags);
            state.teleport(&correction, [0x01, 0x02, 0x04].map(|bit| flags & bit != 0));
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
    let ack = PositionAck {
        x: placement.x,
        y: placement.y,
        z: placement.z,
        yaw: placement.yaw,
        pitch: placement.pitch,
        on_ground: false,
    };
    // Vanilla answers every `Player Position and Look`
    // (`NetHandlerPlayClient.handlePlayerPosLook`). Anti-cheats such as
    // GrimAC require it: a teleport that is not answered is flagged
    // (`BadPacketsN`), so a repeated position is acknowledged again.
    queue.push(command_of(Outgoing::AckPosition(ack)));
    game.server_view.forget_position();
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
    game.server_view.forget_position();
}

/// Handles one input update.
fn handle_input(game: &mut Game, update: InputUpdate, queue: &mut CommandQueue) {
    game.input = update.state;
    game.yaw = (game.yaw + update.look_yaw).rem_euclid(std::f32::consts::TAU);
    game.pitch = (game.pitch + update.look_pitch).clamp(-MAX_PITCH, MAX_PITCH);

    if let Some(slot) = update.select_slot {
        select_slot(game, slot, queue);
    }

    if update.toggle_perspective {
        game.perspective = game.perspective.next();
    }
    game.attack_pressed |= update.break_block;
    game.use_pressed |= update.place_block;
    if update.pick_block {
        pick_block(game, queue);
    }
    if let Some(outgoing) = update
        .drop_item
        .and_then(|kind| drop_item(game.gamemode, kind))
    {
        queue.push(command_of(outgoing));
    }
}

/// Runs one tick of the attack button (`umicraft_game::interaction`).
///
/// The client removes the blocks it breaks at once, like MCP
/// `PlayerControllerMP.onPlayerDestroyBlock`; the server confirms or
/// restores them with a block change.
fn run_attack(game: &mut Game, queue: &mut CommandQueue) {
    let pressed = std::mem::take(&mut game.attack_pressed);
    // MCP `Minecraft.runTick` lines 2098-2121: the attack button does
    // nothing while an item is in use.
    if game.spectating() || game.dead() || game.item_use.is_using() {
        if pressed {
            tracing::debug!(
                spectating = game.spectating(),
                dead = game.dead(),
                using_item = game.item_use.is_using(),
                "attack click ignored"
            );
        }
        return;
    }
    let pointed = combat::pointed(game);
    if pressed {
        combat::log_click(game, &pointed);
    }
    let target = pointed.block();
    let entity = pointed.entity().map(|hit| hit.id);
    let (Some(state), Some(world)) = (game.state.as_ref(), game.world.as_ref()) else {
        return;
    };
    let player = state.player();
    let eye = player.eye_position();
    let water = world.block(BlockPos::new(
        eye.x.floor() as i32,
        eye.y.floor() as i32,
        eye.z.floor() as i32,
    ));
    let effects = game.server.effects();
    let amplifier = |id: u8| effects.get(id).map(|effect| effect.amplifier);
    let digger = Digger {
        held: game.windows.selected_item(),
        haste: amplifier(HASTE),
        fatigue: amplifier(MINING_FATIGUE),
        in_water: matches!(water.id(), WATER_FLOWING | WATER_STILL),
        aqua_affinity: game
            .windows
            .helmet()
            .and_then(|helmet| helmet.nbt.as_deref())
            .is_some_and(|nbt| enchantment_level(nbt, AQUA_AFFINITY) > 0),
        on_ground: player.on_ground,
    };
    let output = game.interaction.attack(&AttackTick {
        mode: game.gamemode,
        pressed,
        held: game.input.attack,
        target,
        entity,
        world,
        digger,
    });
    let effects = swing_effects(game);
    if let Some(state) = game.state.as_mut() {
        state.queue_sound_cues(output.sounds);
    }
    for outgoing in output.packets {
        match outgoing {
            Outgoing::Swing => {
                game.hand.swing(effects);
                game.body.animation.swing_item();
            }
            Outgoing::Attack { entity } => combat::after_attack(game, EntityId(entity)),
            _ => {}
        }
        queue.push(command_of(outgoing));
    }
    if let Some(world) = game.world.as_mut() {
        for position in output.broken {
            tracing::info!(block = ?position, "breaking a block");
            let broken = world.block(position);
            game.dirty
                .extend(world.apply_block_change(position, BlockState::AIR));
            game.particles
                .add_block_destroy_effects(position, broken, [1.0; 3]);
        }
    }
    chip_dug_block(game, target.as_ref());
    if let Some(progress) = output.progress {
        game.server.apply_block_break_anim(&BlockBreakAnim {
            entity_id: game.entity_id,
            x: progress.block.x,
            y: progress.block.y,
            z: progress.block.z,
            stage: u8::try_from(progress.stage).unwrap_or(CLEAR_BREAK_STAGE),
        });
    }
}

/// Runs one tick of the use button (`umicraft_game::item_use`): the click on
/// a block or in the air, the held button and the release.
fn run_use(game: &mut Game, queue: &mut CommandQueue) {
    let pressed = std::mem::take(&mut game.use_pressed);
    if game.dead() {
        return;
    }
    let pointed = combat::pointed(game);
    let target = pointed.block();
    let entity = pointed
        .entity()
        .and_then(|hit| Some((game.entities.entities().get(&hit.id)?, hit)));
    let (Some(state), Some(world)) = (game.state.as_ref(), game.world.as_ref()) else {
        return;
    };
    let item = game.windows.selected_item().cloned();
    let has_arrow = game
        .windows
        .main_inventory()
        .iter()
        .flatten()
        .any(|stack| stack.item_id == ARROW);
    let player = state.player();
    let output = game.item_use.tick(
        &mut game.interaction,
        &UseTick {
            mode: game.gamemode,
            pressed,
            held: game.input.use_item,
            target,
            entity,
            world,
            player: Some(player.bounding_box()),
            sneaking: player.sneaking,
            slot: game.selected_slot,
            item: item.as_ref(),
            food: game.food as i32,
            has_arrow,
            viewer: ClickViewer {
                id: EntityId(game.entity_id),
                uuid: game.body.uuid.map(u128::from_be_bytes),
            },
        },
    );
    for outgoing in output.packets {
        if let Outgoing::UseItem { item } = &outgoing {
            // MCP `ItemWritableBook` / `ItemEditableBook.onItemRightClick`
            // open the book screen when the item is used.
            if matches!(item.item_id, WRITABLE_BOOK | WRITTEN_BOOK) {
                game.pending_book = Some(item.clone());
            }
        }
        queue.push(command_of(outgoing));
    }
    if let Some(door) = output.door {
        swing_door(game, door);
    }
    if output.swing {
        // MCP `Minecraft.rightClickMouse`: a used item swings the arm.
        queue.push(ClientCommand::Swing);
        let effects = swing_effects(game);
        game.hand.swing(effects);
        game.body.animation.swing_item();
    }
}

/// MCP `BlockDoor.onBlockActivated` runs on the client too: the door swings at
/// once and the player hears it. The server sends that sound to everyone but
/// the player who clicked (`playAuxSFXAtEntity`), so the clicking client has
/// to make it: it takes the same path as the `Effect` packet the others get.
fn swing_door(game: &mut Game, door: DoorToggle) {
    if let Some(world) = game.world.as_mut() {
        game.dirty
            .extend(world.apply_block_change(door.lower, door.state));
    }
    let Some(state) = game.state.as_mut() else {
        return;
    };
    // 1003 opens, 1006 closes (`RenderGlobal.playAuxSFX`).
    state.on_effect(&umicraft_protocol::packets::play::clientbound::Effect {
        effect_id: if door.opens() { 1003 } else { 1006 },
        x: door.clicked.x,
        y: door.clicked.y,
        z: door.clicked.z,
        data: 0,
        disable_relative_volume: false,
    });
}

/// Picks the targeted block into the hotbar (the middle button).
///
/// MCP `Minecraft.middleClickMouse`: the hotbar slot that holds the item is
/// selected; a creative player gets the item in the first free slot and
/// tells the server about the slot.
fn pick_block(game: &mut Game, queue: &mut CommandQueue) {
    if game.dead() {
        return;
    }
    let (Some(hit), Some(world)) = (game.target(), game.world.as_ref()) else {
        return;
    };
    let Some(picked) = picked_item(world.block(hit.block)) else {
        return;
    };
    let creative = game.gamemode == GameMode::Creative;
    let pick = pick_in(
        &game.windows.main_inventory(),
        game.selected_slot,
        picked,
        creative,
    );
    for (index, stack) in pick.changed {
        game.windows
            .apply_set_slot(0, window_slot(index), stack.clone());
        if creative && index == usize::from(pick.select.unwrap_or(game.selected_slot)) {
            queue.push(ClientCommand::CreativeSlot {
                slot: window_slot(index),
                item: stack,
            });
        }
    }
    if let Some(slot) = pick.select {
        select_slot(game, slot, queue);
    }
}

/// The slot of the player window that holds a main inventory index
/// (the hotbar is the last row of the window).
fn window_slot(index: usize) -> i16 {
    let slot = if index < 9 { 36 + index } else { index };
    slot as i16
}

/// Selects a hotbar slot and tells the server
/// (MCP `PlayerControllerMP.syncCurrentPlayItem`).
fn select_slot(game: &mut Game, slot: u8, queue: &mut CommandQueue) {
    if slot < 9 && slot != game.selected_slot {
        game.selected_slot = slot;
        game.windows.set_selected_slot(slot);
        game.hud_dirty = true;
        queue.push(ClientCommand::HeldItemChange {
            slot: i16::from(slot),
        });
    }
}

/// The network command of a packet the interaction rules produced.
fn command_of(outgoing: Outgoing) -> ClientCommand {
    match outgoing {
        Outgoing::Dig {
            status,
            block,
            face,
        } => ClientCommand::PlayerDigging {
            status,
            x: block.x,
            y: block.y,
            z: block.z,
            face,
        },
        Outgoing::Place {
            block,
            face,
            item,
            cursor: [cursor_x, cursor_y, cursor_z],
        } => ClientCommand::PlayerBlockPlacement {
            x: block.x,
            y: block.y,
            z: block.z,
            face,
            item,
            cursor_x,
            cursor_y,
            cursor_z,
        },
        Outgoing::UseItem { item } => ClientCommand::UseItem { item },
        Outgoing::Swing => ClientCommand::Swing,
        Outgoing::Attack { entity } => use_entity(entity, UseEntityAction::Attack),
        Outgoing::Interact { entity } => use_entity(entity, UseEntityAction::Interact),
        Outgoing::InteractAt {
            entity,
            offset: [x, y, z],
        } => use_entity(entity, UseEntityAction::InteractAt { x, y, z }),
        Outgoing::AckPosition(ack) => ClientCommand::AckPosition {
            x: ack.x,
            y: ack.y,
            z: ack.z,
            yaw: ack.yaw,
            pitch: ack.pitch,
            on_ground: ack.on_ground,
        },
    }
}

/// The `Use Entity` command of an entity click.
fn use_entity(target: i32, action: UseEntityAction) -> ClientCommand {
    ClientCommand::UseEntity { target, action }
}

/// Hands the resource pack offer the server just made to the window, which
/// asks the player or downloads it (NET-11).
fn publish_server_pack(game: &Game, frames: &SyncSender<RenderEvent>) {
    if let Some(offer) = game.server.resource_pack_offer() {
        let request = ServerPackRequest::from(offer);
        let _ = frames.try_send(RenderEvent::ServerPack(Box::new(request)));
    }
}

/// Asks the window to open the sign editor the server requested and the book
/// the player used.
fn publish_text_screens(game: &mut Game, frames: &SyncSender<RenderEvent>) {
    let mut requests = Vec::new();
    if let Some(position) = game.server.take_sign_editor() {
        let lang = LangTable::default();
        let lines = game
            .server
            .block_entities()
            .signs()
            .find(|(at, _)| *at == position)
            .map_or_else(Default::default, |(_, lines)| {
                lines.clone().map(|line| unformatted_text(&line, &lang))
            });
        requests.push(TextScreenRequest::Sign { position, lines });
    }
    if let Some(stack) = game.pending_book.take() {
        requests.push(TextScreenRequest::Book {
            stack,
            author: game.own_name.clone(),
        });
    }
    for request in requests {
        let _ = frames.try_send(RenderEvent::TextScreen(Box::new(request)));
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
    umicraft_core::profile_scope!("game tick");
    game.status_data.ticks += 1;
    run_attack(game, queue);
    run_use(game, queue);
    publish_text_screens(game, frames);
    tick_hand(game);

    // The title and the action bar fade with the tick, so the HUD is
    // published again on every tick while one of them is displayed.
    if game.title.title().is_some()
        || game.title.subtitle().is_some()
        || game.action_bar.message().is_some()
    {
        game.title.tick();
        game.action_bar.tick();
        game.hud_dirty = true;
    }
    if social_view::tick_boss_bar(&mut game.boss_bar, &game.entities) {
        game.hud_dirty = true;
    }

    // The intent of the tick: the raw key state with the yaw and the pitch
    // the player is looking at.
    let intent = RawInput {
        forward: game.input.forward,
        strafe: game.input.strafe,
        jump: game.input.jump,
        sneak: game.input.sneak,
        sprint: game.input.sprint,
        using_item: game.item_use.is_using() && game.vehicle.is_none(),
        yaw: game.yaw,
        pitch: game.pitch,
    }
    .to_intent();

    // A player who spectates another entity does not control its own body
    // (MCP `EntityPlayerSP.updateEntityActionState`, `isCurrentViewEntity`).
    // A dead player does not move either (`EntityLivingBase.isMovementBlocked`).
    let intent = if game.spectating() || game.dead() {
        InputIntent::still(intent.yaw)
    } else {
        intent
    };
    game.death_time = if game.dead() {
        (game.death_time + 1).min(DEATH_TICKS)
    } else {
        0
    };
    apply_movement_stats(game);
    let was_flying = game
        .state
        .as_ref()
        .is_some_and(|state| state.player().flying);

    // The simulation only runs on a loaded world.
    let ready = game.world_ready();
    if ready {
        let seat = game
            .vehicle
            .and_then(|vehicle| game.entities.rider_feet(EntityId(vehicle)));
        if let (Some(state), Some(world)) = (game.state.as_mut(), game.world.as_ref()) {
            match seat {
                Some(feet) => state.ride(feet),
                None => state.tick(&intent, world),
            }
        }
    }
    report_flight(game, was_flying, queue);
    follow_body(game);
    tick_overlays(game);
    game.entities.tick();
    if let Some(world) = game.world.as_ref() {
        game.entities.tick_objects(world);
        game.particles.tick(world);
    }
    combat::tick_emitters(game);
    spawn_ambient_particles(game);
    game.server.tick();
    game.server
        .tick_lightning(game.entities.lightning_flashing());
    tick_pistons(game);
    // A player inside a block is pushed out sideways by the simulation
    // itself, like the vanilla client (`umicraft_game::physics`); it is
    // never lifted.

    report_movement(game, &intent, queue);
    publish_chunks(game, chunks);

    publish_frame(game, frames, tick_time);
    publish_sounds(game, frames);
    tick_hud_view(game);
    if let Some(strength) = game.hud_view.take_jump() {
        queue.ordered.push_back(ClientCommand::RidingJump {
            entity_id: game.entity_id,
            strength,
        });
    }
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

/// Advances the status rows and bars of the HUD by one tick and asks for a new
/// HUD when they changed (they shake, blink and fade with the tick).
fn tick_hud_view(game: &mut Game) {
    let head_in_water = match (game.state.as_ref(), game.world.as_ref()) {
        (Some(state), Some(world)) => {
            crate::fog_context::medium_at(world, state.player().eye_position())
                == umicraft_render::FogMedium::Water
        }
        _ => false,
    };
    let facts = crate::hud_view::HudFacts {
        mode: game.gamemode,
        health: game.health,
        food: game.food,
        armor: game.armor,
        experience: game.experience,
        level: game.level,
        effects: game.server.effects(),
        attributes: game.server.attributes(),
        vehicle: game.vehicle,
        entities: &game.entities,
        head_in_water,
        held: game.windows.selected_item(),
        jump: game.input.jump,
    };
    if game.hud_view.tick(&facts) {
        game.hud_dirty = true;
    }
}

/// Advances the moving piston blocks and lets them push the local player.
///
/// The reference updates the entities (the player) first and the tile
/// entities after them (`World.updateEntities`), so the push lands on the
/// position the player just moved to. A spectator passes through blocks and
/// is not pushed; other entities are placed by the server.
fn tick_pistons(game: &mut Game) {
    let pushable = !game.spectating();
    let Some(world) = game.world.as_mut() else {
        return;
    };
    let mut player = game.state.as_mut().filter(|_| pushable);
    let dirty = motion::tick(
        world,
        game.server.moving_blocks_mut(),
        |world, block, progress, distance| {
            if let Some(state) = player.as_mut() {
                state.launch_by_piston(world, block, progress, distance);
            }
        },
    );
    game.dirty.extend(dirty);
}

/// Sends the movement packets of the tick (`umicraft_game::traffic`).
///
/// Nothing is sent before the server is in the play state and the player is
/// placed (a play packet that arrives during the login makes the server log
/// `Bad packet id`), nor while the chunk under the player is missing: MCP
/// `EntityPlayerSP.onUpdate` skips the whole update until
/// `World.isBlockLoaded` holds for the player's column. The steer packet carries the raw wire values; the 0.98
/// damping of `EntityLivingBase.java:2020` is applied by the game logic and
/// by the server, so it must not be applied twice.
fn report_movement(game: &mut Game, intent: &InputIntent, queue: &mut CommandQueue) {
    let Some(state) = game
        .state
        .as_ref()
        .filter(|_| game.playing && !game.spectating())
    else {
        return;
    };
    let position = state.position();
    let column = ChunkPos::new(
        (position.x.floor() as i32).div_euclid(16),
        (position.z.floor() as i32).div_euclid(16),
    );
    if game
        .world
        .as_ref()
        .and_then(|world| world.chunk(column))
        .is_none()
    {
        return;
    }
    let player = state.player();
    let snapshot = TickSnapshot {
        position,
        yaw: game.yaw.to_degrees(),
        pitch: game.pitch.to_degrees(),
        on_ground: player.on_ground,
        sprinting: player.sprinting,
        sneaking: player.sneaking,
        riding: game.vehicle.map(|_| SteerInput {
            strafe: intent.strafe,
            forward: intent.forward,
            jump: intent.jump,
            sneak: intent.sneak,
        }),
    };
    let packets = game.server_view.tick(&snapshot);
    if packets.reports_position() {
        game.status_data.position_reports += 1;
    }
    for packet in packets.iter() {
        queue.push(movement_command(*packet, game.entity_id));
    }
}

/// The command of one movement packet of the tick.
fn movement_command(packet: TickPacket, entity_id: i32) -> ClientCommand {
    match packet {
        TickPacket::EntityAction(action) => ClientCommand::EntityAction { entity_id, action },
        TickPacket::PositionLook {
            position,
            yaw,
            pitch,
            on_ground,
        } => ClientCommand::PositionLook {
            x: position.x,
            y: position.y,
            z: position.z,
            yaw,
            pitch,
            on_ground,
        },
        TickPacket::Position {
            position,
            on_ground,
        } => ClientCommand::PlayerPosition {
            x: position.x,
            y: position.y,
            z: position.z,
            on_ground,
        },
        TickPacket::Look {
            yaw,
            pitch,
            on_ground,
        } => ClientCommand::PlayerLook {
            yaw,
            pitch,
            on_ground,
        },
        TickPacket::Ground { on_ground } => ClientCommand::Player { on_ground },
        TickPacket::SteerVehicle(steer) => ClientCommand::Input {
            strafe: steer.strafe,
            forward: steer.forward,
            jumping: steer.jump,
            sneaking: steer.sneak,
        },
    }
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

/// Publishes the inventory state to the render thread.
fn publish_inventory(game: &Game, frames: &SyncSender<RenderEvent>) {
    let _ = frames.try_send(RenderEvent::Inventory(Box::new(game.windows.view())));
}

/// Publishes the statistic counters the server sent, by name.
fn publish_stats(game: &Game, frames: &SyncSender<RenderEvent>) {
    let stats = game
        .server
        .statistics()
        .iter()
        .map(|(name, value)| (name.clone(), *value))
        .collect();
    let _ = frames.try_send(RenderEvent::Stats(stats));
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
    use umicraft_game::item_use::{DropKind, UseAction};
    use umicraft_protocol::packets::play::serverbound::DiggingStatus;
    use umicraft_protocol::packets::play::ItemStack as WireItemStack;
    use umicraft_render::plain_text;

    /// An `ench` list with one enchantment, as a server would send it.
    fn enchanted_nbt(id: i16, level: i16) -> Vec<u8> {
        let mut nbt = vec![10, 0, 0, 9, 0, 4];
        nbt.extend(b"ench");
        nbt.extend([10, 0, 0, 0, 1]);
        nbt.extend([2, 0, 2]);
        nbt.extend(b"id");
        nbt.extend(id.to_be_bytes());
        nbt.extend([2, 0, 3]);
        nbt.extend(b"lvl");
        nbt.extend(level.to_be_bytes());
        nbt.extend([0, 0]);
        nbt
    }

    #[test]
    fn the_armour_bar_follows_the_armour_slots() {
        let mut game = placed_game();
        game.windows.apply_set_slot(0, 5, wire_item(310, 0));
        game.windows.apply_set_slot(0, 6, wire_item(311, 0));
        sync_armor(&mut game);
        assert_eq!(game.armor, 11.0);
    }

    /// A placed, playing game on the floor of a loaded chunk, for the
    /// traffic tests.
    pub(super) fn placed_game() -> Game {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.playing = true;
        game
    }

    #[test]
    fn the_f3_facts_hold_the_feet_the_biome_and_the_chunk_cache() {
        let game = placed_game();
        let facts = debug_world(&game);
        let feet = game.state.as_ref().expect("state").position();
        assert_eq!(facts.feet, [feet.x, feet.y, feet.z]);
        assert_eq!(
            facts.chunk_cache.0,
            game.world.as_ref().expect("world").chunk_count()
        );
        let place = facts.location.expect("the player column is loaded");
        assert_eq!(place.day, 0);
        assert!(place.sky_light <= 15);
    }

    #[test]
    fn nothing_is_sent_while_the_player_column_is_not_loaded() {
        // Regression: Umicraft sent bursts of packets before the world
        // loaded; vanilla stays silent until the column is there.
        let mut game = Game::new();
        game.playing = true;
        game.state = Some(GameState::new(DVec3::new(0.0, 4.0, 0.0)));
        game.world = Some(World::new(0));
        let mut queue = CommandQueue::default();
        report_movement(&mut game, &InputIntent::still(0.0), &mut queue);
        assert!(queue.position.is_none() && queue.ordered.is_empty());
    }

    #[test]
    fn an_idle_tick_sends_the_bare_player_packet_and_no_steer() {
        let mut game = placed_game();
        let mut queue = CommandQueue::default();
        let still = InputIntent::still(0.0);
        report_movement(&mut game, &still, &mut queue);
        assert_eq!(game.status_data.position_reports, 1);
        report_movement(&mut game, &still, &mut queue);
        // Regression: `Steer Vehicle` used to be sent on every tick on foot.
        assert!(queue.input.is_none());
        assert_eq!(
            queue.position,
            Some(ClientCommand::Player { on_ground: false })
        );
    }

    #[test]
    fn a_riding_tick_sends_look_then_steer() {
        let mut game = placed_game();
        game.vehicle = Some(42);
        let mut queue = CommandQueue::default();
        report_movement(&mut game, &InputIntent::still(0.0), &mut queue);
        assert!(matches!(
            queue.position,
            Some(ClientCommand::PlayerLook { .. })
        ));
        assert!(matches!(queue.input, Some(ClientCommand::Input { .. })));
        assert_eq!(game.status_data.position_reports, 0);
    }

    #[test]
    fn a_sneak_transition_is_announced_before_the_movement() {
        let mut game = placed_game();
        game.entity_id = 7;
        let mut queue = CommandQueue::default();
        let mut sneaking = GameState::new(DVec3::new(0.0, 4.0, 0.0));
        sneaking.tick(
            &RawInput {
                sneak: true,
                ..RawInput::default()
            }
            .to_intent(),
            &umicraft_world::World::new(0),
        );
        game.state = Some(sneaking);
        report_movement(&mut game, &InputIntent::still(0.0), &mut queue);
        assert_eq!(
            queue.ordered.front(),
            Some(&ClientCommand::EntityAction {
                entity_id: 7,
                action:
                    umicraft_protocol::packets::play::serverbound::EntityActionKind::StartSneaking,
            })
        );
    }

    #[test]
    fn mounting_and_dismounting_follow_attach_entity() {
        let mut game = placed_game();
        game.entity_id = 7;
        let mut queue = CommandQueue::default();
        let (frames, _rx) = mpsc::sync_channel(8);
        let attach = |vehicle_id| {
            PlayEvent::AttachEntity(
                umicraft_protocol::packets::play::clientbound::AttachEntity {
                    leash: 0,
                    entity_id: 7,
                    vehicle_id,
                },
            )
        };
        handle_net(&mut game, attach(42), &mut queue, &frames);
        assert_eq!(game.vehicle, Some(42));
        handle_net(&mut game, attach(-1), &mut queue, &frames);
        assert_eq!(game.vehicle, None);
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
    fn a_game_state_change_switches_the_gamemode() {
        use umicraft_protocol::packets::play::clientbound::ChangeGameState;

        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        let change = |reason, value| PlayEvent::ChangeGameState(ChangeGameState { reason, value });
        handle_net(&mut game, change(3, 3.0), &mut queue, &frames);
        assert_eq!(game.gamemode, GameMode::Spectator);
        assert_eq!(game.status_data.gamemode, 3);
        let player = game.state.as_ref().expect("placed").player();
        assert!(player.no_clip && player.allow_flying);

        handle_net(&mut game, change(3, 0.0), &mut queue, &frames);
        assert_eq!(game.gamemode, GameMode::Survival);
        assert!(!game.state.as_ref().expect("placed").player().no_clip);

        // Rain is not a gamemode.
        handle_net(&mut game, change(1, 3.0), &mut queue, &frames);
        assert_eq!(game.gamemode, GameMode::Survival);
    }

    #[test]
    fn the_server_abilities_let_a_creative_player_fly() {
        use umicraft_protocol::packets::play::clientbound::PlayerAbilities;

        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        let abilities = PlayEvent::PlayerAbilities(PlayerAbilities {
            flags: 0b1101,
            fly_speed: 0.1,
            walk_speed: 0.1,
        });
        handle_net(&mut game, abilities, &mut queue, &frames);
        let player = game.state.as_ref().expect("placed").player();
        assert!(player.allow_flying);
        assert!(!player.flying);
        assert_eq!(player.fly_speed, 0.1);
    }

    #[test]
    fn a_change_of_the_flying_state_is_reported_to_the_server() {
        use umicraft_protocol::packets::play::clientbound::PlayerAbilities;

        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        game.server.apply_abilities(&PlayerAbilities {
            flags: 0b1100,
            fly_speed: 0.05,
            walk_speed: 0.1,
        });
        report_flight(&game, false, &mut queue);
        assert!(queue.ordered.is_empty(), "nothing changed");

        game.state
            .as_mut()
            .expect("placed")
            .set_abilities(true, true, 0.05);
        report_flight(&game, false, &mut queue);
        assert!(matches!(
            queue.ordered.front(),
            Some(ClientCommand::PlayerAbilities { flying: true, .. })
        ));
    }

    fn digging_commands(queue: &CommandQueue) -> usize {
        queue
            .ordered
            .iter()
            .filter(|command| matches!(command, ClientCommand::PlayerDigging { .. }))
            .count()
    }

    #[test]
    fn a_held_survival_click_digs_the_block_over_several_ticks_and_cracks_it() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        game.entity_id = 1;
        game.pitch = 1.5;
        game.input.attack = true;
        game.attack_pressed = true;
        let block = BlockPos::new(0, 63, 0);
        let mut statuses = Vec::new();
        let mut cracked = false;
        // The player never landed in this fixture, so the dig is five times
        // slower (airborne): 0.6 hardness takes about 90 ticks.
        for _ in 0..300 {
            run_attack(&mut game, &mut queue);
            while let Some(command) = queue.ordered.pop_front() {
                if let ClientCommand::PlayerDigging { status, .. } = command {
                    statuses.push(status);
                }
            }
            if game.world.as_ref().expect("world").block(block) == BlockState::AIR {
                break;
            }
            // Once the progress passes 10 % the block shows cracks.
            if !game.cracks().is_empty() {
                cracked = true;
            }
        }
        assert!(cracked, "the block never showed a crack while it was dug");
        assert!(game.cracks().is_empty(), "the cracks go with the block");
        assert_eq!(
            statuses.first(),
            Some(&umicraft_protocol::packets::play::serverbound::DiggingStatus::StartDestroyBlock)
        );
        assert_eq!(
            statuses.last(),
            Some(&umicraft_protocol::packets::play::serverbound::DiggingStatus::StopDestroyBlock)
        );
        assert!(game.world.as_ref().expect("world").block(block).is_air());
    }

    #[test]
    fn a_creative_break_removes_the_block_at_once_and_sends_only_the_start() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        game.gamemode = GameMode::Creative;
        game.pitch = 1.5;
        game.attack_pressed = true;
        game.input.attack = true;
        run_attack(&mut game, &mut queue);
        let world = game.world.as_ref().expect("world");
        assert_eq!(world.block(BlockPos::new(0, 63, 0)), BlockState::AIR);
        assert_eq!(digging_commands(&queue), 1);
        assert_eq!(game.interaction.digging_block(), None);
    }

    #[test]
    fn a_spectator_cannot_break_blocks() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        game.gamemode = GameMode::Spectator;
        game.pitch = 1.5;
        game.attack_pressed = true;
        game.input.attack = true;
        run_attack(&mut game, &mut queue);
        assert_eq!(digging_commands(&queue), 0);
    }

    #[test]
    fn the_camera_packet_moves_the_view_to_the_entity() {
        use umicraft_protocol::packets::play::clientbound::camera::Camera;

        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, received) = mpsc::sync_channel(16);
        game.entity_id = 1;
        game.server.on_join(1);
        game.entities.spawn_mob(
            EntityId(9),
            90,
            DVec3::new(40.0, 70.0, 40.0),
            0.0,
            0.0,
            0.0,
            Vec3::ZERO,
            EntityFlags::default(),
        );
        handle_net(
            &mut game,
            PlayEvent::Camera(Camera { entity_id: 9 }),
            &mut queue,
            &frames,
        );
        assert!(game.spectating());
        publish_frame(&game, &frames, Instant::now());
        let Ok(RenderEvent::Frame(frame)) = received.try_recv() else {
            panic!("a frame was expected");
        };
        assert!((frame.state.position.x - 40.0).abs() < 1e-3);

        handle_net(
            &mut game,
            PlayEvent::Camera(Camera { entity_id: 1 }),
            &mut queue,
            &frames,
        );
        assert!(!game.spectating(), "the local player is the view again");
    }

    #[test]
    fn the_body_in_f5_holds_the_selected_item() {
        // Regression: the local body was drawn with empty hands.
        let mut game = game_on_floor_with(BlockState::AIR);
        game.entity_id = 1;
        follow_body(&mut game);
        assert_eq!(game.body.frame_data().held, None);
        game.windows.apply_set_slot(0, 36, wire_item(276, 0));
        follow_body(&mut game);
        assert_eq!(
            game.body.frame_data().held.map(|held| held.item_id),
            Some(276)
        );
    }

    #[test]
    fn a_rider_follows_its_vehicle_instead_of_walking() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.entity_id = 7;
        game.entities.spawn_object(
            EntityId(42),
            1,
            DVec3::new(8.0, 70.0, 8.0),
            0.0,
            0.0,
            glam::Vec3::ZERO,
            0,
        );
        game.vehicle = Some(42);
        load_surrounding_columns(&mut game);
        let seat = game.entities.rider_feet(EntityId(42)).expect("tracked");
        let mut queue = CommandQueue::default();
        let (frames, _rx) = mpsc::sync_channel(8);
        let (chunks, _chunk_rx) = mpsc::sync_channel(8);
        game.input.forward = 1.0;
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        let position = game.state.as_ref().expect("state").position();
        assert!(
            (position - seat).length() < 1e-9,
            "{position:?} vs {seat:?}"
        );
    }

    #[test]
    fn f5_moves_the_camera_back_and_shows_the_player_body() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.entity_id = 1;
        let mut queue = CommandQueue::default();
        let (frames, received) = mpsc::sync_channel(16);
        let frame = |game: &Game| {
            publish_frame(game, &frames, Instant::now());
            match received.try_recv() {
                Ok(RenderEvent::Frame(frame)) => frame,
                _ => panic!("a frame was expected"),
            }
        };

        follow_body(&mut game);
        let first = frame(&game);
        assert!(first.entities.is_empty(), "no body in first person");

        let toggle = InputUpdate {
            toggle_perspective: true,
            ..InputUpdate::default()
        };
        handle_input(&mut game, toggle, &mut queue);
        let third = frame(&game);
        assert_eq!(third.entities.len(), 1);
        assert_eq!(third.entities[0].id, EntityId(1));
        let moved = (third.state.position - first.state.position).length();
        assert!((moved - 4.0).abs() < 0.5, "moved {moved}");

        handle_input(&mut game, toggle, &mut queue);
        handle_input(&mut game, toggle, &mut queue);
        assert!(
            frame(&game).entities.is_empty(),
            "the cycle is back to first person"
        );
    }

    /// Loads the 3x3 columns around the player: the simulation only runs once
    /// they are there.
    fn load_surrounding_columns(game: &mut Game) {
        let payload = vec![0u8; umicraft_world::chunk::payload_len(0b1000, true, true)];
        let world = game.world.as_mut().expect("world");
        for (x, z) in (-1..=1).flat_map(|x| (-1..=1).map(move |z| (x, z))) {
            if (x, z) != (0, 0) {
                let column = umicraft_world::ChunkPayload {
                    primary_bit_mask: 0b1000,
                    ground_up: true,
                    data: &payload,
                };
                world
                    .apply_chunk(ChunkPos::new(x, z), column)
                    .expect("an empty column");
            }
        }
    }

    #[test]
    fn the_local_body_swings_its_limbs_while_walking() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.entity_id = 1;
        game.playing = true;
        game.input.forward = 1.0;
        load_surrounding_columns(&mut game);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(64);
        let (chunks, _chunk_received) = mpsc::sync_channel(64);
        for _ in 0..10 {
            tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        }
        let walking = game.body.frame_data();
        assert!(walking.limbs.amount > 0.1, "{}", walking.limbs.amount);
        assert!(walking.limbs.swing > 0.0);

        game.input.forward = 0.0;
        for _ in 0..60 {
            tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        }
        assert!(game.body.frame_data().limbs.amount < 0.05);
    }

    #[test]
    fn the_cracks_follow_every_breaker_and_vanish_with_the_block() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let anim = |breaker: i32, stage: u8, game: &mut Game| {
            game.server.apply_block_break_anim(&BlockBreakAnim {
                entity_id: breaker,
                x: 0,
                y: 63,
                z: 0,
                stage,
            });
        };
        assert!(game.cracks().is_empty());
        anim(7, 4, &mut game);
        let cracks = game.cracks();
        assert_eq!(cracks.len(), 1);
        assert_eq!(cracks[0].stage, 4);
        assert_eq!(cracks[0].bounds.min, [0.0, 63.0, 0.0]);
        assert_eq!(cracks[0].bounds.max, [1.0, 64.0, 1.0]);

        // A second player digging elsewhere is drawn as well.
        game.server.apply_block_break_anim(&BlockBreakAnim {
            entity_id: 8,
            x: 1,
            y: 63,
            z: 0,
            stage: 2,
        });
        assert_eq!(game.cracks().len(), 2);

        anim(7, 255, &mut game);
        assert_eq!(game.cracks().len(), 1, "stage > 9 clears the breaker");
    }

    fn health_update(health: f32) -> PlayEvent {
        PlayEvent::UpdateHealth(
            umicraft_protocol::packets::play::clientbound::UpdateHealth {
                health,
                food: 20,
                saturation: 5.0,
            },
        )
    }

    #[test]
    fn the_hud_carries_the_rows_and_bars_of_the_tick() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.experience = 0.5;
        game.level = 4;
        let (frames, _received) = mpsc::sync_channel(16);
        handle_net(
            &mut game,
            health_update(7.0),
            &mut CommandQueue::default(),
            &frames,
        );
        game.health = 7.0;
        tick_hud_view(&mut game);
        let hud = game.hud();
        let hearts = hud.vitals.expect("survival shows the rows").hearts;
        assert_eq!(hearts.len(), 10);
        assert!(matches!(
            hud.bar,
            umicraft_game::hud::BottomBar::Experience {
                fill: 91,
                level: Some(4)
            }
        ));
    }

    #[test]
    fn a_dead_player_neither_moves_attacks_nor_places() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.playing = true;
        game.input.forward = 1.0;
        game.input.attack = true;
        game.attack_pressed = true;
        game.pitch = 1.5;
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        let (chunks, _chunk_received) = mpsc::sync_channel(8);
        handle_net(&mut game, health_update(0.0), &mut queue, &frames);
        assert!(game.dead());
        let before = game.state.as_ref().expect("placed").position();
        for _ in 0..5 {
            tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        }
        let after = game.state.as_ref().expect("placed").position();
        assert_eq!(before.x, after.x);
        assert_eq!(before.z, after.z);
        assert_eq!(digging_commands(&queue), 0);
        click_use(&mut game, &mut queue);
        assert!(!queue
            .ordered
            .iter()
            .any(|command| matches!(command, ClientCommand::PlayerBlockPlacement { .. })));
    }

    #[test]
    fn the_death_animation_runs_twenty_ticks_and_stops_with_the_respawn() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(64);
        let (chunks, _chunk_received) = mpsc::sync_channel(64);
        handle_net(&mut game, health_update(0.0), &mut queue, &frames);
        for _ in 0..30 {
            tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        }
        assert_eq!(game.death_time, DEATH_TICKS);
        handle_net(&mut game, health_update(20.0), &mut queue, &frames);
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        assert_eq!(game.death_time, 0);
    }

    #[test]
    fn the_dead_view_rolls_and_narrows() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, received) = mpsc::sync_channel(64);
        let (chunks, _chunk_received) = mpsc::sync_channel(64);
        handle_net(&mut game, health_update(0.0), &mut queue, &frames);
        for _ in 0..20 {
            tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        }
        let mut last = None;
        while let Ok(event) = received.try_recv() {
            if let RenderEvent::Frame(frame) = event {
                last = Some(frame);
            }
        }
        let frame = last.expect("a frame");
        assert!(frame.state.effects.bob.roll_degrees > 3.0);
        assert!(frame.state.effects.fov_modifier < 1.0);
    }

    #[test]
    fn the_respawn_button_sends_the_request_only_to_a_dead_player() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        handle_event(&mut game, GameEvent::Respawn, &mut queue, &frames);
        assert!(queue.ordered.is_empty(), "a living player cannot respawn");

        handle_net(&mut game, health_update(0.0), &mut queue, &frames);
        handle_event(&mut game, GameEvent::Respawn, &mut queue, &frames);
        assert!(matches!(
            queue.ordered.front(),
            Some(ClientCommand::Respawn)
        ));
    }

    #[test]
    fn the_score_comes_from_the_local_player_metadata() {
        use umicraft_protocol::packets::play::clientbound::EntityMetadata;
        use umicraft_protocol::packets::play::metadata::WatchedObject;

        let mut game = game_on_floor_with(BlockState::AIR);
        game.entity_id = 3;
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        let metadata = |entity_id, score: i32| {
            PlayEvent::EntityMetadata(EntityMetadata {
                entity_id,
                metadata: Metadata {
                    objects: vec![WatchedObject {
                        type_id: 2,
                        index: 18,
                        value: score.to_be_bytes().to_vec(),
                    }],
                },
            })
        };
        handle_net(&mut game, metadata(9, 100), &mut queue, &frames);
        assert_eq!(game.score, 0, "another entity's score is not ours");
        handle_net(&mut game, metadata(3, 57), &mut queue, &frames);
        assert_eq!(game.score, 57);
        assert_eq!(game.hud().score, 57);
    }

    fn spawn_object_event(entity_id: i32, object_type: u8, data: i32) -> PlayEvent {
        use umicraft_protocol::packets::play::clientbound::SpawnObject;

        PlayEvent::SpawnObject(SpawnObject {
            entity_id,
            object_type,
            x: 0,
            y: 0,
            z: 0,
            pitch: 0,
            yaw: 0,
            data,
            velocity_x: 0,
            velocity_y: 0,
            velocity_z: 0,
        })
    }

    fn stack_watcher(
        index: u8,
        stack: &WireItemStack,
    ) -> umicraft_protocol::packets::play::metadata::WatchedObject {
        let mut value = Vec::new();
        stack.encode(&mut value).expect("encodes");
        umicraft_protocol::packets::play::metadata::WatchedObject {
            type_id: 5,
            index,
            value,
        }
    }

    fn metadata_event(
        entity_id: i32,
        objects: Vec<umicraft_protocol::packets::play::metadata::WatchedObject>,
    ) -> PlayEvent {
        use umicraft_protocol::packets::play::clientbound::EntityMetadata;

        PlayEvent::EntityMetadata(EntityMetadata {
            entity_id,
            metadata: Metadata { objects },
        })
    }

    #[test]
    fn the_stack_of_a_dropped_item_comes_from_watcher_ten() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        handle_net(&mut game, spawn_object_event(7, 2, 1), &mut queue, &frames);
        let event = metadata_event(7, vec![stack_watcher(10, &wire_item(264, 0))]);
        handle_net(&mut game, event, &mut queue, &frames);
        let snapshot = &game.entities.snapshots(0.0)[0];
        let item = snapshot.item.as_ref().expect("stack");
        assert_eq!((item.item_id, item.count), (264, 1));
    }

    #[test]
    fn a_collected_item_leaves_the_world() {
        use umicraft_protocol::packets::play::clientbound::CollectItem;

        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        handle_net(&mut game, spawn_object_event(7, 2, 1), &mut queue, &frames);
        assert_eq!(game.entities.snapshots(0.0).len(), 1);
        let event = PlayEvent::CollectItem(CollectItem {
            collected_entity_id: 7,
            collector_entity_id: 1,
        });
        handle_net(&mut game, event, &mut queue, &frames);
        assert_eq!(game.entities.snapshots(0.0).len(), 1, "it flies first");
        for _ in 0..3 {
            game.entities.tick();
        }
        assert!(game.entities.snapshots(0.0).is_empty());
    }

    #[test]
    fn an_item_frame_takes_its_stack_rotation_and_direction() {
        use umicraft_protocol::packets::play::metadata::WatchedObject;

        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        handle_net(&mut game, spawn_object_event(8, 71, 2), &mut queue, &frames);
        let rotation = WatchedObject {
            type_id: 0,
            index: 9,
            value: vec![0x0B],
        };
        let event = metadata_event(8, vec![stack_watcher(8, &wire_item(276, 4)), rotation]);
        handle_net(&mut game, event, &mut queue, &frames);
        let snapshot = &game.entities.snapshots(0.0)[0];
        assert_eq!(snapshot.frame_direction, 2);
        assert_eq!(snapshot.frame_rotation, 3);
        let item = snapshot.item.as_ref().expect("stack");
        assert_eq!((item.item_id, item.damage), (276, 4));
    }

    #[test]
    fn dropped_item_watchers_do_not_leak_onto_other_kinds() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        handle_net(&mut game, spawn_object_event(9, 60, 0), &mut queue, &frames);
        let event = metadata_event(9, vec![stack_watcher(10, &wire_item(264, 0))]);
        handle_net(&mut game, event, &mut queue, &frames);
        assert_eq!(game.entities.snapshots(0.0)[0].item, None);
    }

    #[test]
    fn the_equipment_of_another_entity_sets_its_held_stack() {
        use umicraft_protocol::packets::play::clientbound::EntityEquipment;

        let mut game = game_on_floor_with(BlockState::AIR);
        game.entity_id = 1;
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        game.entities.spawn_player(
            EntityId(4),
            [0; 16],
            DVec3::ZERO,
            0.0,
            0.0,
            None,
            EntityFlags::default(),
        );
        let equip = |item| {
            PlayEvent::EntityEquipment(EntityEquipment {
                entity_id: 4,
                slot: 0,
                item,
            })
        };
        handle_net(&mut game, equip(wire_item(276, 9)), &mut queue, &frames);
        let held = game.entities.snapshots(0.0)[0].held.clone().expect("held");
        assert_eq!((held.item_id, held.damage), (276, 9));

        handle_net(&mut game, equip(WireItemStack::EMPTY), &mut queue, &frames);
        assert_eq!(game.entities.snapshots(0.0)[0].held, None);
    }

    #[test]
    fn a_tab_list_entry_gives_the_spawned_player_its_name() {
        use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;

        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        let uuid = [7u8; 16];
        game.entities.spawn_player(
            EntityId(4),
            uuid,
            DVec3::ZERO,
            0.0,
            0.0,
            None,
            EntityFlags::default(),
        );
        let entry = AddPlayerEntry {
            uuid: u128::from_be_bytes(uuid),
            name: "Notch".to_owned(),
            properties: Vec::new(),
            game_mode: 0,
            ping: 0,
            display_name: None,
        };
        let event = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![entry]));
        handle_net(&mut game, event, &mut queue, &frames);
        let profile = game.entities.snapshots(0.0)[0].profile.clone();
        assert_eq!(
            profile.map(|p| p.name.to_string()),
            Some("Notch".to_owned())
        );
        assert_eq!(
            game.server.player_list().len(),
            1,
            "the tab list still updates"
        );
    }

    #[test]
    fn the_local_body_swings_its_arm_when_the_player_does() {
        let mut game = game_on_floor_with(BlockState::AIR);
        follow_body(&mut game);
        assert_eq!(game.body.animation.swing_progress_at(1.0), 0.0);
        game.body.animation.swing_item();
        for _ in 0..3 {
            follow_body(&mut game);
        }
        assert!(game.body.animation.swing_progress_at(1.0) > 0.0);
    }

    #[test]
    fn the_local_body_wears_the_profile_of_its_own_tab_list_entry() {
        use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;

        let mut game = game_on_floor_with(BlockState::AIR);
        game.own_name = "Me".to_owned();
        follow_body(&mut game);
        assert!(game.body.profile.is_none(), "not listed yet");
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        let entry = |uuid: u8, name: &str| AddPlayerEntry {
            uuid: u128::from_be_bytes([uuid; 16]),
            name: name.to_owned(),
            properties: Vec::new(),
            game_mode: 0,
            ping: 0,
            display_name: None,
        };
        let event = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![
            entry(1, "Other"),
            entry(2, "Me"),
        ]));
        handle_net(&mut game, event, &mut queue, &frames);
        follow_body(&mut game);
        assert_eq!(game.body.uuid, Some([2; 16]));
        assert_eq!(
            game.body.profile.as_ref().map(|p| p.name.to_string()),
            Some("Me".to_owned())
        );
    }

    #[test]
    fn a_team_mate_is_a_ghost_only_when_the_team_sees_invisible_mates() {
        use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;
        use umicraft_protocol::packets::play::clientbound::teams::{TeamInfo, Teams};

        let mut game = game_on_floor_with(BlockState::AIR);
        game.own_name = "me".to_owned();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        let uuid = [9u8; 16];
        game.entities.spawn_player(
            EntityId(4),
            uuid,
            DVec3::ZERO,
            0.0,
            0.0,
            None,
            EntityFlags::default(),
        );
        let entry = AddPlayerEntry {
            uuid: u128::from_be_bytes(uuid),
            name: "Bob".to_owned(),
            properties: Vec::new(),
            game_mode: 0,
            ping: 0,
            display_name: None,
        };
        let listed = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![entry]));
        handle_net(&mut game, listed, &mut queue, &frames);
        let ghost = |game: &Game| game.entities.snapshots(0.0)[0].ghost_visible;
        assert!(!ghost(&game), "no team");

        let team = |flags: i8, players: &[&str]| {
            PlayEvent::Teams(Box::new(Teams::Create {
                name: "red".to_owned(),
                info: TeamInfo {
                    display_name: "Red".to_owned(),
                    prefix: String::new(),
                    suffix: String::new(),
                    friendly_flags: flags,
                    name_tag_visibility: "always".to_owned(),
                    color: 12,
                },
                players: players.iter().map(|name| (*name).to_owned()).collect(),
            }))
        };
        // Bob alone in the team: the local player is not a mate.
        handle_net(&mut game, team(2, &["Bob"]), &mut queue, &frames);
        assert!(!ghost(&game));
        let remove = || {
            PlayEvent::Teams(Box::new(Teams::Remove {
                name: "red".to_owned(),
            }))
        };
        handle_net(&mut game, remove(), &mut queue, &frames);
        // Both in the team, but it does not see invisible mates.
        handle_net(&mut game, team(0, &["Bob", "me"]), &mut queue, &frames);
        assert!(!ghost(&game));
        handle_net(&mut game, remove(), &mut queue, &frames);
        handle_net(&mut game, team(2, &["Bob", "me"]), &mut queue, &frames);
        assert!(ghost(&game));
    }

    #[test]
    fn the_objective_below_the_name_adds_a_score_line_to_the_tag() {
        use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;
        use umicraft_protocol::packets::play::clientbound::{
            DisplayScoreboard, ObjectiveRenderType, ScoreboardObjective, UpdateScore,
        };

        let mut game = game_on_floor_with(BlockState::AIR);
        game.own_name = "me".to_owned();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        let uuid = [9u8; 16];
        game.entities.spawn_player(
            EntityId(4),
            uuid,
            DVec3::ZERO,
            0.0,
            0.0,
            None,
            EntityFlags::default(),
        );
        let entry = AddPlayerEntry {
            uuid: u128::from_be_bytes(uuid),
            name: "Bob".to_owned(),
            properties: Vec::new(),
            game_mode: 0,
            ping: 0,
            display_name: None,
        };
        let listed = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![entry]));
        handle_net(&mut game, listed, &mut queue, &frames);
        let tag = |game: &Game| game.entities.snapshots(0.0)[0].name_tag.clone();
        assert_eq!(tag(&game).as_deref(), Some("Bob"));

        let objective = PlayEvent::ScoreboardObjective(ScoreboardObjective::Create {
            name: "kills".to_owned(),
            value: "Kills".to_owned(),
            render_type: ObjectiveRenderType::Integer,
        });
        handle_net(&mut game, objective, &mut queue, &frames);
        let display = PlayEvent::DisplayScoreboard(DisplayScoreboard {
            position: 2,
            score_name: "kills".to_owned(),
        });
        handle_net(&mut game, display, &mut queue, &frames);
        // No score yet: vanilla shows 0.
        assert_eq!(tag(&game).as_deref(), Some("Bob\n0 Kills"));
        let score = PlayEvent::UpdateScore(UpdateScore::Change {
            name: "Bob".to_owned(),
            objective: "kills".to_owned(),
            value: 12,
        });
        handle_net(&mut game, score, &mut queue, &frames);
        assert_eq!(tag(&game).as_deref(), Some("Bob\n12 Kills"));
    }

    #[test]
    fn a_team_decides_the_name_tag_of_a_player() {
        use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;
        use umicraft_protocol::packets::play::clientbound::teams::{TeamInfo, Teams};

        let mut game = game_on_floor_with(BlockState::AIR);
        game.own_name = "me".to_owned();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        let uuid = [9u8; 16];
        game.entities.spawn_player(
            EntityId(4),
            uuid,
            DVec3::ZERO,
            0.0,
            0.0,
            None,
            EntityFlags::default(),
        );
        let entry = AddPlayerEntry {
            uuid: u128::from_be_bytes(uuid),
            name: "Bob".to_owned(),
            properties: Vec::new(),
            game_mode: 0,
            ping: 0,
            display_name: None,
        };
        let listed = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![entry]));
        handle_net(&mut game, listed, &mut queue, &frames);
        let tag = |game: &Game| game.entities.snapshots(0.0)[0].name_tag.clone();
        assert_eq!(tag(&game).as_deref(), Some("Bob"));

        let team = |visibility: &str| {
            PlayEvent::Teams(Box::new(Teams::Create {
                name: "red".to_owned(),
                info: TeamInfo {
                    display_name: "Red".to_owned(),
                    prefix: "§c".to_owned(),
                    suffix: String::new(),
                    friendly_flags: 0,
                    name_tag_visibility: visibility.to_owned(),
                    color: 12,
                },
                players: vec!["Bob".to_owned()],
            }))
        };
        handle_net(&mut game, team("always"), &mut queue, &frames);
        assert_eq!(tag(&game).as_deref(), Some("§cBob"));
        handle_net(
            &mut game,
            PlayEvent::Teams(Box::new(Teams::Remove {
                name: "red".to_owned(),
            })),
            &mut queue,
            &frames,
        );
        assert_eq!(tag(&game).as_deref(), Some("Bob"));
        handle_net(&mut game, team("never"), &mut queue, &frames);
        assert_eq!(tag(&game), None);
    }

    #[test]
    fn the_velocity_of_the_local_player_replaces_its_motion_and_others_do_not() {
        use umicraft_protocol::packets::play::clientbound::EntityVelocity;

        let mut game = game_on_floor_with(BlockState::AIR);
        game.entity_id = 1;
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        let velocity = |entity_id| {
            PlayEvent::EntityVelocity(EntityVelocity {
                entity_id,
                velocity_x: 4000,
                velocity_y: 2400,
                velocity_z: -8000,
            })
        };
        handle_net(&mut game, velocity(2), &mut queue, &frames);
        let motion = game.state.as_ref().expect("placed").player().velocity;
        assert_eq!(motion, DVec3::ZERO, "another entity is not us");

        handle_net(&mut game, velocity(1), &mut queue, &frames);
        let motion = game.state.as_ref().expect("placed").player().velocity;
        assert!((motion - DVec3::new(0.5, 0.3, -1.0)).length() < 1e-9);
    }

    #[test]
    fn an_explosion_pushes_the_local_player() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.state
            .as_mut()
            .expect("placed")
            .set_velocity(DVec3::new(0.1, 0.0, 0.0));
        apply_explosion(
            &mut game,
            &Explosion {
                x: 0.0,
                y: 64.0,
                z: 0.0,
                strength: 3.0,
                records: Vec::new(),
                player_motion_x: 0.5,
                player_motion_y: 0.25,
                player_motion_z: -0.5,
            },
        );
        let motion = game.state.as_ref().expect("placed").player().velocity;
        assert!((motion - DVec3::new(0.6, 0.25, -0.5)).length() < 1e-6);
    }

    #[test]
    fn the_server_state_drives_the_movement_stats() {
        use umicraft_protocol::packets::play::clientbound::EntityEffect;

        let mut game = game_on_floor_with(BlockState::AIR);
        game.food = 6.0;
        game.server.on_join(1);
        game.server.apply_entity_effect(&EntityEffect {
            entity_id: 1,
            effect_id: 1,
            amplifier: 1,
            duration: 600,
            hide_particles: 0,
        });
        game.server.apply_entity_effect(&EntityEffect {
            entity_id: 1,
            effect_id: 8,
            amplifier: 0,
            duration: 600,
            hide_particles: 0,
        });
        apply_movement_stats(&mut game);
        let player = game.state.as_ref().expect("placed").player();
        assert!((player.movement_speed - 0.14).abs() < 1e-6);
        assert_eq!(player.jump_boost, 1);
        assert!(!player.well_fed, "6 food points are not enough to sprint");
    }

    #[test]
    fn a_click_swings_the_hand_and_a_placement_swings_it_too() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        game.pitch = 1.5;
        game.input.attack = true;
        game.attack_pressed = true;
        run_attack(&mut game, &mut queue);
        tick_hand(&mut game);
        assert!(game.hand.pose().swing > 0.0 || game.hand.swing_progress(1.0) >= 0.0);
        assert!(queue
            .ordered
            .iter()
            .any(|command| matches!(command, ClientCommand::Swing)));

        // Placing with an item swings; with an empty hand it does not.
        let mut empty = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        // Aim ahead of the feet: the block below the player cannot be built on.
        empty.pitch = 1.0;
        click_use(&mut empty, &mut queue);
        assert!(!queue
            .ordered
            .iter()
            .any(|command| matches!(command, ClientCommand::Swing)));
        empty.windows.apply_set_slot(
            0,
            36,
            WireItemStack {
                item_id: 1,
                stack_size: 8,
                damage: 0,
                nbt: None,
            },
        );
        click_use(&mut empty, &mut queue);
        assert!(queue
            .ordered
            .iter()
            .any(|command| matches!(command, ClientCommand::Swing)));
    }

    #[test]
    fn the_frame_shows_the_arm_only_in_first_person_with_an_empty_hand() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.entity_id = 1;
        let mut queue = CommandQueue::default();
        let (frames, received) = mpsc::sync_channel(32);
        let frame = |game: &Game| {
            publish_frame(game, &frames, Instant::now());
            match received.try_recv() {
                Ok(RenderEvent::Frame(frame)) => frame,
                _ => panic!("a frame was expected"),
            }
        };
        tick_hand(&mut game);
        assert!(frame(&game).state.effects.hand.shows_arm);
        assert!(frame(&game).state.effects.hand.visible);

        game.gamemode = GameMode::Spectator;
        assert!(!frame(&game).state.effects.hand.shows_arm);
        game.gamemode = GameMode::Survival;

        let toggle = InputUpdate {
            toggle_perspective: true,
            ..InputUpdate::default()
        };
        handle_input(&mut game, toggle, &mut queue);
        assert!(!frame(&game).state.effects.hand.shows_arm, "third person");
        // Regression: the first-person item stayed drawn in F5.
        assert!(!frame(&game).state.effects.hand.visible, "third person");
        handle_input(&mut game, toggle, &mut queue);
        handle_input(&mut game, toggle, &mut queue);

        // Holding an item hides the bare arm once the item is shown.
        game.windows.apply_set_slot(
            0,
            36,
            WireItemStack {
                item_id: 276,
                stack_size: 1,
                damage: 0,
                nbt: None,
            },
        );
        for _ in 0..10 {
            tick_hand(&mut game);
        }
        assert!(!frame(&game).state.effects.hand.shows_arm);
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

    /// Presses the use button once and runs the tick of the button.
    fn click_use(game: &mut Game, queue: &mut CommandQueue) {
        game.use_pressed = true;
        run_use(game, queue);
    }

    fn wire_item(item_id: i16, damage: i16) -> WireItemStack {
        WireItemStack {
            item_id,
            stack_size: 1,
            damage,
            nbt: None,
        }
    }

    fn confirmation(window_id: u8, action_number: i16) -> PlayEvent {
        PlayEvent::ConfirmTransaction(
            umicraft_protocol::packets::play::clientbound::ConfirmTransaction {
                window_id,
                action_number,
                accepted: false,
            },
        )
    }

    /// Regression test (GrimAC `BadPacketsN`, 2026-11-02): the replies leave in
    /// the order of the packets, the position reply between the two
    /// transactions that surround a teleport.
    #[test]
    fn the_replies_to_a_teleport_keep_the_order_of_the_packets() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(64);
        let placement = umicraft_net::play::PlayerPlacement {
            x: 0.5,
            y: 64.0,
            z: 0.5,
            ..Default::default()
        };
        handle_net(&mut game, confirmation(0, -1), &mut queue, &frames);
        handle_net(
            &mut game,
            PlayEvent::Placement(placement),
            &mut queue,
            &frames,
        );
        handle_net(&mut game, confirmation(0, -2), &mut queue, &frames);
        let sent = commands(&queue);
        assert!(matches!(
            sent[..],
            [
                ClientCommand::ConfirmTransaction {
                    action_number: -1,
                    ..
                },
                ClientCommand::AckPosition { .. },
                ClientCommand::ConfirmTransaction {
                    action_number: -2,
                    ..
                },
            ]
        ));
    }

    #[test]
    fn a_transaction_of_a_window_that_is_not_open_gets_no_reply() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(64);
        handle_net(&mut game, confirmation(7, 1), &mut queue, &frames);
        assert!(commands(&queue).is_empty());
    }

    fn commands(queue: &CommandQueue) -> Vec<&ClientCommand> {
        queue.ordered.iter().collect()
    }

    /// Puts a zombie or a player two blocks ahead of the player (yaw 0 looks
    /// along +z) and returns its id.
    fn spawn_ahead(game: &mut Game, player: bool) -> EntityId {
        let id = EntityId(7);
        let position = DVec3::new(0.5, 64.0, 2.5);
        if player {
            game.entities.spawn_player(
                id,
                [1; 16],
                position,
                0.0,
                0.0,
                None,
                EntityFlags::default(),
            );
        } else {
            game.entities.spawn_mob(
                id,
                54,
                position,
                0.0,
                0.0,
                0.0,
                Vec3::ZERO,
                EntityFlags::default(),
            );
        }
        id
    }

    fn click_attack(game: &mut Game, queue: &mut CommandQueue) {
        game.input.attack = true;
        game.attack_pressed = true;
        run_attack(game, queue);
    }

    #[test]
    fn a_left_click_on_an_entity_swings_then_attacks_it() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        spawn_ahead(&mut game, false);
        click_attack(&mut game, &mut queue);
        let sent = commands(&queue);
        assert!(matches!(sent[0], ClientCommand::Swing));
        assert!(matches!(
            sent[1],
            ClientCommand::UseEntity {
                target: 7,
                action: UseEntityAction::Attack
            }
        ));
        assert_eq!(sent.len(), 2, "no dig packet while an entity is targeted");
    }

    #[test]
    fn an_entity_behind_a_block_is_not_attacked() {
        let mut game = game_on_floor_with(BlockState::new(1, 0));
        let mut queue = CommandQueue::default();
        spawn_ahead(&mut game, false);
        game.pitch = 0.0;
        // A wall one block ahead of the eyes.
        if let Some(world) = game.world.as_mut() {
            world.set_block(BlockPos::new(0, 65, 1), BlockState::new(1, 0));
        }
        click_attack(&mut game, &mut queue);
        assert!(!commands(&queue)
            .iter()
            .any(|command| matches!(command, ClientCommand::UseEntity { .. })));
    }

    #[test]
    fn a_knockback_hit_on_a_player_slows_the_attacker_but_a_mob_does_not() {
        for (player, factor) in [(true, 0.6), (false, 1.0)] {
            let mut game = game_on_floor_with(BlockState::AIR);
            let mut queue = CommandQueue::default();
            spawn_ahead(&mut game, player);
            game.windows.apply_set_slot(
                0,
                36,
                WireItemStack {
                    nbt: Some(enchanted_nbt(19, 1)),
                    ..wire_item(267, 0)
                },
            );
            if let Some(state) = game.state.as_mut() {
                state.set_velocity(DVec3::new(1.0, 0.0, 1.0));
            }
            click_attack(&mut game, &mut queue);
            let velocity = game.state.as_ref().unwrap().player().velocity;
            assert!((velocity.x - factor).abs() < 1e-9, "player: {player}");
        }
    }

    #[test]
    fn a_right_click_on_a_mob_interacts_before_using_the_item() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        spawn_ahead(&mut game, false);
        game.windows.apply_set_slot(0, 36, wire_item(267, 0));
        click_use(&mut game, &mut queue);
        let sent = commands(&queue);
        assert!(matches!(
            sent[0],
            ClientCommand::UseEntity {
                action: UseEntityAction::InteractAt { .. },
                ..
            }
        ));
        assert!(matches!(
            sent[1],
            ClientCommand::UseEntity {
                action: UseEntityAction::Interact,
                ..
            }
        ));
        assert!(matches!(sent[2], ClientCommand::UseItem { .. }));
    }

    /// A hungry player who holds bread starts eating on the click, walks at
    /// a fifth of the speed while the button is held, and releases it.
    #[test]
    fn holding_the_use_button_eats_slowly_and_releasing_tells_the_server() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.food = 10.0;
        game.windows.apply_set_slot(0, 36, wire_item(297, 0));
        game.pitch = -0.5;
        let mut queue = CommandQueue::default();
        game.input.use_item = true;
        click_use(&mut game, &mut queue);
        assert!(game.item_use.is_using());
        assert!(matches!(
            commands(&queue)[0],
            ClientCommand::UseItem { item } if item.item_id == 297
        ));

        game.input.use_item = false;
        run_use(&mut game, &mut queue);
        assert!(!game.item_use.is_using());
        assert!(commands(&queue).iter().any(|command| matches!(
            command,
            ClientCommand::PlayerDigging {
                status: DiggingStatus::ReleaseUseItem,
                ..
            }
        )));
    }

    #[test]
    fn the_published_hand_carries_the_item_use_until_it_ends() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.food = 10.0;
        game.windows.apply_set_slot(0, 36, wire_item(297, 0));
        game.pitch = -0.5;
        game.input.use_item = true;
        let mut queue = CommandQueue::default();
        click_use(&mut game, &mut queue);
        let (frames, received) = mpsc::sync_channel(32);
        let frame = |game: &Game| {
            publish_frame(game, &frames, Instant::now());
            match received.try_recv() {
                Ok(RenderEvent::Frame(frame)) => frame,
                _ => panic!("a frame was expected"),
            }
        };
        let using = frame(&game).state.effects.hand.item_use.expect("a use");
        assert_eq!(using.action, UseAction::Eat);
        assert_eq!(using.count, 32.0);
        game.item_use.finish();
        assert_eq!(frame(&game).state.effects.hand.item_use, None);
    }

    #[test]
    fn the_server_finishing_the_meal_ends_the_use() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.food = 10.0;
        game.entity_id = 7;
        game.windows.apply_set_slot(0, 36, wire_item(297, 0));
        game.pitch = -0.5;
        game.input.use_item = true;
        let mut queue = CommandQueue::default();
        click_use(&mut game, &mut queue);
        let (frames, _received) = mpsc::sync_channel(8);
        let status = PlayEvent::EntityStatus(
            umicraft_protocol::packets::play::clientbound::EntityStatus {
                entity_id: 7,
                status: 9,
            },
        );
        handle_net(&mut game, status, &mut queue, &frames);
        assert!(!game.item_use.is_using());
    }

    fn hurt_status(entity_id: i32, status: i8) -> PlayEvent {
        PlayEvent::EntityStatus(
            umicraft_protocol::packets::play::clientbound::EntityStatus { entity_id, status },
        )
    }

    #[test]
    fn a_hurt_status_of_the_local_player_rolls_the_camera() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.entity_id = 7;
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        follow_body(&mut game);
        handle_net(&mut game, health_update(10.0), &mut queue, &frames);
        follow_body(&mut game);
        handle_net(&mut game, hurt_status(7, 2), &mut queue, &frames);
        assert_eq!(game.body.animation.hurt_time(), 10);
        assert!(game.body.animation.is_hurt_resistant());
        // Someone else's hurt does not shake this camera.
        let mut other = game_on_floor_with(BlockState::AIR);
        other.entity_id = 7;
        handle_net(&mut other, hurt_status(8, 2), &mut queue, &frames);
        assert_eq!(other.body.animation.hurt_time(), 0);
    }

    #[test]
    fn a_hurt_status_of_the_local_player_plays_the_hurt_sound_once() {
        // Regression: no damage sound was played when the player was hit.
        let mut game = game_on_floor_with(BlockState::AIR);
        game.entity_id = 7;
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        let feet = game.state.as_ref().unwrap().position();
        handle_net(&mut game, hurt_status(7, 2), &mut queue, &frames);
        let cues = game.state.as_mut().unwrap().drain_sound_cues();
        let hurt: Vec<_> = cues
            .iter()
            .filter(|c| c.name == "game.player.hurt")
            .collect();
        assert_eq!(hurt.len(), 1);
        assert_eq!(hurt[0].pos, feet.to_array());
        handle_net(&mut game, hurt_status(7, 3), &mut queue, &frames);
        let cues = game.state.as_mut().unwrap().drain_sound_cues();
        assert!(cues.iter().any(|c| c.name == "game.player.die"));
    }

    #[test]
    fn a_hurt_status_of_another_entity_is_silent_because_the_server_sends_the_sound() {
        // `RenderGlobal.playSound` is empty in 1.8: playing it here too would
        // double the Sound Effect packet of the same hit.
        let mut game = game_on_floor_with(BlockState::AIR);
        game.entity_id = 7;
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        handle_net(&mut game, hurt_status(8, 2), &mut queue, &frames);
        let cues = game.state.as_mut().unwrap().drain_sound_cues();
        assert!(cues.iter().all(|c| c.name != "game.player.hurt"));
    }

    #[test]
    fn a_critical_animation_throws_stars_for_three_ticks() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        game.entities.spawn_mob(
            EntityId(30),
            54,
            DVec3::new(2.5, 64.0, 0.5),
            0.0,
            0.0,
            0.0,
            Vec3::ZERO,
            EntityFlags::default(),
        );
        let animation = |code| {
            PlayEvent::Animation(umicraft_protocol::packets::play::clientbound::Animation {
                entity_id: 30,
                animation: code,
            })
        };
        // A swing throws nothing.
        handle_net(&mut game, animation(0), &mut queue, &frames);
        assert!(game.particles.is_empty());
        handle_net(&mut game, animation(4), &mut queue, &frames);
        let first = game.particles.len();
        assert!(first > 0, "the first burst comes at once");
        assert_eq!(game.hit_emitters.len(), 1);
        combat::tick_emitters(&mut game);
        combat::tick_emitters(&mut game);
        assert!(game.particles.len() > first);
        assert_eq!(game.hit_emitters.len(), 1);
        combat::tick_emitters(&mut game);
        assert!(game.hit_emitters.is_empty(), "three bursts, then done");
    }

    #[test]
    fn a_player_who_uses_an_item_walks_at_a_fifth_of_the_speed() {
        let walk = |using: bool| {
            let mut game = game_on_floor_with(BlockState::AIR);
            // The simulation waits for the nine columns around the player.
            let world = game.world.as_mut().expect("a world");
            let payload = vec![0u8; umicraft_world::chunk::payload_len(0b1000, true, true)];
            for (x, z) in [
                (-1, -1),
                (-1, 0),
                (-1, 1),
                (0, -1),
                (0, 1),
                (1, -1),
                (1, 0),
                (1, 1),
            ] {
                world
                    .apply_chunk(
                        ChunkPos::new(x, z),
                        umicraft_world::ChunkPayload {
                            primary_bit_mask: 0b1000,
                            ground_up: true,
                            data: &payload,
                        },
                    )
                    .expect("an empty column");
            }
            game.playing = true;
            game.food = 10.0;
            game.input.forward = 1.0;
            game.input.use_item = using;
            game.windows.apply_set_slot(0, 36, wire_item(267, 0));
            game.pitch = -0.5;
            game.yaw = 0.0;
            let mut queue = CommandQueue::default();
            let (frames, _received) = mpsc::sync_channel(8);
            let (chunks, _chunk_received) = mpsc::sync_channel(8);
            for _ in 0..10 {
                tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
            }
            let position = game.state.as_ref().expect("placed").position();
            position.z - 0.5
        };
        let free = walk(false);
        let slowed = walk(true);
        assert!(slowed > 0.0, "the player still moves");
        assert!((slowed / free - 0.2).abs() < 0.03, "{slowed} vs {free}");
    }

    #[test]
    fn q_and_ctrl_q_send_the_drop_packets() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        for kind in [DropKind::One, DropKind::Stack] {
            let update = InputUpdate {
                drop_item: Some(kind),
                ..InputUpdate::default()
            };
            handle_input(&mut game, update, &mut queue);
        }
        let statuses: Vec<_> = commands(&queue)
            .iter()
            .filter_map(|command| match command {
                ClientCommand::PlayerDigging { status, .. } => Some(*status),
                _ => None,
            })
            .collect();
        assert_eq!(
            statuses,
            vec![DiggingStatus::DropItem, DiggingStatus::DropAllItems]
        );
        game.gamemode = GameMode::Spectator;
        let before = queue.ordered.len();
        let update = InputUpdate {
            drop_item: Some(DropKind::One),
            ..InputUpdate::default()
        };
        handle_input(&mut game, update, &mut queue);
        assert_eq!(queue.ordered.len(), before);
    }

    /// Looks straight down at the grass floor (id 2).
    #[test]
    fn the_middle_button_selects_the_hotbar_slot_of_the_block() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.pitch = std::f32::consts::FRAC_PI_2;
        game.windows.apply_set_slot(0, 38, wire_item(2, 0));
        let mut queue = CommandQueue::default();
        pick_block(&mut game, &mut queue);
        assert_eq!(game.selected_slot, 2);
        assert!(commands(&queue)
            .iter()
            .any(|command| matches!(command, ClientCommand::HeldItemChange { slot: 2 })));
    }

    #[test]
    fn a_creative_pick_fills_a_free_slot_and_tells_the_server() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.gamemode = GameMode::Creative;
        game.pitch = std::f32::consts::FRAC_PI_2;
        let mut queue = CommandQueue::default();
        pick_block(&mut game, &mut queue);
        let given = game.windows.selected_item().expect("an item was given");
        assert_eq!(given.item_id, 2);
        assert!(commands(&queue).iter().any(|command| matches!(
            command,
            ClientCommand::CreativeSlot { slot: 36, item } if item.item_id == 2
        )));
    }

    #[test]
    fn a_survival_pick_without_the_item_changes_nothing() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.pitch = std::f32::consts::FRAC_PI_2;
        let mut queue = CommandQueue::default();
        pick_block(&mut game, &mut queue);
        assert!(queue.ordered.is_empty());
    }

    /// Depth Strider on the worn boots reaches the physics.
    #[test]
    fn the_depth_strider_of_the_boots_reaches_the_player() {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut boots = wire_item(313, 0);
        // `ench` list of one compound: `id` = 8 (Depth Strider), `lvl` = 3.
        boots.nbt = Some(enchanted_nbt(8, 3));
        game.windows.apply_set_slot(0, 8, boots);
        apply_movement_stats(&mut game);
        assert_eq!(
            game.state.as_ref().expect("placed").player().depth_strider,
            3
        );
    }

    #[test]
    fn looking_at_the_floor_selects_the_floor_block() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.pitch = std::f32::consts::FRAC_PI_2;
        let boxes = game.selection_outline();
        assert_eq!(
            boxes,
            vec![OutlineBox {
                min: [0.0, 63.0, 0.0],
                max: [1.0, 64.0, 1.0],
            }]
        );
    }

    #[test]
    fn a_selection_is_offset_by_the_block_position_and_follows_its_shape() {
        // A stone slab (id 44) at (0, 64, 0) seen from beside it: it is
        // half a block high, so the outline must not be the full block.
        let mut game = game_on_floor_with(BlockState::new(44, 0));
        game.state = Some(GameState::new(DVec3::new(0.5, 64.0, 3.5)));
        game.pitch = 0.45;
        game.yaw = std::f32::consts::PI;
        let boxes = game.selection_outline();
        assert_eq!(boxes.len(), 1);
        assert_eq!(boxes[0].min, [0.0, 64.0, 0.0]);
        assert_eq!(boxes[0].max, [1.0, 64.5, 1.0]);
    }

    #[test]
    fn looking_at_the_sky_or_being_a_spectator_selects_nothing() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.pitch = -std::f32::consts::FRAC_PI_2;
        assert!(game.selection_outline().is_empty());
        game.pitch = std::f32::consts::FRAC_PI_2;
        game.gamemode = GameMode::Spectator;
        assert!(game.selection_outline().is_empty());
    }

    #[test]
    fn an_adventure_player_gets_no_selection_outline() {
        let mut game = game_on_floor_with(BlockState::AIR);
        game.pitch = std::f32::consts::FRAC_PI_2;
        assert!(!game.selection_outline().is_empty());
        game.gamemode = GameMode::Adventure;
        assert!(game.selection_outline().is_empty());
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

    /// NET-10: a BungeeCord or Velocity switch is a second `Join Game` on the
    /// same connection; the new server's world replaces the old one.
    #[test]
    fn a_second_join_game_replaces_the_world_of_the_previous_server() {
        use umicraft_net::play::JoinGameInfo;
        use umicraft_protocol::packets::play::clientbound::TimeUpdate;

        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        let join = |entity_id, dimension| JoinGameInfo {
            entity_id,
            gamemode: 0,
            dimension,
            difficulty: 0,
            max_players: 20,
            level_type: "default".to_string(),
            reduced_debug_info: false,
        };

        handle_net(
            &mut game,
            PlayEvent::JoinGame(join(7, 0)),
            &mut queue,
            &frames,
        );
        handle_net(
            &mut game,
            PlayEvent::TimeUpdate(TimeUpdate {
                world_age: 1,
                time_of_day: 9_000,
            }),
            &mut queue,
            &frames,
        );
        handle_net(
            &mut game,
            PlayEvent::JoinGame(join(99, -1)),
            &mut queue,
            &frames,
        );

        assert_eq!(game.entity_id, 99);
        assert_eq!(game.dimension, -1);
        assert_eq!(game.world_time, 0);
        assert_eq!(game.entities.len(), 0);
        assert!(game
            .world
            .as_ref()
            .is_some_and(|world| world.dimension() == -1));
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

    #[test]
    fn a_flat_world_is_known_from_the_join_and_the_respawn_that_changes_dimension() {
        use umicraft_net::play::JoinGameInfo;
        use umicraft_protocol::packets::play::clientbound::Respawn;

        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        let join = |level_type: &str| JoinGameInfo {
            entity_id: 1,
            gamemode: 0,
            dimension: 0,
            difficulty: 0,
            max_players: 20,
            level_type: level_type.to_string(),
            reduced_debug_info: false,
        };
        let respawn = |dimension, world_type: &str| Respawn {
            dimension,
            difficulty: 0,
            game_mode: 0,
            world_type: world_type.to_string(),
        };

        assert!(!game.flat_world, "a normal world until the server says");
        handle_net(
            &mut game,
            PlayEvent::JoinGame(join("flat")),
            &mut queue,
            &frames,
        );
        assert!(game.flat_world);
        // Names are compared without regard to case.
        handle_net(
            &mut game,
            PlayEvent::JoinGame(join("FLAT")),
            &mut queue,
            &frames,
        );
        assert!(game.flat_world);
        handle_net(
            &mut game,
            PlayEvent::JoinGame(join("default")),
            &mut queue,
            &frames,
        );
        assert!(!game.flat_world);

        // A respawn in the same dimension keeps the world, and its type.
        handle_net(
            &mut game,
            PlayEvent::JoinGame(join("flat")),
            &mut queue,
            &frames,
        );
        handle_net(
            &mut game,
            PlayEvent::Respawn(respawn(0, "default")),
            &mut queue,
            &frames,
        );
        assert!(game.flat_world, "the same world stays flat");
        // A new dimension is built with the type of the packet.
        handle_net(
            &mut game,
            PlayEvent::Respawn(respawn(-1, "default")),
            &mut queue,
            &frames,
        );
        assert!(!game.flat_world);
        handle_net(
            &mut game,
            PlayEvent::Respawn(respawn(0, "flat")),
            &mut queue,
            &frames,
        );
        assert!(game.flat_world);
    }

    #[test]
    fn joining_another_server_starts_without_the_old_clicks_and_item_use() {
        // A lobby compass: the right click is still pending and an item is in
        // use when the proxy sends the player to another server.
        use umicraft_net::play::JoinGameInfo;

        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        game.use_pressed = true;
        game.attack_pressed = true;
        let join = JoinGameInfo {
            entity_id: 1,
            gamemode: 0,
            dimension: 0,
            difficulty: 0,
            max_players: 20,
            level_type: "default".to_string(),
            reduced_debug_info: false,
        };
        handle_net(&mut game, PlayEvent::JoinGame(join), &mut queue, &frames);
        assert!(!game.use_pressed && !game.attack_pressed);
        assert!(!game.item_use.is_using());
        assert!(
            game.windows.view().container.is_none(),
            "no window stays open"
        );
    }

    #[test]
    fn a_formatted_chat_message_keeps_its_styles_and_arrival_time() {
        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        let before = Instant::now();
        handle_net(
            &mut game,
            PlayEvent::Chat(
                r#"{"text":"Hi ","color":"red","extra":[{"text":"there","bold":true}]}"#
                    .to_string(),
            ),
            &mut queue,
            &frames,
        );
        let hud = game.hud();
        assert_eq!(hud.chat.len(), 1);
        assert_eq!(hud.chat[0].plain(), "Hi there");
        assert_eq!(hud.chat[0].runs[0].color, [0xFF, 0x55, 0x55]);
        assert!(!hud.chat[0].runs[0].bold);
        assert!(hud.chat[0].runs[1].bold);
        assert!(hud.chat[0].received >= before);
    }

    #[test]
    fn an_empty_chat_component_is_a_blank_line_not_raw_json() {
        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        handle_net(
            &mut game,
            PlayEvent::Chat(r#"{"text":""}"#.to_string()),
            &mut queue,
            &frames,
        );
        assert_eq!(game.hud().chat[0].plain(), "");
    }

    #[test]
    fn the_chat_keeps_the_last_one_hundred_messages() {
        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        for number in 0..150 {
            handle_net(
                &mut game,
                PlayEvent::Chat(format!(r#"{{"text":"message {number}"}}"#)),
                &mut queue,
                &frames,
            );
        }
        let chat = game.hud().chat;
        assert_eq!(chat.len(), CHAT_HISTORY);
        assert_eq!(chat[0].plain(), "message 50");
        assert_eq!(chat[99].plain(), "message 149");
    }

    #[test]
    fn the_typed_chat_limit_counts_characters_not_bytes() {
        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        // 100 two-byte characters: 200 bytes, but within the 100-character limit.
        let message = "\u{e9}".repeat(100);
        handle_event(
            &mut game,
            GameEvent::Chat(message.clone()),
            &mut queue,
            &frames,
        );
        assert_eq!(queue.ordered.len(), 1, "a 100-character message is sent");
        handle_event(
            &mut game,
            GameEvent::Chat("\u{e9}".repeat(101)),
            &mut queue,
            &frames,
        );
        assert_eq!(queue.ordered.len(), 1, "101 characters is too long");
    }

    #[test]
    fn a_tab_request_is_queued_and_the_answer_is_published_with_a_new_serial() {
        use umicraft_protocol::packets::play::clientbound::TabComplete;

        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        handle_event(
            &mut game,
            GameEvent::TabComplete("/he".to_string()),
            &mut queue,
            &frames,
        );
        assert!(matches!(
            queue.ordered.front(),
            Some(ClientCommand::TabComplete { message }) if message == "/he"
        ));

        assert_eq!(game.hud().completions_serial, 0);
        handle_net(
            &mut game,
            PlayEvent::TabComplete(TabComplete {
                matches: vec!["/help".to_string()],
            }),
            &mut queue,
            &frames,
        );
        let hud = game.hud();
        assert_eq!(hud.completions, ["/help"]);
        assert_eq!(hud.completions_serial, 1);
    }

    #[test]
    fn the_chat_visibility_filters_incoming_chat_and_system_messages() {
        let mut game = Game::new();
        let say = |game: &mut Game, event: PlayEvent| {
            let (frames, _received) = mpsc::sync_channel(16);
            handle_net(game, event, &mut CommandQueue::default(), &frames);
        };
        // Commands only: the system message stays, the player message goes.
        game.chat_mode = 1;
        say(
            &mut game,
            PlayEvent::Chat(r#"{"text":"player"}"#.to_string()),
        );
        say(
            &mut game,
            PlayEvent::SystemChat(r#"{"text":"system"}"#.to_string()),
        );
        // Hidden: nothing is kept.
        game.chat_mode = 2;
        say(
            &mut game,
            PlayEvent::SystemChat(r#"{"text":"late"}"#.to_string()),
        );
        let plain: Vec<_> = game.hud().chat.iter().map(ChatLineView::plain).collect();
        assert_eq!(plain, ["system"]);
    }

    #[test]
    fn a_client_settings_event_updates_the_chat_visibility_filter() {
        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        handle_event(
            &mut game,
            GameEvent::ClientSettings {
                view_distance: 8,
                chat_mode: 2,
                chat_colors: true,
                skin_parts: 0x7F,
            },
            &mut queue,
            &frames,
        );
        assert_eq!(game.chat_mode, 2);
    }

    #[test]
    fn a_local_chat_line_is_never_parsed_as_json() {
        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        for text in ["null", "123", "Steve, Alex"] {
            handle_event(
                &mut game,
                GameEvent::LocalChat(text.to_string()),
                &mut queue,
                &frames,
            );
        }
        let plain: Vec<_> = game.hud().chat.iter().map(ChatLineView::plain).collect();
        assert_eq!(plain, ["null", "123", "Steve, Alex"]);
    }

    #[test]
    fn the_social_events_fill_the_hud() {
        use umicraft_protocol::packets::play::clientbound::{
            AddPlayerEntry, DisplayScoreboard, ObjectiveRenderType, PlayerListHeaderFooter,
            PlayerListItem, ScoreboardObjective, Title, UpdateScore,
        };

        let mut game = Game::new();
        game.own_name = "Steve".to_string();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        let mut send = |game: &mut Game, event| handle_net(game, event, &mut queue, &frames);

        send(
            &mut game,
            PlayEvent::ScoreboardObjective(ScoreboardObjective::Create {
                name: "kills".to_string(),
                value: "Kills".to_string(),
                render_type: ObjectiveRenderType::Integer,
            }),
        );
        send(
            &mut game,
            PlayEvent::DisplayScoreboard(DisplayScoreboard {
                position: 1,
                score_name: "kills".to_string(),
            }),
        );
        send(
            &mut game,
            PlayEvent::UpdateScore(UpdateScore::Change {
                name: "Alex".to_string(),
                objective: "kills".to_string(),
                value: 4,
            }),
        );
        send(
            &mut game,
            PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![AddPlayerEntry {
                uuid: u128::from_be_bytes([1; 16]),
                name: "Steve".to_string(),
                properties: vec![],
                game_mode: 0,
                ping: 37,
                display_name: None,
            }])),
        );
        send(
            &mut game,
            PlayEvent::PlayerListHeaderFooter(PlayerListHeaderFooter {
                header: r#"{"text":"Welcome"}"#.to_string(),
                footer: r#"{"text":""}"#.to_string(),
            }),
        );
        send(
            &mut game,
            PlayEvent::Title(Title::Title {
                json: r#"{"text":"Round 1"}"#.to_string(),
            }),
        );
        send(
            &mut game,
            PlayEvent::ActionBar(r#"{"text":"Go!"}"#.to_string()),
        );

        // The title fades in from nothing, so it needs a few ticks to show.
        for _ in 0..5 {
            game.title.tick();
        }
        let hud = game.hud();
        let sidebar = hud.sidebar.expect("the sidebar objective is displayed");
        assert_eq!(plain_text(&sidebar.title), "Kills");
        assert_eq!(sidebar.lines.len(), 1);
        assert_eq!(plain_text(&sidebar.lines[0].name), "Alex");
        assert_eq!(sidebar.lines[0].score, 4);

        let tab_list = hud.tab_list.expect("a player is listed");
        assert_eq!(
            tab_list.header.as_deref().map(plain_text).as_deref(),
            Some("Welcome")
        );
        assert_eq!(tab_list.footer, None, "an empty footer is absent");
        assert_eq!(tab_list.entries.len(), 1);
        assert_eq!(plain_text(&tab_list.entries[0].name), "Steve");
        assert_eq!(tab_list.entries[0].ping, 37);

        let title = hud.title.expect("a title is displayed");
        assert_eq!(plain_text(&title.title), "Round 1");
        assert!(title.alpha > 0.0);
        let action_bar = hud.action_bar.expect("an action bar is displayed");
        assert_eq!(plain_text(&action_bar.text), "Go!");
        assert!(action_bar.alpha > 0.0);
    }

    #[test]
    fn a_dimension_change_clears_the_scoreboard_but_keeps_the_tab_list() {
        use umicraft_protocol::packets::play::clientbound::{
            AddPlayerEntry, DisplayScoreboard, ObjectiveRenderType, PlayerListItem, Respawn,
            ScoreboardObjective,
        };

        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        handle_net(
            &mut game,
            PlayEvent::ScoreboardObjective(ScoreboardObjective::Create {
                name: "o".to_string(),
                value: "O".to_string(),
                render_type: ObjectiveRenderType::Integer,
            }),
            &mut queue,
            &frames,
        );
        handle_net(
            &mut game,
            PlayEvent::DisplayScoreboard(DisplayScoreboard {
                position: 1,
                score_name: "o".to_string(),
            }),
            &mut queue,
            &frames,
        );
        handle_net(
            &mut game,
            PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![AddPlayerEntry {
                uuid: u128::from_be_bytes([2; 16]),
                name: "Alex".to_string(),
                properties: vec![],
                game_mode: 0,
                ping: 5,
                display_name: None,
            }])),
            &mut queue,
            &frames,
        );
        assert!(game.hud().sidebar.is_some());

        handle_net(
            &mut game,
            PlayEvent::Respawn(Respawn {
                dimension: -1,
                difficulty: 0,
                game_mode: 0,
                world_type: "default".to_string(),
            }),
            &mut queue,
            &frames,
        );
        let hud = game.hud();
        assert!(hud.sidebar.is_none(), "a new world has a new scoreboard");
        assert!(
            hud.tab_list.is_some(),
            "the tab list belongs to the connection"
        );
    }

    #[test]
    fn boss_mob_spawn_and_metadata_updates_hud_boss_bar() {
        use umicraft_protocol::packets::play::clientbound::{
            DestroyEntities, EntityMetadata, SpawnMob,
        };
        use umicraft_protocol::packets::play::metadata::{Metadata, WatchedObject};

        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);

        // Spawn Ender Dragon (mob type 63) with full health (200.0)
        let mut metadata = Metadata::default();
        metadata.objects.push(WatchedObject {
            type_id: 3,
            index: 6,
            value: 200.0_f32.to_be_bytes().to_vec(),
        });
        handle_net(
            &mut game,
            PlayEvent::SpawnMob(SpawnMob {
                entity_id: 500,
                mob_type: 63,
                x: 0,
                y: 64 * 32,
                z: 0,
                yaw: 0,
                pitch: 0,
                head_yaw: 0,
                velocity_x: 0,
                velocity_y: 0,
                velocity_z: 0,
                metadata,
            }),
            &mut queue,
            &frames,
        );

        social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
        let hud = game.hud();
        let boss = hud.boss.expect("boss bar present after dragon spawn");
        assert_eq!(plain_text(&boss.name), "Ender Dragon");
        assert_eq!(boss.health, 1.0);

        // Update health via EntityMetadata to 100.0 (half health)
        let mut update_meta = Metadata::default();
        update_meta.objects.push(WatchedObject {
            type_id: 3,
            index: 6,
            value: 100.0_f32.to_be_bytes().to_vec(),
        });
        handle_net(
            &mut game,
            PlayEvent::EntityMetadata(EntityMetadata {
                entity_id: 500,
                metadata: update_meta,
            }),
            &mut queue,
            &frames,
        );

        social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
        let hud = game.hud();
        let boss = hud.boss.expect("boss bar present after damage");
        assert_eq!(boss.health, 0.5);

        // Destroy dragon
        handle_net(
            &mut game,
            PlayEvent::DestroyEntities(DestroyEntities {
                entity_ids: vec![500],
            }),
            &mut queue,
            &frames,
        );
        // `BossStatus.statusBarTime` is 100 when the dragon was last seen and
        // the tick of that same frame already took one off: 99 remain.
        for _ in 0..98 {
            social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
        }
        assert!(game.hud().boss.is_some(), "the bar outlives the dragon");
        social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
        assert!(
            game.hud().boss.is_none(),
            "boss bar disappears 100 ticks after the dragon"
        );
    }

    #[test]
    fn the_first_placement_publishes_the_placed_flag_at_once() {
        use umicraft_net::play::PlayerPlacement;

        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, received) = mpsc::sync_channel(16);
        let placement = PlayerPlacement {
            x: 0.5,
            y: 64.0,
            z: 0.5,
            yaw: 0.0,
            pitch: 0.0,
            raw: None,
        };
        handle_net(
            &mut game,
            PlayEvent::Placement(placement.clone()),
            &mut queue,
            &frames,
        );
        let statuses: Vec<bool> = received
            .try_iter()
            .filter_map(|event| match event {
                RenderEvent::Status(status) => Some(status.placed),
                _ => None,
            })
            .collect();
        assert_eq!(statuses, [true]);
        // A later correction does not publish again.
        handle_net(
            &mut game,
            PlayEvent::Placement(placement),
            &mut queue,
            &frames,
        );
        assert!(received
            .try_iter()
            .all(|event| !matches!(event, RenderEvent::Status(_))));
    }

    #[test]
    fn a_statistics_packet_is_forwarded_by_name() {
        use umicraft_protocol::packets::play::clientbound::Statistics;

        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, received) = mpsc::sync_channel(16);
        let packet = Statistics {
            entries: vec![("stat.jump".to_string(), 7), ("stat.deaths".to_string(), 1)],
        };
        handle_net(
            &mut game,
            PlayEvent::Statistics(packet),
            &mut queue,
            &frames,
        );
        let stats = received.try_iter().find_map(|event| match event {
            RenderEvent::Stats(stats) => Some(stats),
            _ => None,
        });
        assert_eq!(
            stats,
            Some(vec![
                ("stat.deaths".to_string(), 1),
                ("stat.jump".to_string(), 7)
            ])
        );
    }

    #[test]
    fn opening_the_statistics_asks_the_server() {
        let mut game = Game::new();
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(16);
        handle_event(&mut game, GameEvent::RequestStats, &mut queue, &frames);
        assert!(matches!(
            queue.ordered.front(),
            Some(ClientCommand::RequestStats)
        ));
    }
}
