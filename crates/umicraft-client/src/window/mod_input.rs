// SPDX-License-Identifier: MIT OR Apache-2.0

//! The attack and use input of the mods (API 1.5, ADR-0098): the requests they
//! queued join the player's own presses and look, and the `INPUT` event tells them
//! about the bindings, the hand and the crosshair.
//!
//! A child of [`super`] so it reaches the window state without widening its
//! visibility.

use std::time::Instant;

use umicraft_game::input::TURN_DEGREES;
use umicraft_modhost::{Input, InputAction, InputFacts, InputRequest};

use super::state::PendingPresses;
use super::App;
use crate::options::Action;
use crate::screens::Screen;

/// Adds `request` to the presses and the look of the next input update.
fn apply_request(presses: &mut PendingPresses, request: InputRequest) {
    match request {
        InputRequest::Press { action, count } => {
            let pending = pending_mut(presses, action);
            *pending = pending.saturating_add(count);
        }
        InputRequest::ClearPress { action } => *pending_mut(presses, action) = 0,
        InputRequest::AddLook { yaw, pitch } => {
            presses.mod_look.0 += yaw;
            presses.mod_look.1 += pitch;
        }
    }
}

fn pending_mut(presses: &mut PendingPresses, action: InputAction) -> &mut u8 {
    match action {
        InputAction::Attack => &mut presses.attack_presses,
        InputAction::Use => &mut presses.use_presses,
    }
}

/// A look in degrees as the turn `set_angles` takes (it multiplies by
/// [`TURN_DEGREES`]), so the mods' look travels the mouse's path.
pub(super) fn mod_look_turn((yaw, pitch): (f32, f32)) -> (f32, f32) {
    (
        (f64::from(yaw) / TURN_DEGREES) as f32,
        (f64::from(pitch) / TURN_DEGREES) as f32,
    )
}

impl App {
    /// Whether the game owns the input: a world, no screen open and the mouse
    /// captured, the conditions under which the player's own click counts
    /// ([`run_action`](Self::press_binding)).
    fn game_has_focus(&self) -> bool {
        self.world.is_some() && self.screen() == Screen::None && self.input.mouse_captured
    }

    /// Applies the input requests the mods queued since the last frame, before the
    /// input update is built, and tells the host whether the game owns the input.
    /// Without the focus the requests are dropped, as a real press would be.
    pub(super) fn apply_mod_input(&mut self) {
        let focus = self.game_has_focus();
        let Some(mods) = self.external_mods.as_ref() else {
            return;
        };
        mods.set_game_focus(focus);
        if !focus {
            mods.clear_input();
            return;
        }
        let presses = &mut self.presses;
        mods.drain_input(|request| apply_request(presses, request));
    }

    /// The `INPUT` event of this frame; `None` when nobody listens or before the
    /// game thread published the hand.
    pub(super) fn input_event(&self, now: Instant) -> Option<Input> {
        if !self.render_bus.has_subscribers::<Input>() {
            return None;
        }
        let hand = self.world.as_ref()?.mod_hand()?;
        Some(Input::new(InputFacts {
            now_ms: u64::try_from(now.duration_since(self.clock.started).as_millis())
                .unwrap_or(u64::MAX),
            attack_held: self.held(Action::Attack),
            use_held: self.held(Action::Use),
            attack_presses: self.presses.attack_presses,
            use_presses: self.presses.use_presses,
            game_focus: self.game_has_focus(),
            hand,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::input::set_angles;

    fn presses() -> PendingPresses {
        PendingPresses {
            select_slot: None,
            hotbar_keys: [0; 9],
            attack_presses: 1,
            use_presses: 0,
            pick_block: false,
            drop_item: None,
            toggle_perspective: false,
            screenshot: false,
            respawn: false,
            mod_look: (0.0, 0.0),
        }
    }

    #[test]
    fn requests_join_the_players_presses_in_call_order() {
        let mut pending = presses();
        let attack = InputAction::Attack;
        apply_request(
            &mut pending,
            InputRequest::Press {
                action: attack,
                count: 2,
            },
        );
        assert_eq!(pending.attack_presses, 3, "added to the player's own press");
        apply_request(&mut pending, InputRequest::ClearPress { action: attack });
        apply_request(
            &mut pending,
            InputRequest::Press {
                action: attack,
                count: 1,
            },
        );
        assert_eq!(pending.attack_presses, 1);
        apply_request(
            &mut pending,
            InputRequest::Press {
                action: InputAction::Use,
                count: 8,
            },
        );
        assert_eq!(pending.use_presses, 8);
    }

    #[test]
    fn presses_saturate() {
        let mut pending = presses();
        pending.attack_presses = 250;
        apply_request(
            &mut pending,
            InputRequest::Press {
                action: InputAction::Attack,
                count: 8,
            },
        );
        assert_eq!(pending.attack_presses, u8::MAX);
    }

    #[test]
    fn look_requests_add_up_and_turn_by_their_degrees() {
        let mut pending = presses();
        apply_request(
            &mut pending,
            InputRequest::AddLook {
                yaw: 10.0,
                pitch: -3.0,
            },
        );
        apply_request(
            &mut pending,
            InputRequest::AddLook {
                yaw: 5.0,
                pitch: 1.0,
            },
        );
        assert_eq!(pending.mod_look, (15.0, -2.0));
        let (yaw, pitch) = set_angles(0.0, 0.0, mod_look_turn(pending.mod_look));
        assert!((yaw - 15.0).abs() < 1e-4 && (pitch + 2.0).abs() < 1e-4);
    }
}
