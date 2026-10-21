// SPDX-License-Identifier: MIT OR Apache-2.0

//! Windowed client: winit event loop, free camera, chunk upload, debug overlay.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use umicraft_mesh::ChunkSnapshot;
use umicraft_render::{Camera, Overlay, Renderer};
use umicraft_world::{ChunkPos, World};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

use crate::args::Args;
use crate::error::ClientError;
use crate::mesher::{default_worker_count, MeshPool};
use crate::network::{self, NetEvent, NetworkConfig};
use crate::scene::SessionWorld;

const MOUSE_SENSITIVITY: f32 = 0.0022;
const MOVE_SPEED: f32 = 16.0;
const FAST_SPEED: f32 = 48.0;
const LOOK_SPEED: f32 = 1.6;
const FRAME_HISTORY: usize = 120;

/// Runs the windowed client until the user closes the window.
///
/// # Errors
/// Window/event-loop creation, network thread startup, or renderer
/// initialization failure. Runtime draw failures are surfaced the same way
/// after the event loop returns.
pub fn run(args: Args) -> Result<(), ClientError> {
    let (network, receiver) = network::spawn(NetworkConfig {
        host: args.host.clone(),
        port: args.port,
        name: args.name.clone(),
        view_distance: args.view_distance,
    })?;
    let event_loop = EventLoop::new().map_err(|error| ClientError::Window(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(args, receiver, network)?;
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
    camera_initialized: bool,
    awaiting_spawn: bool,
    overlay: Overlay,
    show_overlay: bool,
    mouse_captured: bool,
    keys: HashSet<KeyCode>,
    scene: SessionWorld,
    pool: MeshPool,
    pending: HashSet<ChunkPos>,
    deferred: HashSet<ChunkPos>,
    faces: HashMap<ChunkPos, usize>,
    net_rx: Receiver<NetEvent>,
    _network: JoinHandle<()>,
    last_frame: Instant,
    frame_times: Vec<f32>,
    frame_ms: f32,
    fps: f32,
    packets_seen: u64,
    exit_error: Option<String>,
}

impl App {
    fn new(
        args: Args,
        net_rx: Receiver<NetEvent>,
        network: JoinHandle<()>,
    ) -> Result<Self, ClientError> {
        let pool = MeshPool::new(default_worker_count())?;
        Ok(Self {
            args,
            window: None,
            renderer: None,
            camera: Camera::default(),
            camera_initialized: false,
            awaiting_spawn: false,
            overlay: Overlay::new(1280, 720),
            show_overlay: true,
            mouse_captured: false,
            keys: HashSet::new(),
            scene: SessionWorld::new(),
            pool,
            pending: HashSet::new(),
            deferred: HashSet::new(),
            faces: HashMap::new(),
            net_rx,
            _network: network,
            last_frame: Instant::now(),
            frame_times: Vec::with_capacity(FRAME_HISTORY),
            frame_ms: 0.0,
            fps: 0.0,
            packets_seen: 0,
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
                        KeyCode::KeyR => self.reset_camera(),
                        _ => {}
                    }
                }
                self.keys.insert(code);
            }
            ElementState::Released => {
                self.keys.remove(&code);
            }
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

    fn reset_camera(&mut self) {
        let mut camera = self
            .scene
            .placement
            .as_ref()
            .map_or_else(Camera::default, |placement| {
                Camera::from_minecraft(
                    placement.x,
                    placement.y,
                    placement.z,
                    placement.yaw,
                    placement.pitch,
                )
            });
        if let Some(renderer) = &self.renderer {
            let (width, height) = renderer.size();
            camera.set_aspect(width, height);
        }
        self.camera = camera;
        self.camera_initialized = true;
    }

    fn drain_network(&mut self) {
        while let Ok(event) = self.net_rx.try_recv() {
            let joined = matches!(event, NetEvent::JoinGame(_));
            if joined {
                self.reset_world_state();
            }
            self.packets_seen += 1;
            self.scene.apply(event);
            if joined {
                self.awaiting_spawn = true;
            }
        }
        if (self.awaiting_spawn || !self.camera_initialized) && self.scene.placement.is_some() {
            self.reset_camera();
            self.awaiting_spawn = false;
        }
    }

    fn reset_world_state(&mut self) {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_meshes();
        }
        self.pending.clear();
        self.deferred.clear();
        self.faces.clear();
        // Replacing the pool drops in-flight results from the previous world.
        match MeshPool::new(default_worker_count()) {
            Ok(pool) => self.pool = pool,
            Err(error) => self.exit_error = Some(error.to_string()),
        }
    }

    fn dispatch_dirty(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let Some(world) = self.scene.world.as_ref() else {
            return;
        };
        let dirty = std::mem::take(&mut self.scene.dirty);
        for pos in dirty {
            if self.pending.contains(&pos) {
                self.deferred.insert(pos);
                continue;
            }
            if world.chunk(pos).is_some() {
                let snapshot = ChunkSnapshot::from_world(world, pos);
                if self.pool.submit(pos, snapshot) {
                    self.pending.insert(pos);
                } else {
                    // Queue full (or workers gone): keep the chunk dirty and
                    // retry on a later frame instead of blocking the renderer.
                    self.scene.dirty.insert(pos);
                }
            } else {
                renderer.remove_chunk(pos);
                self.faces.remove(&pos);
            }
        }
    }

    fn collect_meshes(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        while let Some(result) = self.pool.try_recv() {
            let pos = result.pos;
            self.pending.remove(&pos);
            let loaded = self
                .scene
                .world
                .as_ref()
                .is_some_and(|world| world.chunk(pos).is_some());
            if loaded && !result.mesh.is_empty() {
                self.faces.insert(pos, result.mesh.face_count());
                renderer.upload_mesh(pos, &result.mesh);
            } else {
                self.faces.remove(&pos);
                renderer.remove_chunk(pos);
            }
            if self.deferred.remove(&pos) {
                self.scene.dirty.insert(pos);
            }
        }
    }

    fn update_camera(&mut self, dt: f32) {
        let mut forward = 0.0;
        let mut right = 0.0;
        let mut up = 0.0;
        if self.keys.contains(&KeyCode::KeyW) {
            forward += 1.0;
        }
        if self.keys.contains(&KeyCode::KeyS) {
            forward -= 1.0;
        }
        if self.keys.contains(&KeyCode::KeyD) {
            right += 1.0;
        }
        if self.keys.contains(&KeyCode::KeyA) {
            right -= 1.0;
        }
        if self.keys.contains(&KeyCode::Space) {
            up += 1.0;
        }
        if self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight) {
            up -= 1.0;
        }
        let fast =
            self.keys.contains(&KeyCode::ControlLeft) || self.keys.contains(&KeyCode::ControlRight);
        let speed = if fast { FAST_SPEED } else { MOVE_SPEED };
        self.camera.move_local(forward, right, up, speed, dt);

        let turn = LOOK_SPEED * dt;
        if self.keys.contains(&KeyCode::ArrowLeft) {
            self.camera.look(-turn, 0.0);
        }
        if self.keys.contains(&KeyCode::ArrowRight) {
            self.camera.look(turn, 0.0);
        }
        if self.keys.contains(&KeyCode::ArrowUp) {
            self.camera.look(0.0, turn);
        }
        if self.keys.contains(&KeyCode::ArrowDown) {
            self.camera.look(0.0, -turn);
        }
    }

    fn build_overlay(&mut self) {
        let (width, height) = self
            .renderer
            .as_ref()
            .map_or((self.args.width, self.args.height), Renderer::size);
        self.overlay.clear();
        if !self.show_overlay {
            return;
        }
        let scale = (height as f32 / 360.0).floor().clamp(1.0, 3.0);
        let line_height = 8.0 * scale;
        let chunks = self.scene.world.as_ref().map_or(0, World::chunk_count);
        let meshes = self.renderer.as_ref().map_or(0, Renderer::mesh_count);
        let faces: usize = self.faces.values().sum();
        let position = self.camera.position;
        let yaw = self.camera.yaw.to_degrees();
        let pitch = self.camera.pitch.to_degrees();
        let status: String = self.scene.status.chars().take(64).collect();
        let lines = [
            "UMICRAFT M2 // PROTOCOL 47".to_string(),
            status,
            format!("FPS {:.1}  FRAME {:.1} MS", self.fps, self.frame_ms),
            format!(
                "XYZ {:.1} {:.1} {:.1}  YAW {:.0} PITCH {:.0}",
                position.x, position.y, position.z, yaw, pitch
            ),
            format!("CHUNKS {}  MESHES {}  FACES {}", chunks, meshes, faces),
            format!(
                "CHAT {}  BLOCK UPDATES {}  PACKETS {}",
                self.scene.chat_messages, self.scene.block_updates, self.packets_seen
            ),
            "WASD MOVE  SPACE/SHIFT VERTICAL  CTRL FAST".to_string(),
            "RIGHT-DRAG LOOK  ARROWS LOOK  F1 OVERLAY".to_string(),
            "R RESET  ESC RELEASE CURSOR / QUIT".to_string(),
        ];
        let panel_width = lines
            .iter()
            .map(|line| Overlay::text_width(line, scale))
            .fold(0.0_f32, f32::max)
            + 20.0;
        let panel_height = lines.len() as f32 * line_height + 12.0;
        self.overlay
            .rect(4.0, 4.0, panel_width, panel_height, [0.0, 0.0, 0.0, 0.55]);
        for (index, line) in lines.iter().enumerate() {
            self.overlay.text(
                10.0,
                10.0 + index as f32 * line_height,
                scale,
                [1.0, 1.0, 1.0, 1.0],
                line,
            );
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

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let elapsed = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        let dt = elapsed.min(0.1);
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

        self.drain_network();
        self.dispatch_dirty();
        self.collect_meshes();
        self.update_camera(dt);
        self.build_overlay();

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        if let Err(error) = renderer.render(&self.camera, &self.overlay) {
            self.exit_error = Some(error.to_string());
            event_loop.exit();
            return;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Umicraft M2")
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
        match Renderer::new_windowed(instance, window.clone()) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(error) => {
                self.exit_error = Some(error.to_string());
                event_loop.exit();
                return;
            }
        }
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
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => self.capture_mouse(state == ElementState::Pressed),
            WindowEvent::Focused(false) => {
                self.keys.clear();
                self.capture_mouse(false);
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
            if self.mouse_captured {
                self.camera.look(
                    delta.0 as f32 * MOUSE_SENSITIVITY,
                    -(delta.1 as f32) * MOUSE_SENSITIVITY,
                );
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
