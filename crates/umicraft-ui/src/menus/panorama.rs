// SPDX-License-Identifier: MIT OR Apache-2.0

//! The rotating panorama behind the title screen (MCP `GuiMainMenu`).
//!
//! The UI crate owns no GPU resources, so this module provides only what the
//! renderer needs to draw the cube: the handles of the six already-uploaded
//! faces, the per-face turns and the camera rotation of the current frame.
//! Source: `GuiMainMenu.renderSkybox` (`panoramaTimer`, `gluPerspective(120,
//! 1, 0.05, 10)`, the face rotations) and `drawPanorama`.
//!
//! # Deviation
//!
//! Vanilla renders the cube 64 times with sub-pixel offsets and blends seven
//! blur passes over it (`rotateAndBlurSkybox`). That blur is **skipped**: the
//! renderer draws the cube once. The rotation is exact.

use super::background::draw_menu_background;
use super::common::gradient;
use crate::context::Ui;

/// The vertical field of view of the panorama camera in degrees.
pub const FIELD_OF_VIEW_DEGREES: f32 = 120.0;
/// Near and far planes of the panorama camera.
pub const CLIP_PLANES: (f32, f32) = (0.05, 10.0);

/// The yaw and pitch turn of each cube face, in degrees, in file order
/// `panorama_0..5` (`GuiMainMenu.renderSkybox`: none, y 90, y 180, y -90,
/// x 90, x -90).
pub const FACE_TURNS_DEGREES: [(f32, f32); 6] = [
    (0.0, 0.0),
    (90.0, 0.0),
    (180.0, 0.0),
    (-90.0, 0.0),
    (0.0, 90.0),
    (0.0, -90.0),
];

/// Opaque ids of the six panorama faces the renderer has uploaded.
///
/// The ids mean nothing to the UI; the renderer maps them back to its
/// textures. The textures are the user's download (spec §3) and are never
/// part of the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanoramaTextures {
    /// The id of `panorama_0` to `panorama_5`.
    pub faces: [u64; 6],
}

/// The camera rotation of one frame, in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanoramaRotation {
    /// Rotation around the vertical axis: `-timer * 0.1`.
    pub yaw_degrees: f32,
    /// Rotation around the horizontal axis: `sin(timer / 400) * 25 + 20`.
    pub pitch_degrees: f32,
}

/// What the renderer has to draw behind the menu this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanoramaView {
    /// The faces to draw.
    pub textures: PanoramaTextures,
    /// The camera rotation.
    pub rotation: PanoramaRotation,
}

/// The camera rotation `timer` ticks (plus the partial tick) after the start.
///
/// ```
/// use umicraft_ui::menus::panorama_rotation;
/// let r = panorama_rotation(100.0);
/// assert!((r.yaw_degrees + 10.0).abs() < 1e-4);
/// ```
#[must_use]
pub fn panorama_rotation(timer: f32) -> PanoramaRotation {
    PanoramaRotation {
        yaw_degrees: -timer * 0.1,
        pitch_degrees: (timer / 400.0).sin() * 25.0 + 20.0,
    }
}

/// The animation clock of the panorama.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PanoramaBackground {
    ticks: u32,
}

impl PanoramaBackground {
    /// A panorama at its start.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances the clock by one game tick (`GuiMainMenu.updateScreen`).
    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
    }

    /// The ticks counted so far.
    #[must_use]
    pub fn ticks(&self) -> u32 {
        self.ticks
    }

    /// The rotation `partial_tick` (0 to 1) after the last tick.
    #[must_use]
    pub fn rotation(&self, partial_tick: f32) -> PanoramaRotation {
        panorama_rotation(self.ticks as f32 + partial_tick)
    }

    /// Draws the backdrop and returns what the renderer must draw below it.
    ///
    /// With `textures`, the white fade and the dark fade of
    /// `GuiMainMenu.drawScreen` are drawn here and the returned view tells
    /// the renderer to draw the cube first. Without them the dirt
    /// background is drawn and nothing is returned.
    pub fn draw(
        &self,
        ui: &mut Ui,
        textures: Option<PanoramaTextures>,
        partial_tick: f32,
    ) -> Option<PanoramaView> {
        let Some(textures) = textures else {
            draw_menu_background(ui);
            return None;
        };
        let viewport = ui.viewport();
        gradient(ui, viewport, [1.0, 1.0, 1.0, 0.5], [1.0, 1.0, 1.0, 0.0]);
        gradient(ui, viewport, [0.0; 4], [0.0, 0.0, 0.0, 0.5]);
        Some(PanoramaView {
            textures,
            rotation: self.rotation(partial_tick),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::UiInput;
    use crate::menus::common::testing::frame;

    const TEXTURES: PanoramaTextures = PanoramaTextures {
        faces: [0, 1, 2, 3, 4, 5],
    };

    #[test]
    fn the_rotation_is_the_vanilla_formula() {
        let start = panorama_rotation(0.0);
        assert_eq!(start.yaw_degrees, 0.0);
        assert_eq!(start.pitch_degrees, 20.0);
        let later = panorama_rotation(400.0 * std::f32::consts::FRAC_PI_2);
        assert!((later.pitch_degrees - 45.0).abs() < 1e-3);
        assert!((later.yaw_degrees + 400.0 * std::f32::consts::FRAC_PI_2 * 0.1).abs() < 1e-3);
    }

    #[test]
    fn ticks_and_the_partial_tick_advance_the_rotation() {
        let mut panorama = PanoramaBackground::new();
        panorama.tick();
        panorama.tick();
        assert_eq!(panorama.ticks(), 2);
        assert!((panorama.rotation(0.5).yaw_degrees + 0.25).abs() < 1e-6);
    }

    #[test]
    fn without_textures_the_default_background_is_drawn() {
        let panorama = PanoramaBackground::new();
        let (view, commands) = frame(UiInput::default(), |ui| {
            let view = panorama.draw(ui, None, 0.0);
            (view, ui.draw().len())
        });
        assert_eq!(view, None);
        assert!(commands > 0);
    }

    #[test]
    fn with_textures_the_view_is_returned_for_the_renderer() {
        let mut panorama = PanoramaBackground::new();
        panorama.tick();
        let view = frame(UiInput::default(), |ui| {
            panorama.draw(ui, Some(TEXTURES), 0.0)
        })
        .expect("a view");
        assert_eq!(view.textures, TEXTURES);
        assert_eq!(view.rotation, panorama_rotation(1.0));
    }

    #[test]
    fn the_six_faces_cover_the_cube() {
        assert_eq!(FACE_TURNS_DEGREES.len(), 6);
        assert_eq!(FACE_TURNS_DEGREES[4], (0.0, 90.0));
    }
}
