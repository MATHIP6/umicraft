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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::mouse_look::sensitivity_factor;

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
        let factor = |controls: &ControlOptions| sensitivity_factor(controls, 1.0);
        assert!(factor(&low) < factor(&ControlOptions::default()));
        assert!(factor(&ControlOptions::default()) < factor(&high));
        assert!(factor(&low) > 0.0);
    }
}
