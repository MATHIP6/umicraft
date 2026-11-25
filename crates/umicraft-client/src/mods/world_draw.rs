// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the world layers of the external mods drew this frame (API 1.4,
//! ADR-0097), turned into what the renderer draws.
//!
//! The host gives world coordinates in blocks and widths in GUI pixels; the
//! renderer takes positions relative to the render origin (f32 stays precise
//! there, as for the entities) and widths in screen pixels. The buffers are kept
//! from frame to frame.

use glam::DVec3;
use umicraft_modhost::WorldDraw;
use umicraft_render::{ModWorldLine, ModWorldTriangle, Renderer};

/// The world layers' output of a frame and its renderer form.
#[derive(Debug, Default)]
pub(crate) struct ModWorldFrame {
    /// What the layers drew, refilled by `ModHost::draw_world_layers`.
    pub(crate) draw: WorldDraw,
    lines: Vec<ModWorldLine>,
    triangles: Vec<ModWorldTriangle>,
}

impl ModWorldFrame {
    /// Hands the geometry to `renderer`, relative to `origin`, with widths scaled
    /// by `gui_scale` screen pixels per GUI pixel.
    pub(crate) fn upload(&mut self, origin: DVec3, gui_scale: f32, renderer: &mut Renderer) {
        self.convert(origin, gui_scale);
        renderer.set_mod_world(&self.lines, &self.triangles);
    }

    /// Fills the renderer form from [`draw`](Self::draw).
    fn convert(&mut self, origin: DVec3, gui_scale: f32) {
        let relative = |point: [f64; 3]| (DVec3::from_array(point) - origin).as_vec3().to_array();
        self.lines.clear();
        self.lines
            .extend(self.draw.lines.iter().map(|line| ModWorldLine {
                from: relative(line.from),
                to: relative(line.to),
                color: line.color,
                width: line.thickness * gui_scale,
                through_walls: line.through_walls,
            }));
        self.triangles.clear();
        self.triangles
            .extend(self.draw.triangles.iter().map(|triangle| ModWorldTriangle {
                points: triangle.points.map(relative),
                color: triangle.color,
                through_walls: triangle.through_walls,
            }));
    }
}

#[cfg(test)]
mod tests {
    use umicraft_modhost::{WorldLine, WorldTriangle};

    use super::*;

    #[test]
    fn positions_become_relative_to_the_origin_and_widths_screen_pixels() {
        let mut frame = ModWorldFrame::default();
        let far = 29_999_000.0;
        frame.draw.lines.push(WorldLine {
            from: [far + 0.25, 64.0, -far],
            to: [far + 1.5, 65.0, -far + 2.0],
            color: [1.0; 4],
            thickness: 1.5,
            through_walls: true,
        });
        frame.draw.triangles.push(WorldTriangle {
            points: [[far, 64.0, -far]; 3],
            color: [0.5; 4],
            through_walls: false,
        });
        frame.convert(DVec3::new(far, 64.0, -far), 2.0);
        let line = frame.lines[0];
        assert_eq!((line.from, line.to), ([0.25, 0.0, 0.0], [1.5, 1.0, 2.0]));
        assert_eq!((line.width, line.through_walls), (3.0, true));
        assert_eq!(frame.triangles[0].points, [[0.0; 3]; 3]);
        frame.draw.clear();
        frame.convert(DVec3::ZERO, 2.0);
        assert!(frame.lines.is_empty() && frame.triangles.is_empty());
    }
}
