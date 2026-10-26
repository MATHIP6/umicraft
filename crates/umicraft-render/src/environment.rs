// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-frame world environment shared by the sky, fog and terrain passes.
//!
//! The value is plain data: the game thread publishes the world clock and the
//! render thread adds the sub-tick interpolation before handing it to the
//! renderer. Keeping it `Copy` avoids a lock or allocation on the frame path.

/// World clock, dimension and view distance for one rendered frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderEnvironment {
    /// Join Game dimension (`0` overworld, `-1` Nether, `1` End).
    pub dimension: i8,
    /// Time of day in ticks. Negative values mean the server froze the cycle.
    pub world_time: i64,
    /// Interpolation between simulation ticks, in `0.0..1.0`.
    pub partial_tick: f32,
    /// Requested view distance in chunks, used for the fog range.
    pub view_distance: u8,
    /// 1.8 brightness gamma in `0.0..=1.0` (Moody=0 → Bright=1).
    ///
    /// Applied to the lightmap as `1-(1-x)^4`, matching
    /// `EntityRenderer.updateLightmap`.
    pub gamma: f32,
}

impl RenderEnvironment {
    /// A midday overworld frame with the default eight-chunk view distance.
    pub const DEFAULT: Self = Self {
        dimension: 0,
        world_time: 6_000,
        partial_tick: 0.0,
        view_distance: 8,
        gamma: 0.5,
    };

    /// Builds an environment, clamping `partial_tick` to `0.0..1.0`.
    #[must_use]
    pub fn new(dimension: i8, world_time: i64, partial_tick: f32, view_distance: u8) -> Self {
        Self {
            dimension,
            world_time,
            partial_tick: partial_tick.clamp(0.0, 1.0),
            view_distance,
            gamma: 0.5,
        }
    }
}

impl Default for RenderEnvironment {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_tick_is_clamped() {
        assert_eq!(RenderEnvironment::new(0, 0, -1.0, 8).partial_tick, 0.0);
        assert_eq!(RenderEnvironment::new(0, 0, 2.0, 8).partial_tick, 1.0);
        assert_eq!(RenderEnvironment::new(0, 0, 0.25, 8).partial_tick, 0.25);
    }

    #[test]
    fn the_default_is_midday_overworld() {
        assert_eq!(RenderEnvironment::default(), RenderEnvironment::DEFAULT);
    }
}
