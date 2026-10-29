// SPDX-License-Identifier: MIT OR Apache-2.0

//! Degree arithmetic shared by the entity animation code.

/// Wraps an angle in degrees into `[-180, 180)`.
///
/// MCP `MathHelper.wrapAngleTo180_float`.
#[must_use]
pub fn wrap_degrees(angle: f32) -> f32 {
    let mut wrapped = angle % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

/// Interpolates between two angles in degrees along the shortest arc.
///
/// MCP `RendererLivingEntity.interpolateRotation`.
#[must_use]
pub fn lerp_degrees(previous: f32, current: f32, alpha: f32) -> f32 {
    previous + wrap_degrees(current - previous) * alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_maps_into_the_half_open_range() {
        assert_eq!(wrap_degrees(180.0), -180.0);
        assert_eq!(wrap_degrees(-180.0), -180.0);
        assert_eq!(wrap_degrees(190.0), -170.0);
        assert_eq!(wrap_degrees(-190.0), 170.0);
        assert_eq!(wrap_degrees(725.0), 5.0);
    }

    #[test]
    fn interpolation_takes_the_short_way_round() {
        assert!((lerp_degrees(170.0, -170.0, 0.5) - 180.0).abs() < 1e-4);
        assert!((lerp_degrees(-170.0, 170.0, 0.5) + 180.0).abs() < 1e-4);
    }
}
