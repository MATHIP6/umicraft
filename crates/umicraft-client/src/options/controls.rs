// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mouse options: look sensitivity and inversion.

use serde::{Deserialize, Serialize};

/// The mouse options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlOptions {
    /// The look sensitivity, `0.0..=1.0`; `0.5` is the vanilla default.
    pub mouse_sensitivity: f32,
    /// Whether moving the mouse up looks down.
    pub invert_mouse: bool,
}

impl Default for ControlOptions {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 0.5,
            invert_mouse: false,
        }
    }
}

impl ControlOptions {
    /// The multiplier of the base look speed. It is the vanilla formula
    /// `(sensitivity * 0.6 + 0.2)^3 * 8`, which is exactly `1.0` at the
    /// default sensitivity.
    #[must_use]
    pub fn look_factor(&self) -> f32 {
        (self.mouse_sensitivity * 0.6 + 0.2).powi(3) * 8.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_look_factor_grows_with_the_sensitivity() {
        let low = ControlOptions {
            mouse_sensitivity: 0.0,
            ..ControlOptions::default()
        };
        let high = ControlOptions {
            mouse_sensitivity: 1.0,
            ..ControlOptions::default()
        };
        assert!(low.look_factor() < ControlOptions::default().look_factor());
        assert!(ControlOptions::default().look_factor() < high.look_factor());
        assert!(low.look_factor() > 0.0);
    }
}
