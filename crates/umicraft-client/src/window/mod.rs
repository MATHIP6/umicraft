// SPDX-License-Identifier: MIT OR Apache-2.0

//! Windowed client: the winit event loop, the player camera, the chunk
//! upload and the HUD.
//!
//! The render thread owns the window, the GPU resources and the
//! [`WorldView`], which interpolates the camera and drives the mesh workers.
//! It never mutates the world and never blocks: the camera, the HUD and the
//! chunk snapshots arrive over the bounded channels of the game thread
//! (ADR-0005, ADR-0013).

use std::sync::Arc;
use std::time::Instant;

use umicraft_core::events::EventBus;
use umicraft_core::lang::Lang;
use umicraft_modhost::CameraChain;
use umicraft_render::{Camera, Overlay, Renderer};
use umicraft_ui::game_menu::{GameMenu, GameMenuAction};
use umicraft_ui::menus::PanoramaTextures;
use umicraft_ui::{debug_screen::DebugScreen, Ui};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

use crate::args::Args;
use crate::audio_bridge::AudioBridge;
use crate::debug::cpu_split::CpuSplit;
use crate::debug::mod_profile::ModProfiler;
use crate::discord::Presence;
use crate::error::ClientError;
use crate::game::Session;
use crate::hud::cache::HudCache;
use crate::hud::dials::DialAnimator;
use crate::input::ui::UiInputTranslator;
use crate::language;
use crate::mods::entity_facts::SnapshotBuilder;
use crate::mods::external::ExternalMods;
use crate::mods::runtime::ModsRuntime;
use crate::mods::zoom::Zoom;
use crate::options::{self, Options};
use crate::resources::entity_icons::EntityIcons;
use crate::scene::item_world::WorldItems;
use crate::screens::anvil_field::AnvilField;
use crate::screens::creative::input::CreativeInput;
use crate::screens::inventory::input::InventoryInput;
use crate::screens::menu_flow::{MenuCommand, MenuFlow};
use crate::screens::portrait::Spot;
use crate::screens::text::TextScreens;
use crate::screens::Screen;
use crate::world_view::WorldView;

mod state;
use state::{Assets, ChatUi, FrameClock, FrameScratch, InputTracking, OptionsMenu, PendingPresses};

mod binding_input;
mod chat_control;
mod creative_window;
mod death;
mod debug_chords;
mod font_sync;
mod handler;
mod hotbar_scroll;
mod inventory_control;
mod keyboard;
mod menu_screens;
mod mod_events;
mod mod_post;
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

/// The frame rate of a window that has lost the focus (or is minimized): the game
/// goes on, but it stops competing with the program in front for the GPU, the
/// CPU and the battery.
const BACKGROUND_FPS: u32 = 30;

/// Runs the windowed client until the user closes the window.
///
/// # Errors
/// Window/event-loop creation, network thread startup, or renderer
/// initialization failure. Runtime draw failures are surfaced the same way
/// after the event loop returns.
pub fn run(args: Args) -> Result<(), ClientError> {
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
    let mut app = App::new(args, options, options_path, lang)?;
    event_loop
        .run_app(&mut app)
        .map_err(|error| ClientError::Window(error.to_string()))?;
    if let Some(error) = app.exit_error {
        return Err(ClientError::Window(error));
    }
    Ok(())
}

struct App {
    args: Args,
    /// The Discord Rich Presence, kept in line with the screen every loop turn.
    discord: Presence,
    /// What the last frame gathered from the world, kept to reuse its buffers.
    frame: FrameScratch,
    /// Where the CPU time of a frame goes, for F3.
    cpu: CpuSplit,
    /// The zoom of the Zoom mod.
    zoom: Zoom,
    /// Whether the window has the focus; without it the frame rate is capped low.
    focused: bool,
    /// The built HUD, kept while it does not change.
    hud_cache: HudCache,
    /// The user's options: video, mouse, chat and the key bindings.
    options: Options,
    /// The language every UI text comes from (ADR-0039); also installed as
    /// the shared current language for the other threads.
    lang: Arc<Lang>,
    /// The options screen, where it was opened from and how its edits are saved.
    options_menu: OptionsMenu,
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
    /// What the keyboard and the mouse hold now, and the last input sent.
    input: InputTracking,
    /// The camera, clock, chunk meshing and HUD of the game; created with
    /// the renderer, once the terrain textures are bound (ADR-0012, ADR-0013).
    world: Option<WorldView>,
    /// The play session; `None` on the title screen and the server list
    /// (ADR-0044). Started by `connect`, dropped by `disconnect`.
    session: Option<Session>,
    /// The menu screens and their data: the server book, the pings, the
    /// dialogs (ADR-0044).
    menu: MenuFlow,
    /// The presses not handed to the game thread yet.
    presses: PendingPresses,
    /// When the death screen opened; `None` while the player lives.
    death_started: Option<Instant>,
    /// The screen that owns the keyboard and the mouse (ADR-0017). A server
    /// container shows the inventory screen without setting this field, see
    /// [`App::screen`].
    screen: Screen,
    /// The chat line: what is typed, the history, the scroll and the hover.
    chat: ChatUi,
    /// Turns the mouse and keys on the creative screen into game events.
    creative_input: CreativeInput,
    /// Whether a server container was displayed on the previous frame.
    container_was_open: bool,
    /// The immediate-mode context, owned by the render thread.
    ui: Ui,
    /// Whether the vanilla F3 text screen is shown (toggled by F3).
    show_f3: bool,
    /// Whether a chord was used since F3 went down; then its release leaves
    /// the text screen as it is.
    f3_chord_used: bool,
    /// Whether the entity hitboxes are drawn (F3 + B).
    show_hitboxes: bool,
    /// Whether the chunk borders are drawn (F3 + G).
    show_chunk_borders: bool,
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
    /// The fonts, atlases and textures loaded for the session.
    assets: Assets,
    /// The favicon revision of the menus the GUI atlas was last grown for
    /// (`u64::MAX` after the atlas was replaced).
    server_icons_revision: u64,
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
    /// The timing of the displayed frames.
    clock: FrameClock,
    /// The built-in mods (ADR-0065).
    mods: ModsRuntime,
    /// The external native mods and the crash sentinel; shut down on a clean exit.
    external_mods: Option<ExternalMods>,
    /// The entity snapshot the external mods read in the callbacks of this frame,
    /// rebuilt once per frame (ADR-0094).
    entity_snapshot: SnapshotBuilder,
    /// The per-mod lines of the F3 screen, recomputed once a second.
    mod_profiler: ModProfiler,
    /// The mod events of the render thread (connection and screens). Dropped
    /// before `external_mods` shuts down: its handlers call into the mods.
    render_bus: EventBus,
    /// The mods' `render_camera` layers, built once at start; `None` when no mod
    /// registered one (API 1.1).
    camera_layers: Option<CameraChain>,
    /// How far the mouse turned the view this frame, in Minecraft degrees (yaw,
    /// pitch), for the camera layers.
    frame_look: (f32, f32),
    /// Whether a camera layer captured the input in the last placed frame (API 1.3):
    /// the movement keys and the mouse look then turn the camera, not the player.
    camera_capture: bool,
    /// Whether the game thread was told that a camera layer moved the camera away
    /// from the player, so it draws the body (ADR-0096).
    camera_detached: bool,
    /// What the external mods' world layers drew this frame (API 1.4).
    mod_world: crate::mods::world_draw::ModWorldFrame,
    /// The threads still dropping stopped sessions; joined at exit before the
    /// mods shut down, because their game threads may still dispatch.
    releasing: Vec<std::thread::JoinHandle<()>>,
    /// Which post-process passes of the mods run this frame, kept for its buffer;
    /// without capacity when no mod has a pass (F3.7).
    post_frames: Vec<umicraft_render::PostPassFrame>,
    exit_error: Option<String>,
}
