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

use umicraft_game::chat_input::{push_sent, ChatInput, TabOutcome};
use umicraft_game::inventory::{drag_preview, DragPreview, WindowView, HOTBAR_START};
use umicraft_game::item_use::DropKind;
use umicraft_render::{
    draw_debug_only, draw_hud, draw_hud_gui, gui_scale, Camera, ChatInputView, HudLayout, HudState,
    NoIcons, Overlay, RenderEnvironment, Renderer, SkyState,
};
use umicraft_ui::game_menu::{GameMenu, GameMenuAction};
use umicraft_ui::widgets::{death_screen, DeathAction, BUTTON_DELAY_TICKS};
use umicraft_ui::{
    debug_screen::DebugScreen, draw_item, FontAtlas, Rect, ScreenLayout, Ui, UiOutput,
};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{
    DeviceEvent, ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

use crate::args::Args;
use crate::creative_input::CreativeInput;
use crate::creative_layout::CreativeLayout;
use crate::creative_screen::{draw_creative_overlay, CreativeScene};
use crate::dials::{DialAnimator, DialInputs};
use crate::entity_render::EntityTextures;
use crate::error::ClientError;
use crate::game::{GameEvent, Session};
use crate::input::{InputState, InputUpdate};
use crate::inventory_input::{Context, InventoryAction, InventoryInput};
use crate::nametag_draw::{draw_tags, place_tags};

use crate::inventory_screen::{draw_inventory_overlay, screen_input};
use crate::inventory_winit;
use crate::item_icons::{slot_item, IconSource, ItemResources, ItemView};
use crate::item_world::{push_held_item, ItemLayers, WorldItems};
use crate::mesher::default_worker_count;
use crate::network::NetworkConfig;
use crate::options::{self, Action, Options};
use crate::options_screen::{OptionsScreen, Outcome as OptionsOutcome};
use crate::screen::Screen;
use crate::terrain_textures::TerrainTextures;
use crate::texture_animator::TextureAnimator;
use crate::ui_font::{self, LoadedFont};
use crate::ui_input::UiInputTranslator;
use crate::world_view::WorldView;

#[path = "creative_window.rs"]
mod creative_window;

/// The look speed at the default sensitivity, in radians per pixel; the
/// sensitivity option scales it.
const MOUSE_SENSITIVITY: f32 = 0.0022;
/// The render distance `--view-distance` has when it is not given. Any other
/// value on the command line overrides the saved option.
const CLI_DEFAULT_VIEW_DISTANCE: u8 = 8;
/// How long the chat caret stays on, then off, in milliseconds (6 ticks).
const CARET_BLINK_MS: u128 = 300;
/// Rows a Page Up or Page Down scrolls the open chat.
const CHAT_PAGE: isize = 19;
/// Rows one mouse-wheel notch scrolls the open chat.
const CHAT_WHEEL_ROWS: isize = 7;
/// The furthest the chat can be scrolled: more than the 100 messages kept.
const CHAT_SCROLL_MAX: isize = 2_000;
const FRAME_HISTORY: usize = 120;

/// Runs the windowed client until the user closes the window.
///
/// # Errors
/// Window/event-loop creation, network thread startup, or renderer
/// initialization failure. Runtime draw failures are surfaced the same way
/// after the event loop returns.
pub fn run(args: Args) -> Result<(), ClientError> {
    let (host, port) = args.server().ok_or_else(|| {
        ClientError::Network("the windowed client needs a host and port".to_string())
    })?;
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
    let session = Session::start(NetworkConfig {
        host: host.to_string(),
        port,
        name: args.name.clone(),
        view_distance: options.video.render_distance,
        chat_mode: options.chat.visibility.wire(),
        chat_colors: options.chat.colors,
    })?;
    let event_loop = EventLoop::new().map_err(|error| ClientError::Window(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(args, session, options, options_path)?;
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
    /// Where the options are saved, when the platform has a data folder.
    options_path: Option<std::path::PathBuf>,
    /// The options as they are on disk and known to the server, to tell what
    /// changed when the options screen closes.
    saved_options: Options,
    /// The options screen, shown while [`Screen::Options`] is current.
    options_screen: OptionsScreen,
    /// What the options screen did in the last UI frame, applied by `redraw`.
    options_outcome: OptionsOutcome,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    overlay: Overlay,
    show_overlay: bool,
    mouse_captured: bool,
    keys: HashSet<KeyCode>,
    /// The camera, clock, chunk meshing and HUD of the game; created with
    /// the renderer, once the terrain textures are bound (ADR-0012, ADR-0013).
    world: Option<WorldView>,
    session: Session,
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
    sent_messages: Vec<String>,
    /// How many rows the open chat is scrolled up.
    chat_scroll: usize,
    /// When the chat opened; the caret blinks from here.
    chat_opened: Instant,
    /// The completion answer the chat already handled.
    seen_completions: u64,
    /// Whether a Control key is held (word movement in the chat).
    ctrl_held: bool,
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
    /// The acceptance screen shown while [`Screen::Debug`] is current.
    debug_screen: DebugScreen,
    /// The menu shown while [`Screen::GameMenu`] is current.
    game_menu: GameMenu,
    /// The entry the player clicked in the last UI frame, applied by `redraw`.
    menu_action: Option<GameMenuAction>,
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
    /// The first-person held item geometry of the frame.
    hand_layers: ItemLayers,
    /// Steps the water, lava and fire textures once per game tick.
    texture_animator: TextureAnimator,
    /// Entity textures bound for GPU rendering.
    entity_textures: Option<EntityTextures>,
    last_frame: Instant,
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
        session: Session,
        options: Options,
        options_path: Option<std::path::PathBuf>,
    ) -> Result<Self, ClientError> {
        // Assets are loaded once, before any worker or GPU resource exists.
        // A missing/corrupt installation picks the procedural set.
        let terrain = Some(TerrainTextures::load_with_packs(&args.resource_packs));
        let font = ui_font::load();
        let ui_viewport = Rect::new(0.0, 0.0, args.width as f32, args.height as f32);
        Ok(Self {
            args,
            saved_options: options.clone(),
            options,
            options_path,
            options_screen: OptionsScreen::new(),
            options_outcome: OptionsOutcome::default(),
            window: None,
            renderer: None,
            camera: Camera::default(),
            overlay: Overlay::new(1280, 720),
            show_overlay: true,
            mouse_captured: false,
            keys: HashSet::new(),
            world: None,
            session,
            last_input: InputState::default(),
            look: (0.0, 0.0),
            select_slot: None,
            break_block: false,
            attack_held: false,
            death_started: None,
            respawn_requested: false,
            place_block: false,
            screen: Screen::None,
            chat_input: ChatInput::new("", 0),
            sent_messages: Vec::new(),
            chat_scroll: 0,
            chat_opened: Instant::now(),
            seen_completions: 0,
            ctrl_held: false,
            use_held: false,
            pick_block: false,
            drop_item: None,
            toggle_perspective: false,
            creative_input: CreativeInput::new(),
            container_was_open: false,
            ui: Ui::new(ui_viewport),
            debug_screen: DebugScreen::new(),
            game_menu: GameMenu::new(),
            menu_action: None,
            ui_input: UiInputTranslator::new(),
            inventory_input: InventoryInput::new(),
            font,
            terrain,
            ui_atlas: None,
            gui_atlas: None,
            items: None,
            dials: DialAnimator::default(),
            world_items: WorldItems::default(),
            hand_layers: ItemLayers::default(),
            texture_animator: TextureAnimator::default(),
            entity_textures: None,
            last_frame: Instant::now(),
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
        // F2 is the debug-screen toggle. It is handled before the UI
        // translation so it can never be typed into the screen, and it is
        // ignored while the key repeats so one press toggles exactly once.
        // Releases are swallowed as well, so no F2 event ever reaches the UI.
        if code == KeyCode::F2 {
            if is_debug_toggle(code, event.state, event.repeat) {
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
            Screen::GameMenu => {
                self.handle_menu_key(event);
                return;
            }
            Screen::Options => {
                self.handle_options_key(event);
                return;
            }
            Screen::None => {}
        }
        match event.state {
            ElementState::Pressed => {
                if !event.repeat {
                    match code {
                        KeyCode::Escape => self.set_screen(Screen::None.after_escape()),
                        KeyCode::F1 => self.show_overlay = !self.show_overlay,
                        KeyCode::F4 => self.reload_resources(),
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

    /// Hot-reloads the active resource packs, rebinding the block atlas and
    /// re-meshing all loaded chunk columns.
    fn reload_resources(&mut self) {
        tracing::info!("reloading resource packs...");
        let terrain = TerrainTextures::load_with_packs(&self.args.resource_packs);
        if let Some(renderer) = self.renderer.as_mut() {
            let installed = terrain.install(renderer);
            upload_gui_atlas(renderer, installed.gui_atlas());
            upload_glint_texture(renderer, installed.glint_texture());
            self.gui_atlas = installed.gui_atlas().cloned();
            self.items = installed.items();
            self.dials = DialAnimator::new(installed.dials());
            self.texture_animator = TextureAnimator::new(installed.animations().to_vec());
            self.entity_textures = Some(EntityTextures::load(renderer));
            if let Some(world) = self.world.as_mut() {
                world.reload_terrain(&installed, default_worker_count(), renderer);
                let _ = self
                    .session
                    .events()
                    .try_send(crate::game::GameEvent::ReMeshAll);
            }
        }
        tracing::info!("resource packs reloaded");
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

    /// Handles a key while the options screen is open. While a key binding
    /// waits for its key, the next press is the key (Escape gives up).
    fn handle_options_key(&mut self, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let pressed = event.state == ElementState::Pressed && !event.repeat;
        if self.options_screen.capturing().is_some() {
            if pressed {
                if code == KeyCode::Escape {
                    self.options_screen.cancel_capture();
                } else {
                    self.options_screen.capture_key(&mut self.options, code);
                }
            }
            return;
        }
        if pressed && code == KeyCode::Escape {
            self.close_options();
            return;
        }
        self.ui_input.key(code, event.state, event.text.as_deref());
    }

    /// Writes the options to disk when they changed since the last write.
    fn save_options(&mut self) {
        if self.options == self.saved_options {
            return;
        }
        if let Some(path) = &self.options_path {
            if let Err(error) = self.options.save(path) {
                tracing::warn!(path = %path.display(), %error, "could not save the options");
            }
        }
        self.saved_options = self.options.clone();
    }

    /// Closes the options screen: saves the options, tells the server about
    /// a changed render distance or chat setting, and goes back to the menu.
    fn close_options(&mut self) {
        self.options_screen.reset();
        self.options.clamp();
        let server_settings = |options: &Options| {
            (
                options.video.render_distance,
                options.chat.visibility.wire(),
                options.chat.colors,
            )
        };
        let changed = server_settings(&self.options) != server_settings(&self.saved_options);
        self.save_options();
        if changed {
            let (view_distance, chat_mode, chat_colors) = server_settings(&self.options);
            let _ = self.session.events().try_send(GameEvent::ClientSettings {
                view_distance,
                chat_mode,
                chat_colors,
            });
        }
        self.set_screen(Screen::GameMenu);
    }

    /// Handles a key press while the game menu is open.
    fn handle_menu_key(&mut self, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        if code == KeyCode::Escape {
            if event.state == ElementState::Pressed && !event.repeat {
                self.set_screen(Screen::GameMenu.after_escape());
            }
            return;
        }
        self.ui_input.key(code, event.state, event.text.as_deref());
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

    /// Handles a key press while the inventory or a container screen is open.
    fn handle_inventory_key(&mut self, event_loop: &ActiveEventLoop, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let closes = code == KeyCode::Escape || code == self.options.keys.get(Action::Inventory);
        if event.state == ElementState::Pressed && !event.repeat && closes {
            self.close_inventory();
        }
        if event.state == ElementState::Pressed && !event.repeat {
            self.inventory_key(code);
        }
        self.ui_input.key(code, event.state, event.text.as_deref());
        let _ = event_loop;
    }

    /// Asks the server to respawn and gives the game back to the player.
    fn respawn(&mut self) {
        // MCP `GuiGameOver.actionPerformed` (button 0): `respawnPlayer`, then
        // the screen closes at once.
        let _ = self.session.events().try_send(GameEvent::Respawn);
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
            let free = open || self.screen().blocks_game_input() || self.death_screen_visible();
            self.capture_mouse(!free);
        }
    }

    /// Closes the inventory or container screen.
    fn close_inventory(&mut self) {
        if matches!(self.screen(), Screen::Inventory | Screen::Creative) {
            let _ = self.session.events().try_send(GameEvent::CloseInventory);
        }
        self.inventory_input.reset();
        self.creative_input.reset();
        self.set_screen(Screen::None);
        self.ui_input.clear();
    }

    /// The layout and input context of the open inventory screen, if any.
    fn inventory_screen(&self) -> Option<(ScreenLayout, Context)> {
        if self.screen() != Screen::Inventory {
            return None;
        }
        let size = self.window.as_ref()?.inner_size();
        let viewport = Rect::new(0.0, 0.0, size.width as f32, size.height as f32);
        let world = self.world.as_ref()?;
        Some(screen_input(
            viewport,
            world.window_view(),
            world.status().gamemode,
        ))
    }

    /// The stacks a drag split in progress will leave in the slots.
    fn inventory_drag_preview(&self, view: &WindowView) -> Option<DragPreview> {
        let (slots, button) = self.inventory_input.drag_in_progress()?;
        let creative = self.world.as_ref()?.status().gamemode == 1;
        drag_preview(view, slots, button, creative)
    }

    /// Sends the slot interaction of the inventory input to the game thread.
    fn send_inventory(&self, action: Option<InventoryAction>) {
        if let Some(action) = action {
            let _ = self.session.events().try_send(GameEvent::Inventory(action));
        }
    }

    /// Feeds a pointer move to the inventory input.
    fn inventory_pointer(&mut self, position: PhysicalPosition<f64>) {
        if let Some((layout, context)) = self.inventory_screen() {
            let point = (position.x as f32, position.y as f32);
            self.inventory_input.pointer_moved(&layout, point, context);
        }
    }

    /// Feeds a mouse button to the inventory input.
    fn inventory_button(&mut self, state: ElementState, button: MouseButton) {
        let (Some((layout, context)), Some(button)) =
            (self.inventory_screen(), inventory_winit::button(button))
        else {
            return;
        };
        let action = match state {
            ElementState::Pressed => {
                self.inventory_input
                    .button_pressed(&layout, button, context, Instant::now())
            }
            ElementState::Released => self
                .inventory_input
                .button_released(&layout, button, context),
        };
        self.send_inventory(action);
    }

    /// Feeds a key press (number keys, Q) to the inventory input.
    fn inventory_key(&mut self, code: KeyCode) {
        let (Some((layout, context)), Some(key)) =
            (self.inventory_screen(), inventory_winit::key(code))
        else {
            return;
        };
        let action = self.inventory_input.key_pressed(&layout, key, context);
        self.send_inventory(action);
    }

    /// Handles a key press while the chat input is open.
    fn handle_chat_key(&mut self, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        if event.state != ElementState::Pressed {
            return;
        }
        let word = self.ctrl_held;
        match code {
            KeyCode::Escape => self.set_screen(Screen::Chat.after_escape()),
            KeyCode::Enter | KeyCode::NumpadEnter => self.submit_chat(),
            KeyCode::Tab => self.complete_chat(),
            KeyCode::ArrowUp => self.chat_input.history_previous(&self.sent_messages),
            KeyCode::ArrowDown => self.chat_input.history_next(&self.sent_messages),
            KeyCode::PageUp => self.scroll_chat(CHAT_PAGE),
            KeyCode::PageDown => self.scroll_chat(-CHAT_PAGE),
            KeyCode::Backspace => self.chat_input.backspace(word),
            KeyCode::Delete => self.chat_input.delete(word),
            KeyCode::ArrowLeft => self.chat_input.left(word),
            KeyCode::ArrowRight => self.chat_input.right(word),
            KeyCode::Home => self.chat_input.home(),
            KeyCode::End => self.chat_input.end(),
            _ => {
                // Control combinations are shortcuts, not text.
                if !self.ctrl_held {
                    if let Some(text) = event.text.as_ref() {
                        self.chat_input.insert(text);
                    }
                }
            }
        }
    }

    /// Opens the chat with `initial` already typed (`/` for the command key).
    fn open_chat(&mut self, initial: &str) {
        self.set_screen(Screen::Chat);
        self.chat_input = ChatInput::new(initial, self.sent_messages.len());
        self.chat_scroll = 0;
        self.chat_opened = Instant::now();
    }

    /// Sends the typed line, remembers it for the history and closes the chat.
    fn submit_chat(&mut self) {
        let message = self.chat_input.text().trim().to_string();
        self.set_screen(Screen::None);
        if !message.is_empty() {
            push_sent(&mut self.sent_messages, &message);
            let _ = self.session.events().try_send(GameEvent::Chat(message));
        }
    }

    /// Presses Tab in the chat: asks the server or cycles its answer.
    fn complete_chat(&mut self) {
        match self.chat_input.tab() {
            TabOutcome::Nothing => {}
            TabOutcome::Request(text) => {
                let _ = self.session.events().try_send(GameEvent::TabComplete(text));
            }
            TabOutcome::Cycled { listing } => {
                if let Some(listing) = listing {
                    let _ = self
                        .session
                        .events()
                        .try_send(GameEvent::LocalChat(listing));
                }
            }
        }
    }

    /// Scrolls the open chat by `rows` (positive is towards older messages).
    fn scroll_chat(&mut self, rows: isize) {
        self.chat_scroll = (self.chat_scroll as isize + rows).clamp(0, CHAT_SCROLL_MAX) as usize;
    }

    /// Hands a new tab-completion answer to the chat input.
    fn process_completions(&mut self) {
        let Some(world) = self.world.as_ref() else {
            return;
        };
        let serial = world.hud().completions_serial;
        if serial == self.seen_completions {
            return;
        }
        let matches = world.hud().completions.clone();
        self.seen_completions = serial;
        if self.screen == Screen::Chat {
            if let Some(listing) = self.chat_input.on_completions(&matches) {
                let _ = self
                    .session
                    .events()
                    .try_send(GameEvent::LocalChat(listing));
            }
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
        if self
            .session
            .events()
            .try_send(GameEvent::Input(update))
            .is_ok()
        {
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
    /// leaves when the session ended.
    fn sync_world(&mut self, now: Instant) {
        let (Some(world), Some(renderer)) = (self.world.as_mut(), self.renderer.as_mut()) else {
            return;
        };
        world.sync(self.session.frames(), self.session.chunks(), renderer);
        world.update_camera(&mut self.camera, now);
        if let Some(reason) = world.ended() {
            self.exit_error = Some(reason.to_string());
        }
    }

    /// Builds the HUD and the debug block.
    fn build_overlay(&mut self) {
        let (width, height) = self
            .renderer
            .as_ref()
            .map_or((self.args.width, self.args.height), Renderer::size);
        self.overlay.clear();
        if !self.show_overlay {
            return;
        }
        let meshes = self.renderer.as_ref().map_or(0, Renderer::mesh_count);
        let faces = self.world.as_ref().map_or(0, WorldView::face_count);
        let mut game_hud = self
            .world
            .as_ref()
            .map(|world| world.hud().clone())
            .unwrap_or_default();
        self.options.chat.apply(&mut game_hud);
        let position = self.camera.position;
        let yaw = self.camera.yaw.to_degrees();
        let pitch = self.camera.pitch.to_degrees();
        let lines = [
            "UMICRAFT M3 // PROTOCOL 47".to_string(),
            format!(
                "FPS {:.1}  FRAME {:.1} MS  MESHES {}  FACES {}",
                self.fps, self.frame_ms, meshes, faces
            ),
            format!(
                "XYZ {:.1} {:.1} {:.1}  YAW {:.0} PITCH {:.0}",
                position.x, position.y, position.z, yaw, pitch
            ),
            "WASD MOVE  SPACE JUMP  SHIFT SNEAK  CTRL SPRINT".to_string(),
            "LEFT BREAK  RIGHT PLACE  1-9 HOTBAR  ESC MENU".to_string(),
        ];
        // The HUD owns the debug block, the crosshair and the bottom stack;
        // the client only feeds it the lines it already tracks.
        let chatting = self.screen == Screen::Chat;
        let hud = HudState {
            debug_lines: lines.to_vec(),
            chat_open: chatting,
            chat_scroll: self.chat_scroll,
            chat_input: chatting.then(|| ChatInputView {
                text: self.chat_input.text(),
                caret: self.chat_input.caret(),
                caret_visible: (self.chat_opened.elapsed().as_millis() / CARET_BLINK_MS)
                    .is_multiple_of(2),
            }),
            now: Some(Instant::now()),
            // The tab list shows while the key is held and no screen is open.
            show_tab_list: self.screen() == Screen::None && self.held(Action::PlayerList),
            ..game_hud
        };

        let sheets = crate::official::hud_sheets(self.gui_atlas.as_ref());
        if self.ui_atlas.is_some() && sheets.is_complete() {
            draw_debug_only(&mut self.overlay, &hud, width, height);
        } else {
            draw_hud(&mut self.overlay, &hud, width, height);
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
        let atlas = self.ui_atlas.as_ref()?;
        let sheets = crate::official::hud_sheets(self.gui_atlas.as_ref());
        let screen = self.screen();
        let textured_hud = sheets.is_complete();
        if self.world.is_none() && !textured_hud && !screen.uses_ui() {
            return None;
        }

        let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
        let input = self.ui_input.take();
        self.ui.begin_frame(viewport, input);
        self.ui.set_widgets_sheet(sheets.widgets);

        if textured_hud {
            let mut game_hud = self
                .world
                .as_ref()
                .map(|world| world.hud().clone())
                .unwrap_or_default();
            self.options.chat.apply(&mut game_hud);
            let chatting = self.screen == Screen::Chat;
            let hud = HudState {
                debug_lines: Vec::new(),
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
        }
        if let Some(world) = self.world.as_ref() {
            draw_hotbar_items(
                &mut self.ui,
                atlas,
                &world.window_view().player_slots,
                ItemView::new(self.items.as_deref(), self.dials.frames()),
                (width, height),
                textured_hud,
            );
            if !matches!(screen, Screen::Inventory | Screen::Creative) {
                let entities = world.entities(Instant::now());
                let tags = place_tags(&entities, &self.camera, width as f32, height as f32);
                draw_tags(&mut self.ui, atlas, &tags);
            }
        }

        if self.death_screen_visible() {
            let score = self.world.as_ref().map_or(0, |world| world.hud().score);
            let enabled = self.death_buttons_enabled(Instant::now());
            if death_screen(&mut self.ui, atlas, viewport, score, enabled) == DeathAction::Respawn {
                self.respawn();
            }
        } else {
            match screen {
                Screen::Debug => self.debug_screen.show(&mut self.ui, atlas),
                Screen::GameMenu => self.menu_action = self.game_menu.show(&mut self.ui, atlas),
                Screen::Options => {
                    self.options_outcome =
                        self.options_screen
                            .show(&mut self.ui, atlas, &mut self.options);
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
                        draw_creative_overlay(&mut self.ui, atlas, viewport, &layout, &scene);
                    }
                }
                Screen::Inventory => {
                    let wv = self.world.as_ref().map(|w| w.window_view().clone());
                    if let Some(wv) = wv {
                        let drag = self.inventory_drag_preview(&wv);
                        draw_inventory_overlay(
                            &mut self.ui,
                            atlas,
                            viewport,
                            &wv,
                            self.gui_atlas.as_ref(),
                            ItemView::new(self.items.as_deref(), self.dials.frames()),
                            drag.as_ref(),
                        );
                    }
                }
                Screen::None | Screen::Chat => {}
            }
        }
        Some(self.ui.end_frame())
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
        self.sync_world(now);
        self.process_completions();
        self.update_death_screen(now);
        self.update_container_cursor();
        self.publish_input();
        let environment = self
            .world
            .as_ref()
            .map(|world| world.environment(now, self.options.video.render_distance))
            .unwrap_or_default();
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
        match self.menu_action.take() {
            Some(GameMenuAction::Resume) => {
                self.set_screen(Screen::None);
                self.capture_mouse(true);
            }
            Some(GameMenuAction::Options) => self.set_screen(Screen::Options),
            Some(GameMenuAction::Achievements) | Some(GameMenuAction::Stats) => {
                // Not yet implemented: vanilla opens GuiAchievements / GuiStats.
            }
            Some(GameMenuAction::Quit) => {
                self.save_options();
                event_loop.exit();
                return;
            }
            None => {}
        }
        if std::mem::take(&mut self.options_outcome).done {
            self.close_options();
        }
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        // One environment per frame: the sky and the fog read the same clock
        // and interpolation fraction as the camera.
        renderer.set_environment(environment);
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

        if let (Some(entity_textures), Some(world)) =
            (self.entity_textures.as_ref(), self.world.as_ref())
        {
            let entities = world.entities(now);
            let hand = world.hand_pose(now);
            entity_textures.mesh_and_upload(&entities, renderer);
            entity_textures.upload_hand(&hand, self.camera.view_bob(), renderer);
            if let Some(items) = self.items.as_deref() {
                let dropped = self.world_items.build(&entities, items);
                renderer.set_world_items(&dropped.items);
                renderer.set_world_glint(&dropped.glint);
                self.hand_layers.clear();
                if let Some(stack) = held_stack(world).filter(|_| hand.visible) {
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
        // The frame-rate cap: wait out what is left of the frame budget.
        if self.options.video.max_fps < options::UNLIMITED_FPS {
            let budget = Duration::from_secs_f32(1.0 / self.options.video.max_fps as f32);
            if let Some(rest) = budget.checked_sub(now.elapsed()) {
                std::thread::sleep(rest);
            }
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// True when this key event must toggle the debug screen.
///
/// Only a fresh F2 press toggles: releases and auto-repeats are ignored so a
/// held key cannot flicker the screen.
fn is_debug_toggle(code: KeyCode, state: ElementState, repeat: bool) -> bool {
    code == KeyCode::F2 && state == ElementState::Pressed && !repeat
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

/// Draws the item icons, counts and durability bars of the hotbar over the
/// HUD frame. `textured` selects the vanilla layout of the `widgets.png` HUD;
/// otherwise the items follow the placeholder HUD.
fn draw_hotbar_items(
    ui: &mut Ui,
    atlas: &FontAtlas,
    player_slots: &[Option<umicraft_protocol::packets::play::ItemStack>; 45],
    items: ItemView<'_>,
    (width, height): (u32, u32),
    textured: bool,
) {
    for (index, stack) in player_slots[HOTBAR_START..HOTBAR_START + 9]
        .iter()
        .enumerate()
    {
        let Some(stack) = stack else { continue };
        let cell = if textured {
            vanilla_hotbar_cell(index, (width, height))
        } else {
            placeholder_hotbar_cell(index, (width, height))
        };
        draw_item(ui.draw(), atlas, cell, &slot_item(items, stack));
    }
}

/// The 16×16 item cell of hotbar slot `index` in the vanilla HUD: the hotbar
/// starts at `W/2 - 91` and its items 3 GUI pixels in, 20 apart, 19 above the
/// bottom (MCP `GuiIngame.renderHotbarItem`).
fn vanilla_hotbar_cell(index: usize, (width, height): (u32, u32)) -> Rect {
    let scale = gui_scale(width, height) as f32;
    let (gui_width, gui_height) = (width as f32 / scale, height as f32 / scale);
    Rect::new(
        (gui_width * 0.5 - 88.0 + index as f32 * 20.0) * scale,
        (gui_height - 19.0) * scale,
        16.0 * scale,
        16.0 * scale,
    )
}

/// The item cell of hotbar slot `index`, centred in the placeholder HUD slot.
fn placeholder_hotbar_cell(index: usize, (width, height): (u32, u32)) -> Rect {
    let layout = HudLayout::new(width, height);
    let icon = 16.0 * layout.scale();
    let slot = layout.slot_rect(index);
    Rect::new(
        slot.x + (slot.width - icon) * 0.5,
        slot.y + (slot.height - icon) * 0.5,
        icon,
        icon,
    )
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
        self.entity_textures = Some(EntityTextures::load(&renderer));
        self.renderer = Some(renderer);
        self.window = Some(window.clone());
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
                self.ui_input.modifiers(modifiers.state());
                let state = modifiers.state();
                self.inventory_input
                    .set_modifiers(state.shift_key(), state.control_key());
                self.creative_input
                    .set_modifiers(state.shift_key(), state.control_key());
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Keep the pointer current even while the UI is closed: F2 can
                // open the screen before the next mouse movement, and the first
                // click must use the physical cursor, not the initial origin.
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
                    // The chat has no pointer interaction yet.
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
        assert!(is_debug_toggle(KeyCode::F2, ElementState::Pressed, false));
        assert!(!is_debug_toggle(KeyCode::F2, ElementState::Pressed, true));
        assert!(!is_debug_toggle(KeyCode::F2, ElementState::Released, false));
        assert!(!is_debug_toggle(KeyCode::F1, ElementState::Pressed, false));
    }

    #[test]
    fn hotbar_items_sit_inside_the_vanilla_hotbar_slots() {
        // 1920x1080 is GUI scale 4 (the placeholder HUD caps at 3), where the
        // items used to drift away from their slots.
        let first = vanilla_hotbar_cell(0, (1920, 1080));
        let last = vanilla_hotbar_cell(8, (1920, 1080));
        assert_eq!((first.x, first.y, first.width), (608.0, 1004.0, 64.0));
        assert_eq!(last.x - first.x, 8.0 * 20.0 * 4.0);
    }
}
