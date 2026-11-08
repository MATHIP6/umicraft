// SPDX-License-Identifier: MIT OR Apache-2.0
//! Snapping of a dragged widget, for the HUD editor.

use super::HudRect;

/// Distance, in GUI pixels, within which a widget edge or center sticks to a guide.
pub const SNAP_DISTANCE: f32 = 4.0;

/// Moves `rect` onto the nearest guide (screen edges and center lines) when one is
/// within [`SNAP_DISTANCE`], per axis, then keeps it on screen. Returns the rectangle
/// to draw while dragging.
#[must_use]
pub fn snap_rect(rect: HudRect, screen: (f32, f32)) -> HudRect {
    HudRect {
        x: snap_axis(rect.x, rect.w, screen.0),
        y: snap_axis(rect.y, rect.h, screen.1),
        ..rect
    }
}

/// Snaps one axis: the start edge to 0, the end edge to `total`, or the middle to the middle.
fn snap_axis(pos: f32, size: f32, total: f32) -> f32 {
    let candidates = [0.0, total - size, (total - size) / 2.0];
    let snapped = candidates
        .into_iter()
        .filter(|c| (c - pos).abs() <= SNAP_DISTANCE)
        .min_by(|a, b| (a - pos).abs().total_cmp(&(b - pos).abs()))
        .unwrap_or(pos);
    if size >= total {
        0.0
    } else {
        snapped.clamp(0.0, total - size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (f32, f32) = (400.0, 300.0);

    fn rect(x: f32, y: f32) -> HudRect {
        HudRect {
            x,
            y,
            w: 100.0,
            h: 20.0,
        }
    }

    #[test]
    fn a_widget_near_an_edge_sticks_to_it() {
        let r = snap_rect(rect(3.0, 297.0 - 20.0), SCREEN);
        assert_eq!((r.x, r.y), (0.0, 280.0));
    }

    #[test]
    fn a_widget_near_the_middle_centers() {
        let r = snap_rect(rect(152.0, 100.0), SCREEN);
        assert_eq!(r.x, 150.0);
    }

    #[test]
    fn a_far_widget_is_left_alone() {
        let r = snap_rect(rect(60.0, 60.0), SCREEN);
        assert_eq!((r.x, r.y), (60.0, 60.0));
    }

    #[test]
    fn a_dragged_widget_is_kept_on_screen() {
        let r = snap_rect(rect(-50.0, 500.0), SCREEN);
        assert_eq!((r.x, r.y), (0.0, 280.0));
    }
}
