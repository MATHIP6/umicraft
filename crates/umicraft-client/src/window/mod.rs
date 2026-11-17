// SPDX-License-Identifier: MIT OR Apache-2.0

//! Windowed client: the winit event loop, the player camera, the chunk
//! upload and the HUD.
//!
//! The render thread owns the window, the GPU resources and the
//! [`WorldView`], which interpolates the camera and drives the mesh workers.
//! It never mutates the world and never blocks: the camera, the HUD and the
//! chunk snapshots arrive over the bounded channels of the game thread
//! (ADR-0005, ADR-0013).

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use umicraft_core::lang::Lang;
use umicraft_game::chat::{ChatInput, SentHistory};
use umicraft_game::item_use::DropKind;
use umicraft_render::{Camera, HudState, InputHit, Overlay, Renderer};
use umicraft_ui::game_menu::{GameMenu, GameMenuAction};
use umicraft_ui::menus::PanoramaTextures;
use umicraft_ui::{debug_screen::DebugScreen, FontAtlas, Ui};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::KeyCode;
use winit::window::Window;

use crate::anvil_field::AnvilField;
use crate::args::Args;
use crate::audio_bridge::AudioBridge;
use crate::cpu_split::CpuSplit;
use crate::creative_input::CreativeInput;
use crate::dials::DialAnimator;
use crate::entity_icons::EntityIcons;
use crate::entity_render::EntityTextures;
use crate::error::ClientError;
use crate::fps_meter::FpsMeter;
use crate::game::Session;
use crate::hud_cache::HudCache;
use crate::input::InputState;
use crate::inventory_input::InventoryInput;
use crate::item_icons::ItemResources;
use crate::item_world::{ItemLayers, WorldItems};
use crate::language;
use crate::map_render::MapTextures;
use crate::menu_flow::{MenuCommand, MenuFlow};
use crate::metrics::FrameMetricsWindow;
use crate::mods_runtime::ModsRuntime;
use crate::nametag_draw::PlacedTag;
use crate::options::{self, Options};
use crate::options_effects::VideoApplier;
use crate::options_screen::{OptionsScreen, Outcome as OptionsOutcome};
use crate::portrait::Spot;
use crate::screen::Screen;
use crate::text_screens::TextScreens;
use crate::texture_animator::TextureAnimator;
use crate::ui_font::LoadedFont;
use crate::ui_input::UiInputTranslator;
use crate::world_view::WorldView;
use crate::zoom::Zoom;
use startup::AsyncLoadedAssets;

mod binding_input;
mod chat_control;
mod creative_window;
mod death;
mod handler;
mod hotbar_scroll;
mod inventory_control;
mod keyboard;
mod menu_screens;
mod mods_menu;
mod options_control;
mod player_input;
mod redraw;
mod screen_state;
mod server_pack;
mod server_pack_control;
mod session_control;
mod startup;
#[cfg(test)]
mod tests;
mod ui_frame;

/// The render distance `--view-distance` has when it is not given. Any other
/// value on the command line overrides the saved option.
const CLI_DEFAULT_VIEW_DISTANCE: u8 = 8;

/// How long the chat caret stays on, then off, in milliseconds (6 ticks).
const CARET_BLINK_MS: u128 = 300;
const FRAME_HISTORY: usize = 120;

/// The frame rate of a window that has lost the focus (or is minimized): the game
/// goes on, but it stops competing with the program in front for the GPU, the
/// CPU and the battery.
const BACKGROUND_FPS: u32 = 30;

/// The look speed at the default sensitivity, in radians per pixel; the
/// sensitivity option scales it.
///
/// In Minecraft Java 1.8.9 (`EntityRenderer.java:1095-1098` and `Entity.java:383-384`):
/// `f = sensitivity * 0.6 + 0.2`
/// `f1 = f^3 * 8`
/// `deltaYaw = deltaX * f1 * 0.15°`
/// At default sensitivity (0.5), f1 = 1.0, and 1 pixel of mouse motion equals 0.15 degrees.
/// Converted to radians: 0.15° = 0.15 * π / 180 rad.
pub(crate) const MOUSE_SENSITIVITY: f32 = 0.15_f32.to_radians();

/// Computes the relative yaw and pitch deltas in radians from a raw mouse movement,
/// applying the vanilla 1.8.9 sensitivity formula and mouse inversion.
pub(crate) fn compute_mouse_look_delta(
    delta: (f64, f64),
    controls: &crate::options::ControlOptions,
    zoom_look_factor: f32,
) -> (f32, f32) {
    let speed = MOUSE_SENSITIVITY * controls.look_factor() * zoom_look_factor;
    let vertical = if controls.invert_mouse { -1.0 } else { 1.0 };
    (delta.0 as f32 * speed, delta.1 as f32 * speed * vertical)
}

/// Runs the windowed client until the user closes the window.
///
/// # Errors
/// Window/event-loop creation, network thread startup, or renderer
/// initialization failure. Runtime draw failures are surfaced the same way
/// after the event loop returns.
pub fn run(args: Args) -> Result<(), ClientError> {
    umicraft_core::boot_log!("window_run_started");
    let options_path = options::default_path();
    let mut options = options_path
        .as_deref()
        .map(Options::load)
        .unwrap_or_default();
    // A render distance typed on the command line wins over the saved one.
    if args.view_distance != CLI_DEFAULT_VIEW_DISTANCE {
        options.video.render_distance = args.view_distance;
        options.clamp();
    }
    let lang = Arc::new(language::load(&options.language.code));
    language::install(Arc::clone(&lang));
    let event_loop = EventLoop::new().map_err(|error| ClientError::Window(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    umicraft_core::boot_log!("event_loop_created");
    let mut app = App::new(args, options, options_path, lang)?;
    umicraft_core::boot_log!("app_initialized");
    umicraft_core::boot_log!("event_loop_run_app_starting");
    event_loop
        .run_app(&mut app)
        .map_err(|error| ClientError::Window(error.to_string()))?;
    umicraft_core::boot_log!("event_loop_run_app_exited");
    if let Some(error) = app.exit_error {
        return Err(ClientError::Window(error));
    }
    Ok(())
}

struct App {
    args: Args,
    /// The name tags of the frame, drawn by the renderer in the world pass.
    nametags: Vec<umicraft_render::Nametag>,
    /// Reusable screen-placed tag buffer so `place_tags_into` does not allocate per frame.
    placed_tags: Vec<PlacedTag>,
    /// Reusable dynamic moving-blocks / flower-pot mesh buffer across frames.
    moving_blocks_mesh: umicraft_mesh::Mesh,
    /// The pose of the hand in the last frame, to tell the motion blur when it moves.
    last_hand: Option<umicraft_game::hand::HandPose>,
    /// Where the CPU time of a frame goes, for F3.
    cpu: CpuSplit,
    /// The zoom of the Zoom mod.
    zoom: Zoom,
    /// Whether the window has the focus; without it the frame rate is capped low.
    focused: bool,
    /// The built HUD, kept while it does not change.
    hud_cache: HudCache,
    /// Double-buffered HUD state on the render thread: synced from `WorldView`
    /// only when `hud_revision` or `options.chat` changes, avoiding per-frame
    /// heap clones of chat lines, tab list entries, and scoreboard rows.
    frame_hud: HudState,
    frame_hud_revision: u64,
    frame_hud_chat: Option<options::ChatOptions>,
    /// The entities of this frame, interpolated once: the tab list, the name tags and
    /// the scene all read them (perf 2, B4).
    frame_entities: Vec<umicraft_game::entity::EntityRenderSnapshot>,
    /// The particle quads of the frame, rebuilt in place.
    particle_sprites: Vec<umicraft_render::ParticleSprite>,
    /// The user's options: video, mouse, chat and the key bindings.
    options: Options,
    /// The language every UI text comes from (ADR-0039); also installed as
    /// the shared current language for the other threads.
    lang: Arc<Lang>,
    /// Where the options are saved, when the platform has a data folder.
    options_path: Option<std::path::PathBuf>,
    /// The options as they are on disk and known to the server, to tell what
    /// changed when the options screen closes.
    saved_options: Options,
    /// The options screen, shown while [`Screen::Options`] is current.
    options_screen: OptionsScreen,
    /// What the options screen did in the last UI frame, applied by `redraw`.
    options_outcome: OptionsOutcome,
    /// Applies the video options that act on the window and the renderer.
    video_applier: VideoApplier,
    /// The audio thread and its handle; silent with `--no-audio` or without
    /// downloaded assets (P11-E).
    audio: AudioBridge,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    /// The uploaded title panorama, when the user's assets provide it.
    panorama: Option<PanoramaTextures>,
    camera: Camera,
    overlay: Overlay,
    show_overlay: bool,
    mouse_captured: bool,
    keys: HashSet<KeyCode>,
    /// The camera, clock, chunk meshing and HUD of the game; created with
    /// the renderer, once the terrain textures are bound (ADR-0012, ADR-0013).
    world: Option<WorldView>,
    /// The play session; `None` on the title screen and the server list
    /// (ADR-0044). Started by `connect`, dropped by `disconnect`.
    session: Option<Session>,
    /// The menu screens and their data: the server book, the pings, the
    /// dialogs (ADR-0044).
    menu: MenuFlow,
    /// The screen the options were opened from: the title screen or the game
    /// menu. Escape and Done go back there.
    options_origin: Screen,
    last_input: InputState,
    last_input_time: Instant,
    look: (f32, f32),
    select_slot: Option<u8>,
    /// The digit key presses not sent yet (MCP `KeyBinding.pressTime`).
    hotbar_keys: [u8; 9],
    break_block: bool,
    /// The mouse buttons that are down, by `Binding::Mouse` index.
    buttons: HashSet<u8>,
    /// When the death screen opened; `None` while the player lives.
    death_started: Option<Instant>,
    /// The player pressed "Respawn" and waits for the server.
    respawn_requested: bool,
    /// The use button went down since the last input update.
    place_block: bool,
    /// The screen that owns the keyboard and the mouse (ADR-0017). A server
    /// container shows the inventory screen without setting this field, see
    /// [`App::screen`].
    screen: Screen,
    /// The line the player is typing while [`Screen::Chat`] is current.
    chat_input: ChatInput,
    /// The messages the player sent, for the up and down keys.
    sent_messages: SentHistory,
    /// How many rows the open chat is scrolled up.
    chat_scroll: usize,
    /// When the chat opened; the caret blinks from here.
    chat_opened: Instant,
    /// The completion answer the chat already handled.
    seen_completions: u64,
    /// Whether a Control key is held (word movement in the chat).
    ctrl_held: bool,
    /// Whether Shift is held: a chat click then inserts instead of acting.
    shift_held: bool,
    /// The pointer in physical pixels, for the chat hover and click.
    pointer: (f32, f32),
    /// The chat run under the pointer in the last frame (click and hover).
    chat_hover: Option<umicraft_render::ChatRunView>,
    /// Where the pointer is on the chat input line, as of the last frame.
    chat_line: Option<InputHit>,
    /// The pick block binding went down since the last input update.
    pick_block: bool,
    /// The drop key was pressed since the last input update.
    drop_item: Option<DropKind>,
    /// The perspective key was pressed since the last input update.
    toggle_perspective: bool,
    /// The screenshot binding was pressed; the next frame is saved.
    screenshot_requested: bool,
    /// Turns the mouse and keys on the creative screen into game events.
    creative_input: CreativeInput,
    /// Whether a server container was displayed on the previous frame.
    container_was_open: bool,
    /// The immediate-mode context, owned by the render thread.
    ui: Ui,
    /// Whether the vanilla F3 text screen is shown (toggled by F3).
    show_f3: bool,
    /// The acceptance screen shown while [`Screen::Debug`] is current.
    debug_screen: DebugScreen,
    /// The menu shown while [`Screen::GameMenu`] is current.
    game_menu: GameMenu,
    /// The entry the player clicked in the last UI frame, applied by `redraw`.
    menu_action: Option<GameMenuAction>,
    /// What the player did on a menu screen in the last UI frame, applied by
    /// `redraw`.
    menu_command: Option<MenuCommand>,
    /// The winit events waiting for the next UI frame.
    ui_input: UiInputTranslator,
    /// Turns the mouse and keys on the inventory screen into slot clicks.
    inventory_input: InventoryInput,
    /// The font loaded at startup, before the renderer exists.
    font: LoadedFont,
    /// Background asset loader channel receiver.
    asset_receiver: Option<std::sync::mpsc::Receiver<AsyncLoadedAssets>>,
    /// The atlas the renderer accepted; the UI metrics must match it.
    ui_atlas: Option<FontAtlas>,
    /// The brand atlas the renderer accepted; `None` keeps the classic
    /// title screen (ADR-0056).
    brand_atlas: Option<FontAtlas>,
    /// The GUI texture atlas for inventory/container backgrounds.
    gui_atlas: Option<umicraft_assets::GuiAtlas>,
    /// The favicon revision of the menus the GUI atlas was last grown for
    /// (`u64::MAX` after the atlas was replaced).
    server_icons_revision: u64,
    /// The baked item models, once the official assets are bound.
    items: Option<std::sync::Arc<ItemResources>>,
    /// The compass and clock frames of the bound block array.
    dials: DialAnimator,
    /// The dropped-item geometry of the frame, kept for its buffers.
    world_items: WorldItems,
    /// The skull and banner icons drawn into the icon atlas.
    entity_icons: EntityIcons,
    /// Where the inventory screen put the player model in the last frame.
    portrait: Option<Spot>,
    /// The name field of the anvil screen.
    anvil_field: AnvilField,
    /// The open book or sign screen.
    text_screens: TextScreens,
    /// The resource pack the connected server offered (NET-11).
    server_pack: server_pack_control::ServerPackRuntime,
    /// The first-person held item geometry of the frame.
    hand_layers: ItemLayers,
    /// Steps the water, lava and fire textures once per game tick.
    texture_animator: TextureAnimator,
    /// Entity textures bound for GPU rendering.
    entity_textures: Option<EntityTextures>,
    /// The textures of filled maps, created with the entity textures.
    map_textures: Option<MapTextures>,
    last_frame: Instant,
    /// When the frame-rate cap wants the next frame to start.
    next_frame: Instant,
    /// Origin of the clock that scrolls the enchantment glint.
    started: Instant,
    frame_times: VecDeque<f32>,
    frame_ms: f32,
    fps: f32,
    /// Refreshes `fps` once per second for the F3 screen.
    fps_meter: FpsMeter,
    /// Rolling frametime percentiles, 1%/0.1% low FPS, and per-frame allocations.
    metrics_window: FrameMetricsWindow,
    /// Heap allocation snapshot taken at the start of the previous frame.
    last_alloc_snapshot: umicraft_core::profiling::AllocSnapshot,
    /// The built-in mods (ADR-0065).
    mods: ModsRuntime,
    exit_error: Option<String>,
}
