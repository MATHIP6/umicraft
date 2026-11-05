// SPDX-License-Identifier: MIT OR Apache-2.0

//! Windowed client: the winit event loop, the player camera, the chunk
//! upload and the HUD.
//!
//! The render thread owns the window, the GPU resources and the
//! [`WorldView`], which interpolates the camera and drives the mesh workers.
//! It never mutates the world and never blocks: the camera, the HUD and the
//! chunk snapshots arrive over the bounded channels of the game thread
//! (ADR-0005, ADR-0013).

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use umicraft_core::lang::Lang;

use umicraft_game::chat::{ChatInput, SentHistory};
use umicraft_game::inventory::HOTBAR_START;
use umicraft_game::item_use::DropKind;
use umicraft_render::{
    chat_run_at, draw_hud_gui, gui_scale, Camera, ChatInputView, HudState, NoIcons, Overlay,
    RenderEnvironment, Renderer, ScreenOverlays, SkyState,
};
use umicraft_ui::game_menu::{GameMenu, GameMenuAction};
use umicraft_ui::menus::{ConnectStage, PanoramaTextures};
use umicraft_ui::widgets::{death_screen, DeathAction, BUTTON_DELAY_TICKS};
use umicraft_ui::{debug_screen::DebugScreen, FontAtlas, Rect, SlotItem, Ui, UiOutput};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{
    DeviceEvent, ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

use self::chat_control::CHAT_WHEEL_ROWS;
use self::menu_screens::VERSION_LINE;
use self::session_control::lifecycle_step;
use crate::anvil_field::AnvilField;
use crate::args::Args;
use crate::creative_input::CreativeInput;
use crate::creative_layout::CreativeLayout;
use crate::creative_screen::{draw_creative_overlay, CreativeScene};
use crate::dials::{DialAnimator, DialInputs};
use crate::entity_render::EntityTextures;
use crate::error::ClientError;
use crate::game::{GameEvent, Session};
use crate::input::{InputState, InputUpdate};
use crate::inventory_input::InventoryInput;
use crate::language;
use crate::map_render::{map_id_of, MapTextures};
use crate::menu_flow::{MenuCommand, MenuContext, MenuFlow, MenuFrame};
use crate::nametag_draw::{draw_tags, place_tags};
use crate::text_screens::TextScreens;

use crate::achievements_view::java_string_hash;
use crate::block_entity_render::{mesh_block_entities, BlockEntityScene};
use crate::entity_icons::EntityIcons;
use crate::inventory_screen::{draw_inventory_overlay, ScreenFacts};
use crate::item_icons::{slot_item, IconSource, ItemResources, ItemView};
use crate::item_world::{entity_items, push_held_item, ItemLayers, WorldItems};
use crate::mesher::default_worker_count;
use crate::options::{self, Action, Options};
use crate::options_effects::{pack_paths, packs_folder, VideoApplier};
use crate::options_screen::{OptionsScreen, Outcome as OptionsOutcome};
use crate::panorama_faces::load_panorama_faces;
use crate::portrait::{turned_towards, Spot};
use crate::screen::Screen;
use crate::servers::{Pinger, ServerBook};
use crate::sky_textures::{load_border_texture, load_sky_textures};
use crate::terrain_textures::TerrainTextures;
use crate::texture_animator::TextureAnimator;
use crate::ui_font::{self, LoadedFont};
use crate::ui_input::UiInputTranslator;
use crate::world_view::WorldView;

mod chat_control;
mod creative_window;
mod inventory_control;
mod menu_screens;
mod options_control;
mod session_control;

/// The look speed at the default sensitivity, in radians per pixel; the
/// sensitivity option scales it.
const MOUSE_SENSITIVITY: f32 = 0.0022;
/// The render distance `--view-distance` has when it is not given. Any other
/// value on the command line overrides the saved option.
const CLI_DEFAULT_VIEW_DISTANCE: u8 = 8;
/// How long the chat caret stays on, then off, in milliseconds (6 ticks).
const CARET_BLINK_MS: u128 = 300;
const FRAME_HISTORY: usize = 120;

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
    look: (f32, f32),
    select_slot: Option<u8>,
    break_block: bool,
    /// The attack button is down.
    attack_held: bool,
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
    /// The use (right) button is down.
    use_held: bool,
    /// The pick block (middle) button went down since the last input update.
    pick_block: bool,
    /// The drop key was pressed since the last input update.
    drop_item: Option<DropKind>,
    /// The perspective key was pressed since the last input update.
    toggle_perspective: bool,
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
    /// The terrain textures loaded at start-up, until the renderer binds them.
    terrain: Option<TerrainTextures>,
    /// The atlas the renderer accepted; the UI metrics must match it.
    ui_atlas: Option<FontAtlas>,
    /// The GUI texture atlas for inventory/container backgrounds.
    gui_atlas: Option<umicraft_assets::GuiAtlas>,
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
    frame_times: Vec<f32>,
    frame_ms: f32,
    fps: f32,
    exit_error: Option<String>,
}

impl App {
    fn new(
        args: Args,
        options: Options,
        options_path: Option<std::path::PathBuf>,
        lang: Arc<Lang>,
    ) -> Result<Self, ClientError> {
        // Assets are loaded once, before any worker or GPU resource exists.
        // A missing/corrupt installation picks the procedural set.
        let terrain = Some(TerrainTextures::load_with_packs(&pack_paths(
            &args.resource_packs,
            &options,
            packs_folder().as_deref(),
        )));
        let font = ui_font::load();
        let args_has_server = args.server().is_some();
        let last_server = options.multiplayer.last_server.clone();
        let ui_viewport = Rect::new(0.0, 0.0, args.width as f32, args.height as f32);
        Ok(Self {
            args,
            saved_options: options.clone(),
            lang,
            options,
            options_path,
            options_screen: OptionsScreen::new(),
            options_outcome: OptionsOutcome::default(),
            video_applier: VideoApplier::default(),
            window: None,
            renderer: None,
            panorama: None,
            camera: Camera::default(),
            overlay: Overlay::new(1280, 720),
            show_overlay: true,
            mouse_captured: false,
            keys: HashSet::new(),
            world: None,
            session: None,
            menu: new_menu(last_server),
            options_origin: Screen::GameMenu,
            last_input: InputState::default(),
            look: (0.0, 0.0),
            select_slot: None,
            break_block: false,
            attack_held: false,
            death_started: None,
            respawn_requested: false,
            place_block: false,
            // With a server on the command line the window connects at once
            // (see `resumed`); otherwise it opens on the title screen.
            screen: if args_has_server {
                Screen::Connecting
            } else {
                Screen::Title
            },
            chat_input: ChatInput::new(""),
            sent_messages: SentHistory::new(),
            chat_scroll: 0,
            chat_opened: Instant::now(),
            seen_completions: 0,
            ctrl_held: false,
            shift_held: false,
            pointer: (0.0, 0.0),
            chat_hover: None,
            use_held: false,
            pick_block: false,
            drop_item: None,
            toggle_perspective: false,
            creative_input: CreativeInput::new(),
            container_was_open: false,
            ui: Ui::new(ui_viewport),
            show_f3: false,
            debug_screen: DebugScreen::new(),
            game_menu: GameMenu::new(),
            menu_action: None,
            menu_command: None,
            ui_input: UiInputTranslator::new(),
            inventory_input: InventoryInput::new(),
            font,
            terrain,
            ui_atlas: None,
            gui_atlas: None,
            items: None,
            dials: DialAnimator::default(),
            world_items: WorldItems::default(),
            entity_icons: EntityIcons::default(),
            portrait: None,
            anvil_field: AnvilField::default(),
            text_screens: TextScreens::default(),
            hand_layers: ItemLayers::default(),
            texture_animator: TextureAnimator::default(),
            entity_textures: None,
            map_textures: None,
            last_frame: Instant::now(),
            next_frame: Instant::now(),
            started: Instant::now(),
            frame_times: Vec::with_capacity(FRAME_HISTORY),
            frame_ms: 0.0,
            fps: 0.0,
            exit_error: None,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.resize(width, height);
        }
        self.overlay.resize(width, height);
        self.camera.set_aspect(width, height);
    }

    fn handle_key(&mut self, event_loop: &ActiveEventLoop, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        // F4 is the Umicraft debug-screen toggle (F3 is the vanilla text
        // screen, F2 stays free for the screenshot). It is handled before the
        // UI translation so it can never be typed into the screen, and it is
        // ignored while the key repeats so one press toggles exactly once.
        // Releases are swallowed as well, so no F4 event ever reaches the UI.
        if code == KeyCode::F4 {
            // The debug screen shows a game; menus have none.
            if self.session.is_some() && is_debug_toggle(code, event.state, event.repeat) {
                self.set_screen(self.screen().after_debug_toggle());
            }
            return;
        }
        match self.screen() {
            Screen::Debug => {
                self.ui_input.key(code, event.state, event.text.as_deref());
                return;
            }
            Screen::Chat => {
                self.handle_chat_key(event);
                return;
            }
            _ if self.death_screen_visible() => {
                // MCP GuiGameOver.keyTyped does nothing: Escape does not close it.
                return;
            }
            Screen::Creative => {
                self.handle_creative_key(&event);
                return;
            }
            Screen::Inventory => {
                self.handle_inventory_key(event_loop, event);
                return;
            }
            Screen::Sign | Screen::Book => {
                self.handle_text_screen_key(&event);
                return;
            }
            Screen::GameMenu => {
                self.handle_menu_key(event);
                return;
            }
            Screen::Options => {
                self.handle_options_key(event);
                return;
            }
            Screen::Title
            | Screen::ServerList
            | Screen::ServerEdit
            | Screen::ConfirmDelete
            | Screen::Connecting
            | Screen::Disconnected
            | Screen::Stats
            | Screen::Achievements => {
                self.handle_menu_screen_key(&event);
                return;
            }
            Screen::None => {}
        }
        match event.state {
            ElementState::Pressed => {
                if !event.repeat {
                    match code {
                        KeyCode::Escape => {
                            self.set_screen(Screen::None.after_escape(self.options_origin))
                        }
                        KeyCode::F1 => self.show_overlay = !self.show_overlay,
                        KeyCode::F3 => self.show_f3 = !self.show_f3,
                        KeyCode::F5 => self.toggle_perspective = true,
                        // F3 + T reloads the resource packs, whatever the
                        // chat key is bound to.
                        KeyCode::KeyT if self.keys.contains(&KeyCode::F3) => {
                            self.reload_resources();
                        }
                        KeyCode::KeyQ if self.mouse_captured => {
                            self.drop_item = Some(self.drop_kind());
                        }
                        _ => self.press_bound_key(code),
                    }
                }
                self.keys.insert(code);
            }
            ElementState::Released => {
                self.keys.remove(&code);
            }
        }
    }

    /// Whether the key bound to `action`, or its left/right twin, is held.
    fn held(&self, action: Action) -> bool {
        let key = self.options.keys.get(action);
        self.keys.contains(&key) || modifier_twin(key).is_some_and(|twin| self.keys.contains(&twin))
    }

    /// Runs the actions bound to a key that was just pressed.
    fn press_bound_key(&mut self, code: KeyCode) {
        let actions: Vec<Action> = self.options.keys.actions_for(code).collect();
        for action in actions {
            // An action that opened a screen takes the key; the others wait.
            if self.screen != Screen::None {
                break;
            }
            match action {
                Action::Chat => self.open_chat(""),
                Action::Command => self.open_chat("/"),
                Action::Inventory => self.open_inventory(),
                other => {
                    if let Some(slot) = other.hotbar_slot() {
                        self.select_slot = Some(slot);
                    }
                }
            }
        }
    }

    /// What the drop key drops: the whole stack with Ctrl held
    /// (MCP Minecraft.runTick line 2084, GuiScreen.isCtrlKeyDown).
    fn drop_kind(&self) -> DropKind {
        if self.keys.contains(&KeyCode::ControlLeft) || self.keys.contains(&KeyCode::ControlRight) {
            DropKind::Stack
        } else {
            DropKind::One
        }
    }

    /// The screen that owns the input right now.
    ///
    /// A container the server opened is shown by the inventory screen even
    /// though the player did not press the inventory key, so it is derived
    /// from the world view when no screen is set.
    fn screen(&self) -> Screen {
        if self.screen == Screen::None && self.container_is_open() {
            Screen::Inventory
        } else {
            self.screen
        }
    }

    /// Switches to `screen`.
    ///
    /// Every transition drops the pending input so no held key, look delta or
    /// click leaks across screens (a key released while another screen owned
    /// the keyboard would otherwise stay "held"). A screen that blocks the
    /// game also releases the cursor grab. The game and network threads keep
    /// running; the screens only read the render-side snapshot.
    fn set_screen(&mut self, screen: Screen) {
        if screen == self.screen {
            return;
        }
        let was_blocking = self.screen.blocks_game_input();
        self.screen = screen;
        self.menu.enter(screen);
        self.ui_input.clear();
        self.chat_scroll = 0;
        self.keys.clear();
        self.select_slot = None;
        self.break_block = false;
        self.place_block = false;
        self.pick_block = false;
        self.drop_item = None;
        self.look = (0.0, 0.0);
        if screen.blocks_game_input() {
            self.capture_mouse(false);
        } else if was_blocking && !self.container_is_open() && !self.death_screen_visible() {
            // MCP `displayGuiScreen(null)` grabs the mouse again, so closing
            // the chat hides the cursor without a click.
            self.capture_mouse(true);
        }
    }

    /// Handles a key press while the game menu is open.
    fn handle_menu_key(&mut self, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        if code == KeyCode::Escape {
            if event.state == ElementState::Pressed && !event.repeat {
                self.set_screen(Screen::GameMenu.after_escape(self.options_origin));
            }
            return;
        }
        self.ui_input.key(code, event.state, event.text.as_deref());
    }

    /// Whether a game is shown: a session runs and no full-screen menu
    /// (connecting, disconnected) covers it.
    fn world_visible(&self) -> bool {
        self.session.is_some() && !self.screen().is_menu()
    }

    /// Whether a server-opened container is currently displayed.
    fn container_is_open(&self) -> bool {
        self.world
            .as_ref()
            .is_some_and(|w| w.window_view().container.is_some())
    }

    /// True while the local player is dead (MCP `getHealth() <= 0`).
    fn player_dead(&self) -> bool {
        self.world
            .as_ref()
            .is_some_and(|world| world.hud().health <= 0.0)
    }

    /// True while the death screen is shown: the player is dead and has not
    /// asked to respawn yet.
    fn death_screen_visible(&self) -> bool {
        self.death_started.is_some() && !self.respawn_requested
    }

    /// Opens or closes the death screen as the health changes
    /// (MCP `Minecraft.runTick`: a dead player gets a `GuiGameOver`).
    fn update_death_screen(&mut self, now: Instant) {
        if !self.player_dead() {
            self.death_started = None;
            self.respawn_requested = false;
            return;
        }
        if self.death_started.is_none() {
            self.death_started = Some(now);
            self.set_screen(Screen::None);
            self.attack_held = false;
            self.capture_mouse(false);
        }
    }

    /// Whether the death-screen buttons accept clicks yet.
    fn death_buttons_enabled(&self, now: Instant) -> bool {
        self.death_started.is_some_and(|started| {
            now.duration_since(started) >= crate::game::TICK * BUTTON_DELAY_TICKS
        })
    }

    /// Asks the server to respawn and gives the game back to the player.
    fn respawn(&mut self) {
        // MCP `GuiGameOver.actionPerformed` (button 0): `respawnPlayer`, then
        // the screen closes at once.
        self.send_event(GameEvent::Respawn);
        self.respawn_requested = true;
        self.ui_input.clear();
        self.capture_mouse(true);
    }

    /// Frees the mouse while a server container is displayed and takes it
    /// back when it closes (MCP `displayGuiScreen` ungrabs / grabs the mouse).
    fn update_container_cursor(&mut self) {
        let open = self.container_is_open();
        if open != self.container_was_open {
            self.container_was_open = open;
            self.ui_input.clear();
            self.inventory_input.reset();
            self.anvil_field = AnvilField::default();
            let free = open || self.screen().blocks_game_input() || self.death_screen_visible();
            self.capture_mouse(!free);
        }
    }

    fn capture_mouse(&mut self, capture: bool) {
        self.mouse_captured = capture;
        if !capture {
            self.attack_held = false;
            self.use_held = false;
        }
        if let Some(window) = &self.window {
            if capture {
                // Locked is preferred (infinite relative motion); fall back to
                // Confined on backends where locking is unsupported.
                let _ = window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
            } else {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
            }
            window.set_cursor_visible(!capture);
        }
    }

    /// The key state of the window.
    fn key_state(&self) -> InputState {
        if self.screen().blocks_game_input() || self.death_screen_visible() {
            // While a screen is open the movement keys do not steer the player.
            return InputState::default();
        }
        let mut state = InputState::default();
        if self.held(Action::Forward) {
            state.forward += 1.0;
        }
        if self.held(Action::Back) {
            state.forward -= 1.0;
        }
        if self.held(Action::Left) {
            state.strafe += 1.0;
        }
        if self.held(Action::Right) {
            state.strafe -= 1.0;
        }
        state.jump = self.held(Action::Jump);
        state.sneak = self.held(Action::Sneak);
        state.sprint = self.held(Action::Sprint);
        state.attack = self.attack_held;
        state.use_item = self.use_held;
        state
    }

    /// Sends the input of the window to the game thread.
    ///
    /// The send never blocks: when the inbox is full the input is repeated
    /// on the next frame.
    fn publish_input(&mut self) {
        if self.session.is_none() {
            return;
        }
        let state = self.key_state();
        let changed = state != self.last_input
            || self.look != (0.0, 0.0)
            || self.select_slot.is_some()
            || self.break_block
            || self.place_block
            || self.pick_block
            || self.drop_item.is_some()
            || self.toggle_perspective;
        if !changed {
            return;
        }
        let update = InputUpdate {
            state,
            look_yaw: self.look.0,
            look_pitch: self.look.1,
            select_slot: self.select_slot,
            break_block: self.break_block,
            place_block: self.place_block,
            pick_block: self.pick_block,
            drop_item: self.drop_item,
            toggle_perspective: self.toggle_perspective,
        };
        if self.send_event(GameEvent::Input(update)) {
            self.last_input = state;
            self.look = (0.0, 0.0);
            self.select_slot = None;
            self.break_block = false;
            self.place_block = false;
            self.pick_block = false;
            self.drop_item = None;
            self.toggle_perspective = false;
        }
    }

    /// Moves the world view forward, follows the player with the camera and
    /// applies the session lifecycle: leaving the connecting screen, showing
    /// the disconnected screen (ADR-0044).
    fn sync_world(&mut self, now: Instant) {
        let (Some(session), Some(world), Some(renderer)) = (
            self.session.as_ref(),
            self.world.as_mut(),
            self.renderer.as_mut(),
        ) else {
            return;
        };
        world.sync(session.frames(), session.chunks(), renderer);
        world.update_camera(&mut self.camera, now, self.options.video.view_bobbing);
        let step = lifecycle_step(self.screen, world.placed(), world.ended());
        self.apply_lifecycle(step);
    }

    /// Builds the frame-time graph; the rest of the HUD is drawn by the UI
    /// pass (`build_ui_frame`).
    fn build_overlay(&mut self) {
        let (width, height) = self
            .renderer
            .as_ref()
            .map_or((self.args.width, self.args.height), Renderer::size);
        self.overlay.clear();
        if !self.show_overlay || !self.world_visible() {
            return;
        }
        let graph_width = 240.0_f32.min(width as f32 - 20.0);
        let graph_height = 40.0_f32.min(height as f32 - 70.0);
        if graph_width > 0.0 && graph_height > 0.0 {
            self.overlay.frame_graph(
                width as f32 - graph_width - 10.0,
                height as f32 - graph_height - 10.0,
                graph_width,
                graph_height,
                &self.frame_times,
            );
        }
    }

    /// Builds one UI frame from the render-side snapshot: the textured HUD and
    /// the hotbar items while a world is shown, and the modal screen on top
    /// when one is open. Returns `None` when there is nothing to draw.
    ///
    /// The frame is built on the render thread during redraw, after the HUD
    /// overlay, so the UI never touches the game thread or its state. The
    /// caller recycles the returned output after the renderer consumed the
    /// draw list.
    fn build_ui_frame(&mut self, width: u32, height: u32) -> Option<UiOutput> {
        self.portrait = None;
        // Only the title screen sets it again below; every other frame,
        // in particular a session's, draws the world.
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_panorama(None);
        }
        self.open_text_screen();
        let mut text_done = None;
        let mut respawn_clicked = false;
        let inventory_facts = self.inventory_facts();
        let atlas = self.ui_atlas.as_ref()?;
        let sheets = crate::official::hud_sheets(self.gui_atlas.as_ref());
        let screen = self.screen();
        // Without the GUI atlas the hotbar and the status rows are not drawn
        // (no sprite sheets); chat, titles, the sidebar and F3 still are.
        let in_game = self.world_visible();
        if !in_game && !screen.uses_ui() {
            return None;
        }

        let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
        let input = self.ui_input.take();
        self.ui.begin_frame(viewport, input);
        self.ui.set_widgets_sheet(sheets.widgets);
        self.ui
            .set_menu_art(crate::official::menu_art(self.gui_atlas.as_ref()));

        if in_game {
            let mut game_hud = self
                .world
                .as_ref()
                .map(|world| world.hud().clone())
                .unwrap_or_default();
            self.options.chat.apply(&mut game_hud);
            let chatting = self.screen == Screen::Chat;
            let debug_f3 = self.show_f3.then(|| {
                crate::debug_f3_view::build_live(
                    self.renderer.as_ref(),
                    self.world.as_ref(),
                    &crate::debug_f3_view::Live {
                        fps: self.fps,
                        video: &self.options.video,
                        display: (width, height),
                        camera: &self.camera,
                    },
                )
            });
            let items = ItemView::new(self.items.as_deref(), self.dials.frames());
            let hud = HudState {
                hotbar_items: self.world.as_ref().map_or_else(Default::default, |world| {
                    hotbar_slot_items(&world.window_view().player_slots, items)
                }),
                debug_f3,
                chat_open: chatting,
                chat_scroll: self.chat_scroll,
                chat_input: chatting.then(|| ChatInputView {
                    text: self.chat_input.text(),
                    caret: self.chat_input.caret(),
                    caret_visible: (self.chat_opened.elapsed().as_millis() / CARET_BLINK_MS)
                        .is_multiple_of(2),
                }),
                now: Some(Instant::now()),
                show_tab_list: self.screen() == Screen::None && self.held(Action::PlayerList),
                ..game_hud
            };
            draw_hud_gui(self.ui.draw(), &hud, (width, height), atlas, &sheets);
            self.chat_hover = chatting
                .then(|| chat_run_at(&hud, (width, height), atlas, self.pointer))
                .flatten();
            let rows = self
                .chat_hover
                .as_ref()
                .and_then(|run| run.hover.as_ref())
                .map(|hover| crate::chat::hover_rows(hover, false))
                .unwrap_or_default();
            let pointer = [self.pointer.0, self.pointer.1];
            let scale = gui_scale(width, height) as f32;
            umicraft_ui::widgets::tooltip::tooltip(
                &mut self.ui,
                atlas,
                viewport,
                scale,
                pointer,
                &rows,
            );
        }
        if let Some(world) = self.world.as_ref().filter(|_| in_game) {
            if !matches!(screen, Screen::Inventory | Screen::Creative) {
                let entities = world.entities(Instant::now());
                let tags = place_tags(&entities, &self.camera, width as f32, height as f32);
                draw_tags(&mut self.ui, atlas, &tags);
            }
        }

        if self.death_screen_visible() {
            let score = self.world.as_ref().map_or(0, |world| world.hud().score);
            let enabled = self.death_buttons_enabled(Instant::now());
            respawn_clicked =
                death_screen(&mut self.ui, atlas, viewport, score, enabled, &self.lang)
                    == DeathAction::Respawn;
        } else {
            match screen {
                Screen::Debug => self.debug_screen.show(&mut self.ui, atlas),
                Screen::GameMenu => {
                    self.menu_action = self.game_menu.show(&mut self.ui, atlas, &self.lang)
                }
                Screen::Options => {
                    self.options_outcome = self.options_screen.show(
                        &mut self.ui,
                        atlas,
                        &mut self.options,
                        &self.lang,
                    );
                }
                Screen::Creative => {
                    if let Some((world, creative)) = self
                        .world
                        .as_ref()
                        .and_then(|world| Some((world, world.creative_view()?)))
                    {
                        let scene = CreativeScene {
                            window: world.window_view(),
                            creative,
                            gui_atlas: self.gui_atlas.as_ref(),
                            items: ItemView::new(self.items.as_deref(), self.dials.frames()),
                            // Vanilla toggles every 6 ticks (cursorCounter / 6 % 2).
                            caret_visible: (self.started.elapsed().as_millis() / 300)
                                .is_multiple_of(2),
                        };
                        let layout = CreativeLayout::new(width, height);
                        self.portrait =
                            draw_creative_overlay(&mut self.ui, atlas, viewport, &layout, &scene);
                    }
                }
                Screen::Inventory => {
                    let wv = self.world.as_ref().map(|w| w.window_view().clone());
                    if let Some(wv) = wv {
                        let facts = ScreenFacts {
                            drag: self.inventory_drag_preview(&wv),
                            ..inventory_facts
                        };
                        self.portrait = draw_inventory_overlay(
                            &mut self.ui,
                            atlas,
                            viewport,
                            &wv,
                            self.gui_atlas.as_ref(),
                            ItemView::new(self.items.as_deref(), self.dials.frames()),
                            &facts,
                        );
                    }
                }
                Screen::Sign | Screen::Book => {
                    let caret =
                        (self.started.elapsed().as_millis() / CARET_BLINK_MS).is_multiple_of(2);
                    let done = self.text_screens.draw(
                        &mut self.ui,
                        atlas,
                        viewport,
                        self.gui_atlas.as_ref(),
                        caret,
                        &self.lang,
                    );
                    text_done = done;
                }
                Screen::Title
                | Screen::ServerList
                | Screen::ServerEdit
                | Screen::ConfirmDelete
                | Screen::Connecting
                | Screen::Disconnected
                | Screen::Stats
                | Screen::Achievements => {
                    let stats = self.world.as_ref().map_or(&[][..], WorldView::stats);
                    let inventory_key =
                        options::key_label(self.options.keys.get(Action::Inventory));
                    let context = MenuContext {
                        stats_revision: self.world.as_ref().map_or(0, WorldView::stats_revision),
                        stats_loading: self.world.as_ref().is_some_and(WorldView::stats_loading),
                        items: ItemView::new(self.items.as_deref(), self.dials.frames()),
                        inventory_key: &inventory_key,
                        player_hash: java_string_hash(&self.args.name),
                        version: VERSION_LINE,
                        connect_stage: self
                            .world
                            .as_ref()
                            .map_or(ConnectStage::Connecting, WorldView::connect_stage),
                        stats,
                        panorama: self.panorama,
                    };
                    let shown = self
                        .menu
                        .show(screen, &mut self.ui, atlas, &self.lang, &context);
                    let MenuFrame { command, panorama } = shown;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_panorama(panorama);
                    }
                    self.menu_command = command;
                }
                Screen::None | Screen::Chat => {}
            }
        }
        let items = ItemView::new(self.items.as_deref(), self.dials.frames());
        menu_screens::draw_achievement_toast(
            &mut self.ui,
            atlas,
            self.world.as_ref(),
            (&self.lang, items),
        );
        let output = self.ui.end_frame();
        if respawn_clicked {
            self.respawn();
        }
        if let Some(done) = text_done {
            self.finish_text_screen(done);
        }
        Some(output)
    }

    /// Steps the compass and the clock and writes their new frames into the
    /// block array.
    fn advance_dials(&mut self, elapsed: Duration, environment: RenderEnvironment) {
        let inputs = self.world.as_ref().map(|world| {
            let (_, spawn) = world.compass_world();
            DialInputs {
                surface: environment.dimension == 0,
                position: (
                    f64::from(self.camera.position.x),
                    f64::from(self.camera.position.z),
                ),
                // The camera yaw is the server yaw turned by half a circle.
                yaw_degrees: f64::from((self.camera.yaw - std::f32::consts::PI).to_degrees()),
                spawn: spawn.map(|(x, _, z)| (f64::from(x), f64::from(z))),
                celestial_angle: f64::from(SkyState::at(environment).celestial_angle),
            }
        });
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        self.dials
            .advance(elapsed, inputs.as_ref(), |layer, pixels| {
                renderer.write_block_layer(layer, pixels);
            });
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let elapsed = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        let frame_ms = elapsed * 1000.0;
        self.frame_ms = if self.frame_ms == 0.0 {
            frame_ms
        } else {
            self.frame_ms * 0.9 + frame_ms * 0.1
        };
        let instant_fps = 1.0 / elapsed.max(1e-6);
        self.fps = if self.fps == 0.0 {
            instant_fps
        } else {
            self.fps * 0.9 + instant_fps * 0.1
        };
        self.frame_times.push(frame_ms);
        if self.frame_times.len() > FRAME_HISTORY {
            self.frame_times.remove(0);
        }

        self.camera.set_fov_degrees(self.options.video.fov);
        self.advance_menus(Duration::from_secs_f32(elapsed));
        self.sync_world(now);
        self.process_completions();
        self.update_death_screen(now);
        self.update_container_cursor();
        self.publish_input();
        let mut environment = self
            .world
            .as_ref()
            .map(|world| world.environment(now, self.options.video.render_distance))
            .unwrap_or_default();
        environment.gamma = self.options.video.gamma;
        self.build_overlay();

        if self.exit_error.is_some() {
            // The session ended (the server disconnected): leave the loop.
            event_loop.exit();
            return;
        }

        let (width, height) = self
            .renderer
            .as_ref()
            .map_or((self.args.width, self.args.height), Renderer::size);
        // The dials first, so the icon keys of this frame name the frames the
        // block array shows.
        self.advance_dials(Duration::from_secs_f32(elapsed), environment);
        let ui_frame = self.build_ui_frame(width, height);
        if let Some(action) = self.menu_action.take() {
            self.apply_game_menu_action(action);
        }
        if let Some(command) = self.menu_command.take() {
            self.apply_menu_command(command, event_loop);
        }
        let outcome = std::mem::take(&mut self.options_outcome);
        self.apply_options_outcome(outcome);
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        self.video_applier.apply(
            &self.options.video,
            self.window.as_deref(),
            renderer,
            self.entity_textures.as_ref(),
        );
        // One environment per frame: the sky and the fog read the same clock
        // and interpolation fraction as the camera.
        renderer.set_environment(environment);
        renderer.set_cloud_mode(self.options.video.cloud_mode());
        // 20 client ticks per second drive the cloud drift.
        renderer.set_cloud_ticks((now - self.started).as_secs_f64() * 20.0);
        if let Some(world) = self.world.as_ref() {
            renderer.set_precipitation(world.precipitation(), self.options.video.fancy_graphics);
            renderer.set_particles(&world.particle_sprites(environment.partial_tick));
        }
        renderer.set_screen_overlays(
            self.world
                .as_mut()
                .map_or_else(ScreenOverlays::default, |world| {
                    world.screen_overlays(now, self.options.video.fancy_graphics)
                }),
        );
        renderer.set_world_border(
            self.world.as_ref().and_then(WorldView::border),
            (now - self.started).as_secs_f64(),
        );
        renderer.set_glint_time((now - self.started).as_secs_f32());
        self.texture_animator
            .advance(Duration::from_secs_f32(elapsed), |layer, pixels| {
                renderer.write_block_layer(layer, pixels);
            });
        renderer.set_selection_outline(self.world.as_ref().map_or(&[], WorldView::selection));
        if let Some((vertices, indices)) = self.world.as_mut().and_then(WorldView::take_crack_mesh)
        {
            renderer.set_block_cracks(&vertices, &indices);
        }

        if let Some(world) = self.world.as_ref() {
            renderer.set_moving_blocks(&world.moving_blocks_mesh(environment.partial_tick));
        }

        if let (Some(entity_textures), Some(world)) =
            (self.entity_textures.as_ref(), self.world.as_ref())
        {
            let entities = world.entities(now);
            let hand = world.hand_pose(now);
            entity_textures.set_view_yaw(self.camera.yaw);
            entity_textures.set_view_pitch(self.camera.pitch);
            entity_textures.set_camera_position(self.camera.position);
            entity_textures.set_viewer_spectator(
                umicraft_game::gamemode::GameMode::from_wire(world.status().gamemode)
                    == umicraft_game::gamemode::GameMode::Spectator,
            );
            let held_map = held_stack(world)
                .filter(|_| hand.visible)
                .and_then(|stack| map_id_of(stack.item_id, stack.damage));
            let framed = self.map_textures.as_mut().map_or_else(Vec::new, |maps| {
                maps.frame_batches(&entities, world.maps(), renderer)
            });
            let mut block_entities = mesh_block_entities(
                world.block_entities(),
                &BlockEntityScene {
                    lids: world.chest_lids(),
                    signs: world.signs(),
                    data: world.block_entity_data(),
                    skins: entity_textures,
                    seconds: (now - self.started).as_secs_f32(),
                    font: self.ui_atlas.as_ref(),
                    partial_tick: environment.partial_tick,
                    camera: self.camera.position,
                },
            );
            let mut hand_item = None;
            if let Some(items) = self.items.as_deref() {
                entity_items::push(&mut block_entities, &entities, items, entity_textures);
                hand_item = held_stack(world)
                    .filter(|_| hand.visible)
                    .and_then(|stack| {
                        entity_items::hand_item(
                            &mut block_entities,
                            (stack.item_id, stack.damage, stack.nbt.as_deref()),
                            &hand,
                            (self.camera.view_bob(), world.hand_light()),
                            items,
                            entity_textures,
                        )
                    });
            }
            entity_textures.set_block_entities(block_entities);
            entity_textures.mesh_and_upload_with(&entities, &framed, renderer);
            match (held_map, self.map_textures.as_mut()) {
                (Some(map_id), Some(maps)) => {
                    let bob = self.camera.view_bob();
                    let held = maps.hand_batches(
                        world.maps(),
                        map_id,
                        (&hand, bob, world.hand_light()),
                        renderer,
                    );
                    entity_textures.upload_hand_map(&hand, bob, &held, renderer);
                }
                _ => match &hand_item {
                    Some((slot, vertices)) => entity_textures.upload_hand_item(
                        &hand,
                        self.camera.view_bob(),
                        (*slot, vertices),
                        renderer,
                    ),
                    None => entity_textures.upload_hand(&hand, self.camera.view_bob(), renderer),
                },
            }
            if let Some(items) = self.items.as_deref() {
                self.world_items
                    .set_view(self.camera.yaw, self.camera.pitch);
                let dropped = self.world_items.build(&entities, items);
                renderer.set_world_items(&dropped.items);
                renderer.set_world_glint(&dropped.glint);
                self.hand_layers.clear();
                if let Some(stack) = held_stack(world).filter(|stack| {
                    hand.visible && map_id_of(stack.item_id, stack.damage).is_none()
                }) {
                    let nbt = stack.nbt.as_deref();
                    push_held_item(
                        &mut self.hand_layers,
                        (stack.item_id, stack.damage, nbt),
                        &hand,
                        (self.camera.view_bob(), world.hand_light()),
                        items,
                    );
                }
                renderer.set_hand_items(&self.hand_layers.items);
                renderer.set_hand_glint(&self.hand_layers.glint);
            }
        }

        if let (Some(spot), Some(textures), Some(body)) = (
            self.portrait,
            self.entity_textures.as_ref(),
            self.world.as_ref().and_then(|world| world.local_body()),
        ) {
            let mouse = spot.mouse_offset(self.pointer);
            textures.render_portrait(
                &turned_towards(body, mouse),
                spot.placement.matrix(mouse),
                renderer,
            );
        }
        if let (Some(frame), Some(items), Some(textures)) = (
            ui_frame.as_ref(),
            self.items.as_deref(),
            self.entity_textures.as_ref(),
        ) {
            self.entity_icons
                .prepare(&frame.draw_list, items, textures, renderer);
        }
        let rendered = match (ui_frame.as_ref(), self.items.as_deref()) {
            (Some(frame), Some(items)) => renderer.render_with_ui(
                &self.camera,
                &self.overlay,
                &frame.draw_list,
                &mut IconSource(items),
            ),
            (Some(frame), None) => {
                renderer.render_with_ui(&self.camera, &self.overlay, &frame.draw_list, &mut NoIcons)
            }
            (None, _) => renderer.render(&self.camera, &self.overlay),
        };
        if let Some(frame) = ui_frame {
            self.ui.recycle_output(frame);
        }
        if let Err(error) = rendered {
            self.exit_error = Some(error.to_string());
            event_loop.exit();
            return;
        }
        // The frame-rate cap. The target moves on by one exact budget per frame,
        // so the wake-up delay of a sleep is not lost on every frame; the last
        // millisecond is spun because a sleep overshoots by about that much.
        if self.options.video.max_fps < options::UNLIMITED_FPS {
            let budget = Duration::from_secs_f32(1.0 / self.options.video.max_fps as f32);
            self.next_frame += budget;
            let arrived = Instant::now();
            if self.next_frame < arrived {
                // Behind schedule (a slow frame, a changed cap): do not catch up.
                self.next_frame = arrived;
            } else {
                let spin = Duration::from_millis(1);
                if let Some(rest) = (self.next_frame - arrived).checked_sub(spin) {
                    std::thread::sleep(rest);
                }
                while Instant::now() < self.next_frame {
                    std::hint::spin_loop();
                }
            }
        } else {
            self.next_frame = Instant::now();
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Starts the thread that pings the saved servers. Without it the list still
/// works and every row stays "pinging".
fn start_pinger() -> Option<Pinger> {
    match Pinger::new() {
        Ok(pinger) => Some(pinger),
        Err(error) => {
            tracing::warn!(%error, "server pings are off");
            None
        }
    }
}

/// A number that differs from start to start, for the title splash.
fn menu_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// True when this key event must toggle the debug screen.
///
/// Only a fresh F4 press toggles: releases and auto-repeats are ignored so a
/// held key cannot flicker the screen.
fn is_debug_toggle(code: KeyCode, state: ElementState, repeat: bool) -> bool {
    code == KeyCode::F4 && state == ElementState::Pressed && !repeat
}

/// Uploads the GUI atlas to the renderer, if available.
fn upload_gui_atlas(renderer: &mut Renderer, gui_atlas: Option<&umicraft_assets::GuiAtlas>) {
    if let Some(atlas) = gui_atlas {
        if !atlas.is_empty() {
            renderer.set_gui_atlas(atlas.width(), atlas.height(), atlas.pixels());
            tracing::info!(
                regions = atlas.regions().len(),
                "{}x{} GUI texture atlas uploaded",
                atlas.width(),
                atlas.height()
            );
        }
    }
}

/// Uploads the sun, moon and End sky pictures of the verified jar.
fn upload_sky_textures(renderer: &mut Renderer) {
    let jar = crate::official::find_verified_jar().ok();
    renderer.set_sky_textures(&load_sky_textures(jar.as_ref()));
    if let Some(forcefield) = load_border_texture(jar.as_ref()) {
        renderer.set_border_texture(&forcefield);
    }
}

/// Uploads the six title panorama faces of the verified jar; `None` keeps the
/// dirt background (no jar, or a face that does not decode).
fn upload_panorama(renderer: &mut Renderer) -> Option<PanoramaTextures> {
    let jar = crate::official::find_verified_jar().ok();
    renderer.upload_panorama(&load_panorama_faces(jar.as_ref())?)
}

/// Uploads the enchantment glint texture to the renderer, if available.
fn upload_glint_texture(renderer: &mut Renderer, glint: Option<&umicraft_assets::GlintTexture>) {
    if let Some(glint) = glint {
        renderer.set_glint_texture(glint.width, glint.height, &glint.pixels);
    }
}

/// The stack in the selected hotbar slot of the local player.
fn held_stack(world: &WorldView) -> Option<&umicraft_protocol::packets::play::ItemStack> {
    let selected = usize::from(world.hud().selected_slot).min(8);
    world.window_view().player_slots[HOTBAR_START + selected].as_ref()
}

/// The widget data of the nine hotbar slots, for the textured HUD.
fn hotbar_slot_items(
    player_slots: &[Option<umicraft_protocol::packets::play::ItemStack>; 45],
    items: ItemView<'_>,
) -> [Option<SlotItem>; 9] {
    std::array::from_fn(|index| {
        player_slots[HOTBAR_START + index]
            .as_ref()
            .map(|stack| slot_item(items, stack))
    })
}

/// The key on the other side of the keyboard that counts as the same
/// modifier: holding either Shift, Control or Alt triggers the binding.
fn modifier_twin(key: KeyCode) -> Option<KeyCode> {
    match key {
        KeyCode::ShiftLeft => Some(KeyCode::ShiftRight),
        KeyCode::ShiftRight => Some(KeyCode::ShiftLeft),
        KeyCode::ControlLeft => Some(KeyCode::ControlRight),
        KeyCode::ControlRight => Some(KeyCode::ControlLeft),
        KeyCode::AltLeft => Some(KeyCode::AltRight),
        KeyCode::AltRight => Some(KeyCode::AltLeft),
        _ => None,
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Umicraft M3")
            .with_inner_size(LogicalSize::new(
                f64::from(self.args.width),
                f64::from(self.args.height),
            ));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.exit_error = Some(format!("create window: {error}"));
                event_loop.exit();
                return;
            }
        };
        let surface_size = window.inner_size();
        self.resize(surface_size.width, surface_size.height);

        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(
                Box::new(event_loop.owned_display_handle()),
            ));
        let mut renderer = match Renderer::new_windowed(instance, window.clone()) {
            Ok(renderer) => renderer,
            Err(error) => {
                self.exit_error = Some(error.to_string());
                event_loop.exit();
                return;
            }
        };
        // Upload the atlas once, before the first UI frame. A rejected atlas
        // falls back to the built-in font; only a rejected fallback is fatal.
        match ui_font::install(&mut renderer, &self.font.atlas) {
            Ok(installed) => self.ui_atlas = Some(installed.atlas),
            Err(error) => {
                self.exit_error = Some(ClientError::FontAtlas(error).to_string());
                event_loop.exit();
                return;
            }
        }
        // The mesh workers only exist once the terrain textures are bound,
        // so they never write layers of another array (ADR-0012).
        let terrain = self
            .terrain
            .take()
            .unwrap_or(TerrainTextures::Procedural)
            .install(&mut renderer);
        upload_gui_atlas(&mut renderer, terrain.gui_atlas());
        upload_glint_texture(&mut renderer, terrain.glint_texture());
        upload_sky_textures(&mut renderer);
        self.panorama = upload_panorama(&mut renderer);
        self.gui_atlas = terrain.gui_atlas().cloned();
        self.items = terrain.items();
        self.dials = DialAnimator::new(terrain.dials());
        self.texture_animator = TextureAnimator::new(terrain.animations().to_vec());
        match WorldView::new(&terrain, default_worker_count()) {
            Ok(world) => self.world = Some(world),
            Err(error) => {
                self.exit_error = Some(error.to_string());
                event_loop.exit();
                return;
            }
        }
        self.entity_textures = Some(EntityTextures::load_with_font(
            &renderer,
            self.ui_atlas.as_ref(),
        ));
        self.map_textures = Some(MapTextures::load(&renderer));
        self.renderer = Some(renderer);
        self.window = Some(window.clone());
        self.connect_from_arguments();
        event_loop.listen_device_events(DeviceEvents::Always);
        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.save_options();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(event_loop, event),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.ctrl_held = modifiers.state().control_key();
                self.shift_held = modifiers.state().shift_key();
                self.ui_input.modifiers(modifiers.state());
                let state = modifiers.state();
                self.inventory_input
                    .set_modifiers(state.shift_key(), state.control_key());
                self.creative_input
                    .set_modifiers(state.shift_key(), state.control_key());
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Keep the pointer current even while the UI is closed: F4 can
                // open the screen before the next mouse movement, and the first
                // click must use the physical cursor, not the initial origin.
                self.pointer = (position.x as f32, position.y as f32);
                self.ui_input.pointer_moved(position);
                self.inventory_pointer(position);
                self.creative_pointer(position);
                if !self.screen().uses_ui() && !self.death_screen_visible() {
                    self.ui_input.clear();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let screen = self.screen();
                if screen.uses_ui() || self.death_screen_visible() {
                    self.ui_input.pointer_button(state, button);
                    self.inventory_button(state, button);
                    self.creative_button(state, button);
                } else if screen.blocks_game_input() {
                    if screen == Screen::Chat
                        && button == MouseButton::Left
                        && state == ElementState::Pressed
                    {
                        self.chat_click();
                    }
                } else {
                    match button {
                        MouseButton::Left => {
                            self.attack_held =
                                state == ElementState::Pressed && self.mouse_captured;
                            if state == ElementState::Pressed {
                                if self.mouse_captured {
                                    self.break_block = true;
                                } else {
                                    self.capture_mouse(true);
                                }
                            }
                        }
                        MouseButton::Right => {
                            self.use_held = state == ElementState::Pressed && self.mouse_captured;
                            if self.use_held {
                                self.place_block = true;
                            }
                        }
                        MouseButton::Middle
                            if state == ElementState::Pressed && self.mouse_captured =>
                        {
                            self.pick_block = true;
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.screen().uses_ui() || self.death_screen_visible() {
                    self.ui_input.wheel(delta);
                    self.creative_wheel(delta);
                } else if self.screen == Screen::Chat {
                    // A notch scrolls seven rows, one with Shift held
                    // (GuiScreen.handleMouseInput in the reference).
                    let notches = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(position) => position.y as f32 / 20.0,
                    };
                    let rows = if notches > 0.0 {
                        CHAT_WHEEL_ROWS
                    } else if notches < 0.0 {
                        -CHAT_WHEEL_ROWS
                    } else {
                        0
                    };
                    self.scroll_chat(rows);
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if self.screen().uses_ui() {
                    self.ui_input.text(&text);
                } else if self.screen == Screen::Chat {
                    self.chat_input.insert(&text);
                }
            }
            WindowEvent::Focused(false) => {
                self.keys.clear();
                self.capture_mouse(false);
                self.ui_input.clear();
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.mouse_captured
                && !self.screen().blocks_game_input()
                && !self.death_screen_visible()
            {
                // A positive delta moves the mouse to the right and down; the
                // game logic turns right and looks down with it.
                let speed = MOUSE_SENSITIVITY * self.options.controls.look_factor();
                let vertical = if self.options.controls.invert_mouse {
                    -1.0
                } else {
                    1.0
                };
                self.look.0 += delta.0 as f32 * speed;
                self.look.1 += delta.1 as f32 * speed * vertical;
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_f2_press_is_the_only_toggle() {
        assert!(is_debug_toggle(KeyCode::F4, ElementState::Pressed, false));
        assert!(!is_debug_toggle(KeyCode::F4, ElementState::Pressed, true));
        assert!(!is_debug_toggle(KeyCode::F4, ElementState::Released, false));
        assert!(!is_debug_toggle(KeyCode::F3, ElementState::Pressed, false));
        assert!(!is_debug_toggle(KeyCode::F2, ElementState::Pressed, false));
        assert!(!is_debug_toggle(KeyCode::F1, ElementState::Pressed, false));
    }
}

/// The menu flow, with the splash of the user's `splashes.txt` when there is
/// one (the title screen keeps its original list otherwise).
fn new_menu(last_server: String) -> MenuFlow {
    let seed = menu_seed();
    let mut menu = MenuFlow::new(
        ServerBook::open_default(),
        start_pinger(),
        last_server,
        seed,
    );
    let lines = crate::splash::load_lines();
    if let Some(splash) = crate::splash::choose(&lines, crate::splash::today(), seed) {
        menu.set_splash(splash);
    }
    menu
}
