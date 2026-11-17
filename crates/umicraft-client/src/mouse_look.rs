// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mouse look in whole pixels (ADR-0076).
//!
//! The vanilla client reads the mouse with LWJGL's `Mouse.getDX()` and
//! `getDY()`, which are integers, and turns the look once a frame with
//! `EntityRenderer.updateCameraAndRender`: `f = sensitivity * 0.6F + 0.2F`,
//! `f1 = f * f * f * 8.0F`, `setAngles(dx * f1, dy * f1 * invert)`. Every
//! vanilla rotation is therefore a whole number of sensitivity steps, which
//! is what the aim checks of anti-cheats test. winit reports `f64` deltas
//! that can be fractional (Wayland, high-resolution mice), so the window
//! keeps the fraction here and hands the game only whole pixels.

use crate::options::ControlOptions;

/// The raw mouse motion not yet turned into a look.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MouseSteps {
    /// The motion in device units, positive to the right and downwards.
    raw: (f64, f64),
}

impl MouseSteps {
    /// Adds the motion of one device event.
    pub fn add(&mut self, delta: (f64, f64)) {
        self.raw.0 += delta.0;
        self.raw.1 += delta.1;
    }

    /// The whole pixels ready to be used, rounded towards zero; the
    /// fraction waits for the next events.
    #[must_use]
    pub fn whole(&self) -> (i32, i32) {
        (self.raw.0.trunc() as i32, self.raw.1.trunc() as i32)
    }

    /// Removes `pixels` once the game took them.
    pub fn consume(&mut self, pixels: (i32, i32)) {
        self.raw.0 -= f64::from(pixels.0);
        self.raw.1 -= f64::from(pixels.1);
    }

    /// Forgets the motion (a screen took the mouse).
    pub fn clear(&mut self) {
        self.raw = (0.0, 0.0);
    }
}

/// The sensitivity factor `f1` of `EntityRenderer.updateCameraAndRender`,
/// in `float` and in vanilla's order of operations; the zoom multiplies it.
#[must_use]
pub fn sensitivity_factor(controls: &ControlOptions, zoom: f32) -> f32 {
    let f = controls.mouse_sensitivity * 0.6 + 0.2;
    f * f * f * 8.0 * zoom
}

/// The `setAngles` arguments of `pixels`: `(dx * f1, dy * f1 * invert)`,
/// positive to the right and downwards (see
/// [`umicraft_game::input::set_angles`]).
#[must_use]
pub fn look_turn(pixels: (i32, i32), controls: &ControlOptions, zoom: f32) -> (f32, f32) {
    let factor = sensitivity_factor(controls, zoom);
    let invert = if controls.invert_mouse { -1.0 } else { 1.0 };
    (pixels.0 as f32 * factor, pixels.1 as f32 * factor * invert)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controls(sensitivity: f32) -> ControlOptions {
        ControlOptions {
            mouse_sensitivity: sensitivity,
            ..ControlOptions::default()
        }
    }

    #[test]
    fn fractions_wait_until_they_make_a_whole_pixel() {
        let mut steps = MouseSteps::default();
        steps.add((0.4, -0.4));
        assert_eq!(steps.whole(), (0, 0));
        steps.add((0.4, -0.4));
        assert_eq!(steps.whole(), (0, 0));
        steps.add((0.4, -0.4));
        // 1.2 and -1.2: one pixel each way, 0.2 kept.
        assert_eq!(steps.whole(), (1, -1));
        steps.consume((1, -1));
        assert_eq!(steps.whole(), (0, 0));
        steps.add((0.8, -0.8));
        assert_eq!(steps.whole(), (1, -1));
    }

    #[test]
    fn a_motion_not_taken_stays_whole() {
        let mut steps = MouseSteps::default();
        steps.add((3.0, 2.0));
        steps.add((4.0, -1.0));
        assert_eq!(steps.whole(), (7, 1));
        steps.clear();
        assert_eq!(steps.whole(), (0, 0));
    }

    #[test]
    fn the_factor_is_vanillas_float_formula() {
        for sensitivity in [0.0_f32, 0.2, 0.5, 0.8, 1.0] {
            let f = sensitivity * 0.6_f32 + 0.2_f32;
            let f1 = f * f * f * 8.0_f32;
            assert_eq!(
                sensitivity_factor(&controls(sensitivity), 1.0).to_bits(),
                f1.to_bits()
            );
        }
        assert_eq!(sensitivity_factor(&controls(0.5), 1.0), 1.0);
    }

    #[test]
    fn the_turn_is_a_whole_number_of_steps() {
        let factor = sensitivity_factor(&controls(0.37), 1.0);
        let turn = look_turn((7, -3), &controls(0.37), 1.0);
        assert_eq!(turn, (7.0 * factor, -3.0 * factor));
    }

    #[test]
    fn invert_flips_the_vertical_turn_only() {
        let inverted = ControlOptions {
            invert_mouse: true,
            ..controls(0.5)
        };
        assert_eq!(look_turn((2, 5), &inverted, 1.0), (2.0, -5.0));
    }
}
