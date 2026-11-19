// SPDX-License-Identifier: MIT OR Apache-2.0

//! The winit event loop handler of the app.

use std::sync::Arc;
use std::time::{Duration, Instant};

use umicraft_render::Renderer;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents};
use winit::window::{Fullscreen, Window};

use super::chat_control::CHAT_WHEEL_ROWS;
use super::startup::upload_sky_textures;
use super::App;
use crate::discord::Status;
use crate::error::ClientError;
use crate::resources::ui_font::{self};
use crate::screens::Screen;

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }
        let fullscreen_mode = self
            .options
            .video
            .fullscreen
            .then_some(Fullscreen::Borderless(None));
        let attributes = Window::default_attributes()
            .with_title("Umicraft M3")
            .with_fullscreen(fullscreen_mode)
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

        let backends = match self.args.backend.as_deref() {
            Some("dx12") | Some("directx12") => {
                tracing::info!("Selected graphics backend explicitly: DirectX 12");
                wgpu::Backends::DX12
            }
            Some("vulkan") | Some("vk") => {
                tracing::info!("Selected graphics backend explicitly: Vulkan");
                wgpu::Backends::VULKAN
            }
            Some(other) => {
                tracing::warn!(
                    backend = other,
                    "Unknown backend selection; falling back to all backends"
                );
                wgpu::Backends::all()
            }
            None => {
                tracing::info!("Graphics backend selection: auto (all backends enabled)");
                wgpu::Backends::all()
            }
        };

        let mut descriptor = wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(
            event_loop.owned_display_handle(),
        ));
        descriptor.backends = backends;
        let instance = wgpu::Instance::new(descriptor);
        let mut renderer = match Renderer::new_windowed(instance, window.clone()) {
            Ok(renderer) => renderer,
            Err(error) => {
                self.exit_error = Some(error.to_string());
                event_loop.exit();
                return;
            }
        };

        // Upload the initial fallback atlas immediately so the first loading/UI frame can be presented
        match ui_font::install(&mut renderer, &self.assets.font.atlas) {
            Ok(installed) => {
                self.assets.ui_atlas = Some(Arc::new(installed.atlas));
                self.hud_cache.invalidate();
            }
            Err(error) => {
                self.exit_error = Some(ClientError::FontAtlas(error).to_string());
                event_loop.exit();
                return;
            }
        }
        self.assets.brand_atlas = ui_font::install_brand(&mut renderer);

        // Minimal procedural fallback setup & sky textures for instant responsive frame
        renderer.use_procedural_blocks();
        upload_sky_textures(&mut renderer);

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
                self.input.ctrl_held = modifiers.state().control_key();
                self.input.shift_held = modifiers.state().shift_key();
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
                self.input.pointer = (position.x as f32, position.y as f32);
                self.ui_input.pointer_moved(position);
                self.inventory_pointer(position);
                self.creative_pointer(position);
                if !self.screen().uses_ui() && !self.death_screen_visible() {
                    self.ui_input.clear();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if state == ElementState::Released {
                    self.release_button(button);
                }
                let screen = self.screen();
                if self.capture_mouse_binding(screen, state, button) {
                    // The button became the binding that was waiting for one.
                } else if screen.uses_ui() || self.death_screen_visible() {
                    self.ui_input.pointer_button(state, button);
                    self.inventory_button(state, button);
                    self.creative_button(state, button);
                } else if screen.blocks_game_input() {
                    if screen == Screen::Chat && button == MouseButton::Left {
                        match state {
                            ElementState::Pressed => self.chat_press(),
                            ElementState::Released => self.chat.input.release(),
                        }
                    }
                } else {
                    self.handle_game_mouse(state, button);
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
                } else {
                    // With the zoom key held the wheel changes the zoom; else, in a
                    // running game, it scrolls the hotbar.
                    if !self.scroll_zoom(delta) {
                        self.scroll_hotbar(delta);
                    }
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if self.screen().uses_ui() {
                    self.ui_input.text(&text);
                } else if self.screen == Screen::Chat {
                    self.chat.input.insert(&text);
                }
            }
            // Coming back to a game with no screen open takes the mouse again,
            // so the player moves at once. The loss of the focus let it go;
            // an open screen (inventory, menu, death screen) keeps the pointer
            // free as before.
            WindowEvent::Focused(true) => {
                self.focused = true;
                if self.session.is_some()
                    && self.screen() == Screen::None
                    && !self.death_screen_visible()
                {
                    self.capture_mouse(true);
                }
            }
            WindowEvent::Focused(false) => {
                self.focused = false;
                // A fullscreen game left in the background would show through a
                // transparent taskbar, so it goes to the taskbar instead.
                if let Some(window) = self.window.as_ref().filter(|w| w.fullscreen().is_some()) {
                    window.set_minimized(true);
                }
                self.input.keys.clear();
                self.input.buttons.clear();
                self.capture_mouse(false);
                self.ui_input.clear();
            }
            WindowEvent::RedrawRequested => {
                self.redraw(event_loop);
            }
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
            if self.input.mouse_captured
                && !self.screen().blocks_game_input()
                && !self.death_screen_visible()
            {
                // A positive delta moves the mouse to the right and down; the
                // game logic turns right and looks down with it. The motion
                // is turned into a look once a frame, in whole pixels.
                self.input.mouse.add(delta);
                self.input.last_time = Instant::now();
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.discord
            .show(Status::of(self.screen(), self.session.is_some()));
        let now = Instant::now();
        if now + Duration::from_micros(500) >= self.clock.next_frame {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.clock.next_frame));
        }
    }
}
