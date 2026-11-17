// SPDX-License-Identifier: MIT OR Apache-2.0

//! The input the window hands to the game thread: the held keys, the
//! mouse look and the presses since the last update.

use winit::dpi::PhysicalPosition;
use winit::window::CursorGrabMode;

use super::App;
use crate::input::{InputState, InputUpdate};
use crate::options::Action;

impl App {
    pub(super) fn capture_mouse(&mut self, capture: bool) {
        let was_captured = self.mouse_captured;
        if was_captured && !capture {
            // A screen takes the mouse: the game forgets what was held.
            self.unpress_all();
        }
        self.mouse_captured = capture;
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
            self.pointer = (position.x as f32, position.y as f32);
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
        state.attack = self.mouse_captured && self.held(Action::Attack);
        state.use_item = self.mouse_captured && self.held(Action::Use);
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
        let changed = state != self.last_input
            || self.look != (0.0, 0.0)
            || self.select_slot.is_some()
            || self.hotbar_keys != [0; 9]
            || self.break_block
            || self.place_block
            || self.pick_block
            || self.drop_item.is_some()
            || self.toggle_perspective;
        if !changed {
            return;
        }
        let update = InputUpdate {
            created_at: self.last_input_time,
            state,
            look_yaw: self.look.0,
            look_pitch: self.look.1,
            select_slot: self.select_slot,
            hotbar_keys: self.hotbar_keys,
            break_block: self.break_block,
            place_block: self.place_block,
            pick_block: self.pick_block,
            drop_item: self.drop_item,
            toggle_perspective: self.toggle_perspective,
        };
        let sent = self
            .session
            .as_ref()
            .is_some_and(|session| session.send_input(update));
        if sent {
            if self.look != (0.0, 0.0) {
                if let Some(world) = self.world.as_mut() {
                    world.apply_look_delta(self.look.0, self.look.1);
                }
            }
            self.last_input = state;
            self.look = (0.0, 0.0);
            self.select_slot = None;
            self.hotbar_keys = [0; 9];
            self.break_block = false;
            self.place_block = false;
            self.pick_block = false;
            self.drop_item = None;
            self.toggle_perspective = false;
        }
    }
}
