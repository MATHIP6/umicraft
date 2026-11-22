// SPDX-License-Identifier: MIT OR Apache-2.0

//! The messages that cross the thread boundaries: what the front end and
//! the network send to the game thread, and what it publishes back.

use std::sync::Arc;
use std::time::Instant;

use umicraft_game::editors::book::BookPacket;
use umicraft_game::entity::EntityFrameData;
use umicraft_game::gamemode::GameMode;
use umicraft_game::inventory::WindowView;
use umicraft_game::piston::MovingBlocks;
use umicraft_game::server::block_entities::EntityTable;
use umicraft_game::server::block_entities::SignTable;
use umicraft_game::server::chest_lids::ChestLids;
use umicraft_game::server::maps::MapStore;
use umicraft_game::sound::cue::SoundCue;
use umicraft_game::sound::looping::LoopingSound;
use umicraft_game::view::Perspective;
use umicraft_game::RenderState;
use umicraft_mesh::ChunkSnapshot;
use umicraft_net::play::PlayEvent;
use umicraft_protocol::packets::play::serverbound::ResourcePackResult;
use umicraft_protocol::packets::play::ItemStack;
use umicraft_render::{BorderView, FogContext, HudState, OutlineBox};
use umicraft_ui::menus::ConnectStage;
use umicraft_world::{ChunkPos, LightLevel};

use super::look::LookSlot;
use crate::input::InputUpdate;
use crate::pack_offer::ServerPackRequest;
use crate::scene::cracks::Crack;
use crate::screens::connection_end::ConnectionEnd;
use crate::screens::creative::input::CreativeEvent;
use crate::screens::creative::view::CreativeView;
use crate::screens::inventory::input::{ContainerAction, InventoryAction};

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
    /// The player changed which particle categories may be created.
    ParticleCategories(umicraft_game::particles::ParticleFilter),
    /// The 1.7 animations the built-in mod selects (ADR-0079).
    LegacyAnimations(umicraft_game::legacy::LegacyAnimations),
    /// A line the client shows in its own chat (the completion list).
    LocalChat(String),
    /// F3 + D: empties the chat history.
    ClearChat,
    /// Request to re-mesh every loaded chunk column (e.g. after a resource pack reload).
    ReMeshAll,
    /// The client closes a container or the player inventory (E / Escape).
    CloseInventory,
    /// A slot interaction of the open inventory or container screen.
    Inventory(InventoryAction),
    /// A click on a button or text change of a container screen.
    Container(ContainerAction),
    /// The lines of the sign being edited changed. They show on the sign at
    /// once, as `GuiEditSign` edits the tile entity in place; nothing is sent.
    SignTyped {
        /// Block position of the sign.
        position: (i32, i32, i32),
        /// The four lines as chat-component JSON.
        lines: [String; 4],
    },
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
    /// The inventory key opened the player's inventory.
    InventoryOpened,
}

/// An angle of the view a teleport sets, in radians.
///
/// `Player Position and Look` can make the yaw and the pitch relative (flags
/// 0x08 and 0x10): vanilla adds them to the look the player has *now*
/// (`NetHandlerPlayClient.handlePlayerPosLook`). The window turns its view
/// with the mouse before the game thread hears of it, so a relative angle
/// must be added to the window's view too, not replaced by the game's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewAngle {
    /// The view takes this angle.
    To(f32),
    /// The view turns by this angle.
    By(f32),
}

impl ViewAngle {
    /// The angle after the teleport, from the angle the view has now.
    #[must_use]
    pub fn applied_to(self, now: f32) -> f32 {
        match self {
            Self::To(angle) => angle,
            Self::By(turn) => now + turn,
        }
    }
}

/// One simulation frame published to the render thread.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Active camera perspective for this frame.
    pub perspective: Perspective,
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
    /// How many server placements the game thread had applied when it built
    /// the frame. A count the render thread has not seen in a
    /// [`RenderEvent::Teleport`] means that event was dropped by the full
    /// queue, and the view takes the look of the body instead.
    pub placement: u32,
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
    /// The biome id at the player (`None` while the column is not loaded),
    /// which the custom sky layers filter on.
    pub sky_biome: Option<u8>,
    /// The medium at the eye and the effects that shape the fog (P7g).
    pub fog: FogContext,
    /// The world border of the dimension (P7g).
    pub border: BorderView,
    /// Rain, thunder, the lightning flash and the columns rain falls on (P7f).
    pub weather: crate::scene::weather::WeatherView,
    /// The live particles with the light where they stand (P8).
    pub particles: Arc<[crate::scene::particles::ParticleDraw]>,
    /// What the screen overlays need to decide what shows (P8-f).
    pub overlays: crate::scene::overlay_state::OverlayInputs,
    /// The world facts the F3 screen prints (P9-h).
    pub debug: Arc<crate::debug::world::DebugWorld>,
    /// The looping sounds to hear after this tick (rolling minecarts).
    pub loops: Arc<[LoopingSound]>,
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
    /// The local player teleported or was set back by the server (packet 0x08);
    /// the render thread immediately snaps both `current` and `previous` states
    /// without interpolation gliding.
    Teleport {
        /// Absolute target eye position in world space.
        position: glam::DVec3,
        /// The new yaw, or the turn a relative teleport adds to the look.
        yaw: ViewAngle,
        /// The new pitch, or the turn a relative teleport adds.
        pitch: ViewAngle,
        /// The placement count after this teleport (see [`Frame::placement`]).
        placement: u32,
    },
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
        /// A standing sign (with its post); a wall sign otherwise.
        standing: bool,
    },
    /// The book screen of a book and quill or a written book.
    Book {
        /// The held stack.
        stack: ItemStack,
        /// The name of the local player, the author of a signed book.
        author: String,
    },
}

/// A chunk whose geometry changed or that was unloaded.
#[derive(Debug)]
pub struct ChunkEvent {
    /// The chunk column.
    pub pos: ChunkPos,
    /// The immutable snapshot the mesher needs, or `None` if the column was unloaded.
    pub snapshot: Option<ChunkSnapshot>,
}
