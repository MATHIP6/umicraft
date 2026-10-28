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

use umicraft_game::inventory::HOTBAR_START;
use umicraft_game::item_use::DropKind;
use umicraft_mesh::item::ItemMesh;
use umicraft_render::{draw_hud, Camera, HudLayout, HudState, NoIcons, Overlay, Renderer};
use umicraft_ui::widgets::{
    container_layout, death_screen, player_inventory_layout, slot, ContainerLayout, DeathAction,
    InventoryLayout, BUTTON_DELAY_TICKS,
};
use umicraft_ui::{debug_screen::DebugScreen, draw_item, FontAtlas, Rect, Ui, UiOutput};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, ElementState, Ime, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

use crate::args::Args;
use crate::entity_textures::EntityTextures;
use crate::error::ClientError;
use crate::game::{GameEvent, Session};
use crate::input::{InputState, InputUpdate};
use crate::item_icons::{slot_item, IconSource, ItemResources};
use crate::item_world::{push_held_item, WorldItems};
use crate::mesher::default_worker_count;
use crate::network::NetworkConfig;
use crate::terrain_textures::TerrainTextures;
use crate::texture_animator::TextureAnimator;
use crate::ui_font::{self, LoadedFont};
use crate::ui_input::UiInputTranslator;
use crate::world_view::WorldView;

const MOUSE_SENSITIVITY: f32 = 0.0022;
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
    let session = Session::start(NetworkConfig {
        host: host.to_string(),
        port,
        name: args.name.clone(),
        view_distance: args.view_distance,
    })?;
    let event_loop = EventLoop::new().map_err(|error| ClientError::Window(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(args, session)?;
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
    /// The use (right) button is down.
    use_held: bool,
    /// The pick block (middle) button went down since the last input update.
    pick_block: bool,
    /// The drop key was pressed since the last input update.
    drop_item: Option<DropKind>,
    /// The perspective key was pressed since the last input update.
    toggle_perspective: bool,
    /// Whether the chat input is open.
    chat_open: bool,
    /// The line the player is typing.
    chat_input: String,
    /// Whether the player inventory is open (E key toggle).
    inventory_open: bool,
    /// Whether the M4 debug screen is open.
    show_debug_screen: bool,
    /// The immediate-mode context, owned by the render thread.
    ui: Ui,
    /// The acceptance screen shown while `show_debug_screen` is set.
    debug_screen: DebugScreen,
    /// The winit events waiting for the next UI frame.
    ui_input: UiInputTranslator,
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
    /// The dropped-item geometry of the frame, kept for its buffers.
    world_items: WorldItems,
    /// The first-person held item geometry of the frame.
    hand_item_mesh: ItemMesh,
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
    fn new(args: Args, session: Session) -> Result<Self, ClientError> {
        // Assets are loaded once, before any worker or GPU resource exists.
        // A missing/corrupt installation picks the procedural set.
        let terrain = Some(TerrainTextures::load_with_packs(&args.resource_packs));
        let font = ui_font::load();
        let ui_viewport = Rect::new(0.0, 0.0, args.width as f32, args.height as f32);
        Ok(Self {
            args,
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
            use_held: false,
            pick_block: false,
            drop_item: None,
            toggle_perspective: false,
            chat_open: false,
            chat_input: String::new(),
            inventory_open: false,
            show_debug_screen: false,
            ui: Ui::new(ui_viewport),
            debug_screen: DebugScreen::new(),
            ui_input: UiInputTranslator::new(),
            font,
            terrain,
            ui_atlas: None,
            gui_atlas: None,
            items: None,
            world_items: WorldItems::default(),
            hand_item_mesh: ItemMesh::default(),
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
                self.toggle_debug_screen();
            }
            return;
        }
        if self.show_debug_screen {
            self.ui_input.key(code, event.state, event.text.as_deref());
            return;
        }
        if self.chat_open {
            self.handle_chat_key(event);
            return;
        }
        if self.death_screen_visible() {
            // MCP `GuiGameOver.keyTyped` does nothing: Escape does not close it.
            return;
        }
        if self.inventory_open || self.container_is_open() {
            self.handle_inventory_key(event_loop, event);
            return;
        }
        match event.state {
            ElementState::Pressed => {
                if !event.repeat {
                    match code {
                        KeyCode::Escape => {
                            if self.mouse_captured {
                                self.capture_mouse(false);
                            } else {
                                event_loop.exit();
                            }
                        }
                        KeyCode::F1 => self.show_overlay = !self.show_overlay,
                        KeyCode::F4 => self.reload_resources(),
                        KeyCode::F5 => self.toggle_perspective = true,
                        KeyCode::KeyT => {
                            if self.keys.contains(&KeyCode::F3) {
                                self.reload_resources();
                            } else {
                                self.chat_open = true;
                                self.chat_input.clear();
                                self.capture_mouse(false);
                            }
                        }
                        KeyCode::KeyQ if self.mouse_captured => {
                            self.drop_item = Some(self.drop_kind());
                        }
                        KeyCode::KeyE => {
                            self.inventory_open = true;
                            self.capture_mouse(false);
                        }
                        _ => {}
                    }
                    if let Some(slot) = hotbar_slot(code) {
                        self.select_slot = Some(slot);
                    }
                }
                self.keys.insert(code);
            }
            ElementState::Released => {
                self.keys.remove(&code);
            }
        }
    }

    /// What the drop key drops: the whole stack with Ctrl held
    /// (MCP `Minecraft.runTick` line 2084, `GuiScreen.isCtrlKeyDown`).
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

    /// Opens or closes the M4 debug screen.
    ///
    /// Opening releases the cursor grab and clears the game input so no held
    /// key, look delta or click leaks into the game while the UI owns the
    /// mouse and keyboard. The game and network threads keep running; the
    /// screen only reads the render-side snapshot.
    fn toggle_debug_screen(&mut self) {
        self.show_debug_screen = !self.show_debug_screen;
        self.ui_input.clear();
        if self.show_debug_screen {
            self.chat_open = false;
            self.chat_input.clear();
            self.keys.clear();
            self.select_slot = None;
            self.break_block = false;
            self.place_block = false;
            self.pick_block = false;
            self.drop_item = None;
            self.look = (0.0, 0.0);
            self.capture_mouse(false);
        }
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

    /// True while a screen takes the keyboard and the mouse from the game.
    fn modal_screen_open(&self) -> bool {
        self.inventory_open || self.container_is_open() || self.death_screen_visible()
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
            self.chat_open = false;
            self.inventory_open = false;
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
        if event.state == ElementState::Pressed && !event.repeat {
            match code {
                KeyCode::Escape | KeyCode::KeyE => {
                    self.close_inventory();
                }
                _ => {}
            }
        }
        // Forward to the UI translator so slot-click-by-number-key works.
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

    /// Closes the inventory or container screen.
    fn close_inventory(&mut self) {
        if self.inventory_open || self.container_is_open() {
            let _ = self.session.events().try_send(GameEvent::CloseInventory);
        }
        self.inventory_open = false;
        self.ui_input.clear();
    }

    /// Handles a key press while the chat input is open.
    fn handle_chat_key(&mut self, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        match event.state {
            ElementState::Pressed => match code {
                KeyCode::Escape => {
                    self.chat_open = false;
                    self.chat_input.clear();
                }
                KeyCode::Enter => {
                    let message = std::mem::take(&mut self.chat_input);
                    self.chat_open = false;
                    if !message.is_empty() {
                        let _ = self.session.events().try_send(GameEvent::Chat(message));
                    }
                }
                KeyCode::Backspace => {
                    self.chat_input.pop();
                }
                _ => {
                    if let Some(text) = event.text.as_ref() {
                        for character in text.chars() {
                            if !character.is_control() && self.chat_input.len() < 100 {
                                self.chat_input.push(character);
                            }
                        }
                    }
                }
            },
            ElementState::Released => {}
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
        let show_inventory = self.modal_screen_open();
        if self.chat_open || self.show_debug_screen || show_inventory {
            // While the chat, debug screen or inventory is open the movement
            // keys do not steer the player.
            return InputState::default();
        }
        let mut state = InputState::default();
        if self.keys.contains(&KeyCode::KeyW) {
            state.forward += 1.0;
        }
        if self.keys.contains(&KeyCode::KeyS) {
            state.forward -= 1.0;
        }
        if self.keys.contains(&KeyCode::KeyA) {
            state.strafe += 1.0;
        }
        if self.keys.contains(&KeyCode::KeyD) {
            state.strafe -= 1.0;
        }
        state.jump = self.keys.contains(&KeyCode::Space);
        state.sneak =
            self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight);
        state.sprint =
            self.keys.contains(&KeyCode::ControlLeft) || self.keys.contains(&KeyCode::ControlRight);
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
        let game_hud = self
            .world
            .as_ref()
            .map(|world| world.hud().clone())
            .unwrap_or_default();
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
            "LEFT BREAK  RIGHT PLACE  1-9 HOTBAR  ESC RELEASE CURSOR".to_string(),
        ];
        // The HUD owns the debug block, the crosshair and the bottom stack;
        // the client only feeds it the lines it already tracks.
        let mut chat_lines = game_hud.chat_lines.clone();
        if self.chat_open {
            chat_lines.push(format!("> {}", self.chat_input));
        }
        let hud = HudState {
            debug_lines: lines.to_vec(),
            chat_lines,
            ..game_hud
        };
        draw_hud(&mut self.overlay, &hud, width, height);
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

    /// Builds one UI frame from the render-side snapshot: the hotbar items
    /// while a world is shown, and the modal screen on top when one is open.
    /// Returns `None` when there is nothing to draw.
    ///
    /// The frame is built on the render thread during redraw, after the HUD
    /// overlay, so the UI never touches the game thread or its state. The
    /// caller recycles the returned output after the renderer consumed the
    /// draw list.
    fn build_ui_frame(&mut self, width: u32, height: u32) -> Option<UiOutput> {
        let show_inventory = self.modal_screen_open();
        if self.world.is_none() && !self.show_debug_screen && !show_inventory {
            return None;
        }
        let atlas = self.ui_atlas.as_ref()?;
        let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
        let input = self.ui_input.take();
        self.ui.begin_frame(viewport, input);
        if let Some(world) = self.world.as_ref() {
            draw_hotbar_items(
                &mut self.ui,
                atlas,
                &world.window_view().player_slots,
                self.items.as_deref(),
                (width, height),
            );
        }
        if self.death_screen_visible() {
            let score = self.world.as_ref().map_or(0, |world| world.hud().score);
            let enabled = self.death_buttons_enabled(Instant::now());
            if death_screen(&mut self.ui, atlas, viewport, score, enabled) == DeathAction::Respawn {
                self.respawn();
            }
        } else if self.show_debug_screen {
            self.debug_screen.show(&mut self.ui, atlas);
        } else if show_inventory {
            let wv = self.world.as_ref().map(|w| w.window_view().clone());
            if let Some(wv) = wv {
                draw_inventory_overlay(
                    &mut self.ui,
                    atlas,
                    viewport,
                    &wv,
                    self.gui_atlas.as_ref(),
                    self.items.as_deref(),
                );
            }
        }
        Some(self.ui.end_frame())
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

        self.sync_world(now);
        self.update_death_screen(now);
        self.publish_input();
        let environment = self
            .world
            .as_ref()
            .map(|world| world.environment(now, self.args.view_distance))
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
        let ui_frame = self.build_ui_frame(width, height);
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
                renderer.set_world_items(self.world_items.build(&entities, items));
                self.hand_item_mesh.clear();
                if let Some(stack) = held_stack(world) {
                    let nbt = stack.nbt.as_deref();
                    push_held_item(
                        &mut self.hand_item_mesh,
                        (stack.item_id, stack.damage, nbt),
                        &hand,
                        self.camera.view_bob(),
                        items,
                    );
                }
                renderer.set_hand_items(&self.hand_item_mesh);
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

/// Draws the inventory or container screen as a modal overlay.
fn draw_inventory_overlay(
    ui: &mut umicraft_ui::Ui,
    atlas: &FontAtlas,
    viewport: Rect,
    wv: &umicraft_game::inventory::WindowView,
    gui_atlas: Option<&umicraft_assets::GuiAtlas>,
    items: Option<&ItemResources>,
) {
    let bg_color = [0.0_f32, 0.0, 0.0, 0.55];
    ui.draw().rect(viewport, bg_color);

    let textured = gui_atlas.is_some();

    match &wv.container {
        Some(container) => {
            let layout = container_layout(viewport, container.slots.len());
            let texture_name = gui_texture_for_container(container.slots.len());
            let rows = container.slots.len().div_ceil(9);
            let container_bg_h = 114 + rows as u32 * 18;
            draw_gui_background(
                ui,
                gui_atlas,
                texture_name,
                &layout.background,
                (176, container_bg_h),
            );
            draw_container_slots(
                ui,
                atlas,
                &layout,
                &container.slots,
                &wv.player_slots,
                textured,
                items,
            );
        }
        None => {
            let layout = player_inventory_layout(viewport);
            draw_gui_background(
                ui,
                gui_atlas,
                "container/inventory",
                &layout.background,
                (176, 166),
            );
            draw_player_inventory(ui, atlas, &layout, &wv.player_slots, textured, items);
        }
    }
}

/// Draws the GUI texture background for an inventory or container screen.
///
/// `gui_pixel_size` is the unscaled pixel dimensions of the GUI content inside
/// the texture (e.g. 176×166 for the player inventory). Vanilla's GUI PNGs are
/// often larger than the visible area (256×256 for inventory.png, which also
/// holds potion effect icons), so `sub_region` extracts just the GUI portion.
///
/// Falls back to a solid rectangle when the GUI atlas is not loaded or the
/// texture is not found.
fn draw_gui_background(
    ui: &mut umicraft_ui::Ui,
    gui_atlas: Option<&umicraft_assets::GuiAtlas>,
    name: &str,
    background: &Rect,
    gui_pixel_size: (u32, u32),
) {
    let region = gui_atlas.and_then(|a| a.region(name));
    match region {
        Some(region) => {
            let src = region.sub_region(0, 0, gui_pixel_size.0, gui_pixel_size.1);
            ui.draw().image(src, *background, [1.0, 1.0, 1.0, 1.0]);
        }
        None => {
            ui.draw().rect(*background, [0.12, 0.12, 0.15, 0.92]);
        }
    }
}

/// Returns the GUI texture name for a container given its slot count.
fn gui_texture_for_container(slot_count: usize) -> &'static str {
    match slot_count {
        27 => "container/generic_27",
        54 => "container/generic_54",
        3 => "container/furnace",
        9 => "container/dispenser",
        5 => "container/hopper",
        _ => "container/generic_27",
    }
}

/// The stack in the selected hotbar slot of the local player.
fn held_stack(world: &WorldView) -> Option<&umicraft_protocol::packets::play::ItemStack> {
    let selected = usize::from(world.hud().selected_slot).min(8);
    world.window_view().player_slots[HOTBAR_START + selected].as_ref()
}

/// Draws the item icons, counts and durability bars of the hotbar over the
/// HUD frame, centred in each of its slots.
fn draw_hotbar_items(
    ui: &mut Ui,
    atlas: &FontAtlas,
    player_slots: &[Option<umicraft_protocol::packets::play::ItemStack>; 45],
    items: Option<&ItemResources>,
    (width, height): (u32, u32),
) {
    let layout = HudLayout::new(width, height);
    let icon = 16.0 * layout.scale();
    for (index, stack) in player_slots[HOTBAR_START..HOTBAR_START + 9]
        .iter()
        .enumerate()
    {
        let Some(stack) = stack else { continue };
        let slot = layout.slot_rect(index);
        let cell = Rect::new(
            slot.x + (slot.width - icon) * 0.5,
            slot.y + (slot.height - icon) * 0.5,
            icon,
            icon,
        );
        draw_item(ui.draw(), atlas, cell, &slot_item(items, stack));
    }
}

/// Draws the player inventory (45 slots) using the layout grid.
fn draw_player_inventory(
    ui: &mut umicraft_ui::Ui,
    atlas: &FontAtlas,
    layout: &InventoryLayout,
    player_slots: &[Option<umicraft_protocol::packets::play::ItemStack>; 45],
    textured: bool,
    items: Option<&ItemResources>,
) {
    let mut label = String::with_capacity(8);
    for (i, rect) in layout.slots.iter().enumerate() {
        label.clear();
        std::fmt::Write::write_fmt(&mut label, format_args!("inv{i}")).unwrap();
        let id = ui.id(&label);
        let item = player_slots[i]
            .as_ref()
            .map(|stack| slot_item(items, stack));
        slot(ui, id, *rect, atlas, item.as_ref(), textured);
    }
}

/// Draws a container screen: container slots on top, player main+hotbar below.
fn draw_container_slots(
    ui: &mut umicraft_ui::Ui,
    atlas: &FontAtlas,
    layout: &ContainerLayout,
    container_slots: &[Option<umicraft_protocol::packets::play::ItemStack>],
    player_slots: &[Option<umicraft_protocol::packets::play::ItemStack>; 45],
    textured: bool,
    items: Option<&ItemResources>,
) {
    let mut label = String::with_capacity(8);
    for (i, rect) in layout.container_slots.iter().enumerate() {
        label.clear();
        std::fmt::Write::write_fmt(&mut label, format_args!("cs{i}")).unwrap();
        let id = ui.id(&label);
        let item = container_slots
            .get(i)
            .and_then(|s| s.as_ref())
            .map(|stack| slot_item(items, stack));
        slot(ui, id, *rect, atlas, item.as_ref(), textured);
    }
    for (i, &(inv_idx, rect)) in layout.player_slots.iter().enumerate() {
        label.clear();
        std::fmt::Write::write_fmt(&mut label, format_args!("ps{i}")).unwrap();
        let id = ui.id(&label);
        let item = player_slots
            .get(inv_idx)
            .and_then(|s| s.as_ref())
            .map(|stack| slot_item(items, stack));
        slot(ui, id, rect, atlas, item.as_ref(), textured);
    }
}

/// The hotbar slot of a number key.
fn hotbar_slot(code: KeyCode) -> Option<u8> {
    match code {
        KeyCode::Digit1 => Some(0),
        KeyCode::Digit2 => Some(1),
        KeyCode::Digit3 => Some(2),
        KeyCode::Digit4 => Some(3),
        KeyCode::Digit5 => Some(4),
        KeyCode::Digit6 => Some(5),
        KeyCode::Digit7 => Some(6),
        KeyCode::Digit8 => Some(7),
        KeyCode::Digit9 => Some(8),
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
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(event_loop, event),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.ui_input.modifiers(modifiers.state());
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Keep the pointer current even while the UI is closed: F2 can
                // open the screen before the next mouse movement, and the first
                // click must use the physical cursor, not the initial origin.
                self.ui_input.pointer_moved(position);
                let show_inventory = self.modal_screen_open();
                if !self.show_debug_screen && !show_inventory {
                    self.ui_input.clear();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let show_inventory = self.modal_screen_open();
                if self.show_debug_screen || show_inventory {
                    self.ui_input.pointer_button(state, button);
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
                let show_inventory = self.modal_screen_open();
                if self.show_debug_screen || show_inventory {
                    self.ui_input.wheel(delta);
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                let show_inventory = self.modal_screen_open();
                if self.show_debug_screen || show_inventory {
                    self.ui_input.text(&text);
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
            let show_inventory = self.modal_screen_open();
            if self.mouse_captured && !self.show_debug_screen && !show_inventory {
                // A positive delta moves the mouse to the right and down; the
                // game logic turns right and looks down with it.
                self.look.0 += delta.0 as f32 * MOUSE_SENSITIVITY;
                self.look.1 += delta.1 as f32 * MOUSE_SENSITIVITY;
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
}
