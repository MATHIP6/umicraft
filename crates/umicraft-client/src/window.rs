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
use std::time::Instant;

use umicraft_render::{draw_hud, Camera, HudState, Overlay, Renderer};
use umicraft_ui::{debug_screen::DebugScreen, FontAtlas, Rect, Ui, UiOutput};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, ElementState, Ime, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

use crate::args::Args;
use crate::error::ClientError;
use crate::game::{GameEvent, Session};
use crate::input::{InputState, InputUpdate};
use crate::mesher::default_worker_count;
use crate::network::NetworkConfig;
use crate::terrain_textures::TerrainTextures;
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
    place_block: bool,
    /// Whether the chat input is open.
    chat_open: bool,
    /// The line the player is typing.
    chat_input: String,
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
    last_frame: Instant,
    frame_times: Vec<f32>,
    frame_ms: f32,
    fps: f32,
    exit_error: Option<String>,
}

impl App {
    fn new(args: Args, session: Session) -> Result<Self, ClientError> {
        // Assets are loaded once, before any worker or GPU resource exists.
        // A missing/corrupt installation picks the procedural set.
        let terrain = Some(TerrainTextures::load());
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
            place_block: false,
            chat_open: false,
            chat_input: String::new(),
            show_debug_screen: false,
            ui: Ui::new(ui_viewport),
            debug_screen: DebugScreen::new(),
            ui_input: UiInputTranslator::new(),
            font,
            terrain,
            ui_atlas: None,
            last_frame: Instant::now(),
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
                        KeyCode::KeyT => {
                            self.chat_open = true;
                            self.chat_input.clear();
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
            self.look = (0.0, 0.0);
            self.capture_mouse(false);
        }
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
        if self.chat_open || self.show_debug_screen {
            // While the chat or the debug screen is open the movement keys do
            // not steer the player.
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
            || self.place_block;
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

    /// Builds one UI frame from the render-side snapshot, or returns `None`
    /// while the debug screen is closed.
    ///
    /// The frame is built on the render thread during redraw, after the HUD
    /// overlay, so the UI never touches the game thread or its state. The
    /// caller recycles the returned output after the renderer consumed the
    /// draw list.
    fn build_ui_frame(&mut self, width: u32, height: u32) -> Option<UiOutput> {
        if !self.show_debug_screen {
            return None;
        }
        let atlas = self.ui_atlas.as_ref()?;
        let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
        let input = self.ui_input.take();
        self.ui.begin_frame(viewport, input);
        self.debug_screen.show(&mut self.ui, atlas);
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
        let rendered = match ui_frame.as_ref() {
            Some(frame) => renderer.render_with_ui(&self.camera, &self.overlay, &frame.draw_list),
            None => renderer.render(&self.camera, &self.overlay),
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
        match WorldView::new(&terrain, default_worker_count()) {
            Ok(world) => self.world = Some(world),
            Err(error) => {
                self.exit_error = Some(error.to_string());
                event_loop.exit();
                return;
            }
        }
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
                if !self.show_debug_screen {
                    self.ui_input.clear();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if self.show_debug_screen {
                    self.ui_input.pointer_button(state, button);
                } else {
                    match button {
                        MouseButton::Left => {
                            if state == ElementState::Pressed {
                                if self.mouse_captured {
                                    self.break_block = true;
                                } else {
                                    self.capture_mouse(true);
                                }
                            }
                        }
                        MouseButton::Right
                            if state == ElementState::Pressed && self.mouse_captured =>
                        {
                            self.place_block = true;
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.show_debug_screen {
                    self.ui_input.wheel(delta);
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if self.show_debug_screen {
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
            if self.mouse_captured && !self.show_debug_screen {
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
