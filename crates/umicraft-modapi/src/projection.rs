// SPDX-License-Identifier: MIT OR Apache-2.0

//! World-to-screen projection with the camera of a `FRAME` (API 1.1, ADR-0094).
//!
//! The host renders every position relative to an integer render origin, with a
//! view-projection that only carries the small remainder `eye - origin` (F2-1a).
//! Projecting the same way here, the point minus the origin in `f64` and then the
//! matrix in `f32`, gives the pixel the renderer draws the point at, even far from
//! the world origin.

use umicraft_modapi_sys::UmiFrame;

/// Projects `point` (blocks) with the camera of `frame` into GUI pixels from the
/// top-left corner; `None` behind the camera or outside the viewport.
pub(crate) fn world_to_screen(frame: &UmiFrame, point: [f64; 3]) -> Option<[f32; 2]> {
    let relative: [f32; 4] = [
        (point[0] - frame.render_origin[0]) as f32,
        (point[1] - frame.render_origin[1]) as f32,
        (point[2] - frame.render_origin[2]) as f32,
        1.0,
    ];
    let clip = transform(&frame.view_proj, relative);
    if clip[3] <= 0.0 {
        return None;
    }
    let (x, y) = (clip[0] / clip[3], clip[1] / clip[3]);
    if !(-1.0..=1.0).contains(&x) || !(-1.0..=1.0).contains(&y) || frame.gui_scale <= 0.0 {
        return None;
    }
    // NDC is y-up; GUI pixels are y-down.
    let width = frame.viewport[0] as f32 / frame.gui_scale;
    let height = frame.viewport[1] as f32 / frame.gui_scale;
    Some([(x + 1.0) / 2.0 * width, (1.0 - y) / 2.0 * height])
}

/// `matrix * vector` for a column-major matrix.
fn transform(matrix: &[[f32; 4]; 4], vector: [f32; 4]) -> [f32; 4] {
    core::array::from_fn(|row| (0..4).map(|col| matrix[col][row] * vector[col]).sum())
}

#[cfg(test)]
mod tests {
    use umicraft_modapi_sys::{UmiEventHeader, UmiEventId};

    use super::*;

    const NEAR: f32 = 0.1;
    const FAR: f32 = 512.0;

    /// The host's projection (`glam` right-handed, depth 0..1) for a camera at the
    /// world origin looking down -Z, as `umicraft-render` builds it.
    fn perspective(fov_y_degrees: f32, aspect: f32) -> [[f32; 4]; 4] {
        let f = 1.0 / (fov_y_degrees.to_radians() / 2.0).tan();
        let depth = FAR / (NEAR - FAR);
        [
            [f / aspect, 0.0, 0.0, 0.0],
            [0.0, f, 0.0, 0.0],
            [0.0, 0.0, depth, -1.0],
            [0.0, 0.0, depth * NEAR, 0.0],
        ]
    }

    /// A 200x100 GUI-pixel screen (400x200 physical at scale 2), 90° vertical FOV.
    fn frame(render_origin: [f64; 3]) -> UmiFrame {
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        UmiFrame {
            header: UmiEventHeader {
                size: core::mem::size_of::<UmiFrame>() as u32,
                id: UmiEventId::FRAME,
                flags: 0,
            },
            now_ms: 0,
            position: [0.0; 3],
            fps: 60,
            frame_ms: 16.0,
            yaw: 180.0,
            pitch: 0.0,
            left_clicks: 0,
            right_clicks: 0,
            ping_ms: -1,
            sprinting: 0,
            eye: render_origin,
            render_origin,
            view_proj: perspective(90.0, 2.0),
            viewport: [400, 200],
            gui_scale: 2.0,
            fov_y: 90.0,
            aspect: 2.0,
            view_bob: identity,
        }
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn points_on_each_axis_project_where_the_camera_sees_them() {
        let frame = frame([0.0; 3]);
        // Straight ahead (-Z): the centre of the screen.
        let ahead = world_to_screen(&frame, [0.0, 0.0, -10.0]).unwrap();
        assert!(close(ahead, [100.0, 50.0]), "{ahead:?}");
        // +X half way to the edge of a 90° x 2 aspect view at distance 10.
        let right = world_to_screen(&frame, [10.0, 0.0, -10.0]).unwrap();
        assert!(close(right, [150.0, 50.0]), "{right:?}");
        // +Y half way up: a quarter of the height from the top.
        let up = world_to_screen(&frame, [0.0, 5.0, -10.0]).unwrap();
        assert!(close(up, [100.0, 25.0]), "{up:?}");
        // -Y half way down.
        let down = world_to_screen(&frame, [0.0, -5.0, -10.0]).unwrap();
        assert!(close(down, [100.0, 75.0]), "{down:?}");
        // +Z is behind the camera, and a point on the X axis is in its plane.
        assert_eq!(world_to_screen(&frame, [0.0, 0.0, 10.0]), None);
        assert_eq!(world_to_screen(&frame, [10.0, 0.0, 0.0]), None);
    }

    #[test]
    fn points_outside_the_viewport_are_none() {
        let frame = frame([0.0; 3]);
        assert_eq!(world_to_screen(&frame, [30.0, 0.0, -10.0]), None);
        assert_eq!(world_to_screen(&frame, [0.0, -11.0, -10.0]), None);
    }

    #[test]
    fn far_from_the_world_origin_the_render_origin_keeps_the_precision() {
        // At x = 10 million an f32 position is only good to a metre; relative to
        // the origin, a point 1 cm right of the view axis still projects right.
        let origin = [10_000_000.0, 64.0, -10_000_000.0];
        let frame = frame(origin);
        let point = [origin[0] + 0.01, origin[1], origin[2] - 1.0];
        let at = world_to_screen(&frame, point).unwrap();
        assert!(at[0] > 100.0 && at[0] < 101.0, "{at:?}");
    }
}
