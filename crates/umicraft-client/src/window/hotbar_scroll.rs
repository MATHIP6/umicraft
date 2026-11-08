// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mouse wheel over a running game scrolls the selected hotbar slot.
//!
//! `Minecraft.runTick` takes only the direction of each wheel event
//! (`inventory.changeCurrentItem`): wheel up goes to the previous slot, wheel
//! down to the next one, and either wraps from one end of the hotbar to the
//! other. The slot goes out the same way a digit key's does, through
//! `select_slot` of the next input update.

use winit::event::MouseScrollDelta;

use super::App;
use crate::screen::Screen;

/// Slots of the hotbar.
const SLOTS: i32 = 9;
/// Pixels a touchpad scrolls for one notch of a wheel.
const PIXELS_PER_NOTCH: f32 = 20.0;

/// The slot after scrolling by `delta` from `current`, or `None` when the
/// wheel did not move. Only the direction counts, never the distance.
pub(super) fn scrolled_slot(current: u8, delta: MouseScrollDelta) -> Option<u8> {
    let notches = match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(position) => position.y as f32 / PIXELS_PER_NOTCH,
    };
    let step = if notches > 0.0 {
        1
    } else if notches < 0.0 {
        -1
    } else {
        return None;
    };
    Some((i32::from(current) - step).rem_euclid(SLOTS) as u8)
}

impl App {
    /// Scrolls the hotbar when the wheel turns in a game that has the mouse:
    /// no screen is open and the pointer is captured. Returns whether it did.
    pub(super) fn scroll_hotbar(&mut self, delta: MouseScrollDelta) -> bool {
        if self.session.is_none()
            || self.screen != Screen::None
            || self.death_screen_visible()
            || !self.mouse_captured
        {
            return false;
        }
        // Several notches in one frame add up: start from the slot already
        // chosen and not yet sent, if any.
        let shown = self.world.as_ref().map(|world| world.hud().selected_slot);
        let Some(current) = self.select_slot.or(shown) else {
            return false;
        };
        if let Some(slot) = scrolled_slot(current, delta) {
            self.select_slot = Some(slot);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::dpi::PhysicalPosition;

    fn notches(y: f32) -> MouseScrollDelta {
        MouseScrollDelta::LineDelta(0.0, y)
    }

    #[test]
    fn wheel_up_goes_to_the_previous_slot_and_wheel_down_to_the_next() {
        assert_eq!(scrolled_slot(4, notches(1.0)), Some(3));
        assert_eq!(scrolled_slot(4, notches(-1.0)), Some(5));
    }

    #[test]
    fn the_hotbar_wraps_around_at_both_ends() {
        assert_eq!(scrolled_slot(0, notches(1.0)), Some(8));
        assert_eq!(scrolled_slot(8, notches(-1.0)), Some(0));
    }

    #[test]
    fn only_the_direction_counts_not_the_distance() {
        assert_eq!(scrolled_slot(4, notches(5.0)), Some(3));
        assert_eq!(scrolled_slot(4, notches(-0.2)), Some(5));
    }

    #[test]
    fn a_still_wheel_changes_nothing() {
        assert_eq!(scrolled_slot(4, notches(0.0)), None);
    }

    #[test]
    fn a_touchpad_scrolls_by_pixels_with_the_same_direction() {
        let down = MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -30.0));
        let up = MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 8.0));
        assert_eq!(scrolled_slot(2, down), Some(3));
        assert_eq!(scrolled_slot(2, up), Some(1));
    }
}
