// SPDX-License-Identifier: MIT OR Apache-2.0

//! The input the window hands to the game thread: the held keys, the
//! mouse look and the presses since the last update.

use winit::dpi::PhysicalPosition;
use winit::window::CursorGrabMode;

use super::App;
use crate::input::mouse_look::look_turn;
use crate::input::{InputState, InputUpdate};
use crate::options::Action;
use umicraft_game::input::set_angles;

impl App {
    pub(super) fn capture_mouse(&mut self, capture: bool) {
        let was_captured = self.input.mouse_captured;
        if was_captured && !capture {
            // A screen takes the mouse: the game forgets what was held.
            self.unpress_all();
        }
        self.input.mouse_captured = capture;
        let mut centred = None;
        if let Some(window) = &self.window {
            if capture {
                // Locked is preferred (infinite relative motion); fall back to
                // Confined on backends where locking is unsupported.
                let _ = window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
            } else {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
                if was_captured {
                    // `MouseHelper.ungrabMouseCursor`: the freed cursor starts in
                    // the middle of the window, not where the look left it.
                    let size = window.inner_size();
                    let middle = PhysicalPosition::new(
                        f64::from(size.width) / 2.0,
                        f64::from(size.height) / 2.0,
                    );
                    let _ = window.set_cursor_position(middle);
                    centred = Some(middle);
                }
            }
            window.set_cursor_visible(!capture);
        }
        if let Some(position) = centred {
            // The move event may arrive late or not at all (it depends on the
            // platform); the screens must already know where the cursor is.
            self.input.pointer = (position.x as f32, position.y as f32);
            self.ui_input.pointer_moved(position);
            self.inventory_pointer(position);
            self.creative_pointer(position);
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
        state.attack = self.input.mouse_captured && self.held(Action::Attack);
        state.use_item = self.input.mouse_captured && self.held(Action::Use);
        state
    }

    /// Sends the input of the window to the game thread.
    ///
    /// The send never blocks: when the inbox is full the input is repeated
    /// on the next frame.
    pub(super) fn publish_input(&mut self) {
        if self.session.is_none() {
            return;
        }
        let mut state = self.key_state();
        self.mods.apply_to_state(&mut state);
        if self.mods.take_state_changed() {
            // Toggle sprint went on or off: keep it for the next run.
            self.options.mods = self.mods.config();
            self.save_options();
        }
        // `EntityRenderer.updateCameraAndRender` turns the look by the whole
        // pixels of the frame (`Mouse.getDX()` is an integer).
        let pixels = self.input.mouse.whole();
        let zoom = self.zoom.look_factor(self.mods.zoom().as_ref());
        let turn = look_turn(pixels, &self.options.controls, zoom);
        let changed = state != self.input.last
            || pixels != (0, 0)
            || self.presses.select_slot.is_some()
            || self.presses.hotbar_keys != [0; 9]
            || self.presses.break_block
            || self.presses.place_block
            || self.presses.pick_block
            || self.presses.drop_item.is_some()
            || self.presses.toggle_perspective;
        if !changed {
            return;
        }
        let update = InputUpdate {
            created_at: self.input.last_time,
            state,
            look_yaw: turn.0,
            look_pitch: turn.1,
            select_slot: self.presses.select_slot,
            hotbar_keys: self.presses.hotbar_keys,
            break_block: self.presses.break_block,
            place_block: self.presses.place_block,
            pick_block: self.presses.pick_block,
            drop_item: self.presses.drop_item,
            toggle_perspective: self.presses.toggle_perspective,
        };
        let sent = self
            .session
            .as_ref()
            .is_some_and(|session| session.send_input(update));
        if sent {
            if pixels != (0, 0) {
                if let Some(world) = self.world.as_mut() {
                    // The view of the window is in radians; the game thread
                    // turns the authoritative look with `set_angles`.
                    let (yaw, pitch) = set_angles(0.0, 0.0, turn);
                    world.apply_look_delta(yaw.to_radians(), pitch.to_radians());
                }
            }
            self.input.last = state;
            self.input.mouse.consume(pixels);
            self.presses.select_slot = None;
            self.presses.hotbar_keys = [0; 9];
            self.presses.break_block = false;
            self.presses.place_block = false;
            self.presses.pick_block = false;
            self.presses.drop_item = None;
            self.presses.toggle_perspective = false;
        }
    }
}
