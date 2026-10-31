// SPDX-License-Identifier: MIT OR Apache-2.0

//! The world border as the renderer needs it: plain data published by the
//! game thread, plus the arithmetic of `WorldBorder` the pass relies on.
//!
//! Source: MCP `WorldBorder` (`minX`..`maxZ` clamp to the portal boundary,
//! `getClosestDistance`, `getStatus`) and `EnumBorderStatus` (the colours).

/// Whether the border is moving (`WorldBorder.getStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderStatus {
    /// Not resizing; drawn blue.
    #[default]
    Stationary,
    /// Growing; drawn green.
    Growing,
    /// Shrinking; drawn red.
    Shrinking,
}

impl BorderStatus {
    /// The status of a resize from `start` to `end` diameter.
    #[must_use]
    pub fn of_resize(start: f64, end: f64) -> Self {
        if end < start {
            Self::Shrinking
        } else if end > start {
            Self::Growing
        } else {
            Self::Stationary
        }
    }

    /// The wall tint (`EnumBorderStatus.getID()` split into channels).
    #[must_use]
    pub fn colour(self) -> [f32; 3] {
        let id: u32 = match self {
            Self::Growing => 0x41_FF00,
            Self::Shrinking => 0xFF_3030,
            Self::Stationary => 0x20_A0FF,
        };
        [16, 8, 0].map(|shift| ((id >> shift) & 0xFF) as f32 / 255.0)
    }
}

/// One frame of the world border of the current dimension.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderView {
    /// Centre `(x, z)`.
    pub center: (f64, f64),
    /// Current diameter in blocks (`WorldBorder.getDiameter`).
    pub diameter: f64,
    /// Half-size the walls are clamped to (`WorldBorder.worldSize`).
    pub portal_boundary: i32,
    /// Moving or not.
    pub status: BorderStatus,
}

impl BorderView {
    /// The west wall's x (`WorldBorder.minX`).
    #[must_use]
    pub fn min_x(&self) -> f64 {
        (self.center.0 - self.diameter / 2.0).max(-f64::from(self.portal_boundary))
    }

    /// The east wall's x (`WorldBorder.maxX`).
    #[must_use]
    pub fn max_x(&self) -> f64 {
        (self.center.0 + self.diameter / 2.0).min(f64::from(self.portal_boundary))
    }

    /// The north wall's z (`WorldBorder.minZ`).
    #[must_use]
    pub fn min_z(&self) -> f64 {
        (self.center.1 - self.diameter / 2.0).max(-f64::from(self.portal_boundary))
    }

    /// The south wall's z (`WorldBorder.maxZ`).
    #[must_use]
    pub fn max_z(&self) -> f64 {
        (self.center.1 + self.diameter / 2.0).min(f64::from(self.portal_boundary))
    }

    /// Distance from `(x, z)` to the nearest wall; negative outside
    /// (`WorldBorder.getClosestDistance`).
    #[must_use]
    pub fn closest_distance(&self, x: f64, z: f64) -> f64 {
        (x - self.min_x())
            .min(self.max_x() - x)
            .min(z - self.min_z())
            .min(self.max_z() - z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(diameter: f64) -> BorderView {
        BorderView {
            center: (0.0, 0.0),
            diameter,
            portal_boundary: 29_999_984,
            status: BorderStatus::Stationary,
        }
    }

    #[test]
    fn status_follows_the_direction_of_the_resize() {
        assert_eq!(BorderStatus::of_resize(10.0, 5.0), BorderStatus::Shrinking);
        assert_eq!(BorderStatus::of_resize(5.0, 10.0), BorderStatus::Growing);
        assert_eq!(BorderStatus::of_resize(5.0, 5.0), BorderStatus::Stationary);
    }

    #[test]
    fn colours_are_the_enum_border_status_ids() {
        assert_eq!(BorderStatus::Shrinking.colour()[0], 1.0);
        assert!((BorderStatus::Growing.colour()[1] - 1.0).abs() < 1e-6);
        let blue = BorderStatus::Stationary.colour();
        assert!((blue[0] - 32.0 / 255.0).abs() < 1e-6 && blue[2] == 1.0);
        assert!(BorderStatus::Growing.colour()[2] == 0.0);
    }

    #[test]
    fn walls_clamp_to_the_portal_boundary() {
        let huge = view(6.0e7);
        assert_eq!(huge.max_x(), 29_999_984.0);
        assert_eq!(huge.min_z(), -29_999_984.0);
        let small = view(100.0);
        assert_eq!((small.min_x(), small.max_z()), (-50.0, 50.0));
    }

    #[test]
    fn the_closest_distance_is_signed() {
        let border = view(100.0);
        assert_eq!(border.closest_distance(10.0, 0.0), 40.0);
        assert_eq!(border.closest_distance(0.0, -45.0), 5.0);
        assert_eq!(border.closest_distance(60.0, 0.0), -10.0);
    }
}
