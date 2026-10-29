// SPDX-License-Identifier: MIT OR Apache-2.0

//! The swing of a cape: it trails the player and lifts when they move.
//!
//! Source: MCP `EntityPlayer.onUpdate` (the `chasingPos` fields) and
//! `LayerCape.doRenderLayer`. The walking bob term (`cameraYaw`) is zero for
//! other players, whose motion the client never sets, and is left out.

use glam::{DVec3, Vec3};

/// Distance beyond which the chased position jumps to the player
/// (MCP `EntityPlayer.onUpdate`: `d2 = 10`).
const CHASE_JUMP: f64 = 10.0;
/// Share of the gap the chased position closes each tick.
const CHASE_SPEED: f64 = 0.25;

/// The position the cape lags behind (MCP `chasingPosX/Y/Z`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CapeChase {
    /// The chased position now.
    pub current: DVec3,
    /// The chased position one tick ago.
    pub previous: DVec3,
}

impl CapeChase {
    /// Starts at `position`, so a newly spawned player has a still cape.
    #[must_use]
    pub fn at(position: DVec3) -> Self {
        Self {
            current: position,
            previous: position,
        }
    }

    /// Advances one tick towards `position`; a gap beyond 10 blocks on an
    /// axis (a teleport) snaps that axis.
    pub fn update(&mut self, position: DVec3) {
        self.previous = self.current;
        let gap = position - self.current;
        for axis in 0..3 {
            if gap[axis].abs() > CHASE_JUMP {
                self.previous[axis] = position[axis];
                self.current[axis] = position[axis];
            }
        }
        self.current += (position - self.current) * CHASE_SPEED;
    }
}

/// The rotations of the cape, in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CapeAngles {
    /// About the shoulder line (x): the lift of the hem, `6` at rest.
    pub pitch: f32,
    /// About the vertical (z, and y inverted): the sideways sway.
    pub roll: f32,
}

/// The cape angles for a player whose cape trails by `trail` (chased
/// position minus position, interpolated) with the torso at `body_yaw`.
#[must_use]
pub fn cape_angles(trail: Vec3, body_yaw: f32, sneaking: bool) -> CapeAngles {
    let (sin, cos) = body_yaw.to_radians().sin_cos();
    let (forward, side) = (-cos, sin);
    let lift = (trail.y * 10.0).clamp(-6.0, 32.0);
    let streaming = ((trail.x * side + trail.z * forward) * 100.0).max(0.0);
    let sway = (trail.x * forward - trail.z * side) * 100.0;
    let crouch = if sneaking { 25.0 } else { 0.0 };
    CapeAngles {
        pitch: 6.0 + streaming / 2.0 + lift + crouch,
        roll: sway / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chase_closes_a_quarter_of_the_gap_per_tick() {
        let mut chase = CapeChase::at(DVec3::ZERO);
        chase.update(DVec3::new(4.0, 0.0, 0.0));
        assert!((chase.current.x - 1.0).abs() < 1e-9);
        assert_eq!(chase.previous.x, 0.0);
        chase.update(DVec3::new(4.0, 0.0, 0.0));
        assert!((chase.current.x - 1.75).abs() < 1e-9);
    }

    #[test]
    fn a_teleport_snaps_the_chase() {
        let mut chase = CapeChase::at(DVec3::ZERO);
        chase.update(DVec3::new(100.0, 0.0, 0.0));
        assert_eq!(chase.previous.x, 100.0);
        assert_eq!(chase.current.x, 100.0);
        assert_eq!(chase.current.y, 0.0);
    }

    #[test]
    fn a_still_cape_hangs_six_degrees_out() {
        let angles = cape_angles(Vec3::ZERO, 0.0, false);
        assert_eq!(
            angles,
            CapeAngles {
                pitch: 6.0,
                roll: 0.0
            }
        );
    }

    #[test]
    fn crouching_swings_the_hem_out_by_25_more_degrees() {
        assert_eq!(cape_angles(Vec3::ZERO, 0.0, true).pitch, 31.0);
    }

    #[test]
    fn running_forward_streams_the_cape_back() {
        // Facing south (yaw 0) the cape trails to the north: chased minus
        // position is negative z.
        let angles = cape_angles(Vec3::new(0.0, 0.0, -0.2), 0.0, false);
        assert!((angles.pitch - (6.0 + 10.0)).abs() < 1e-3, "{angles:?}");
        assert!(angles.roll.abs() < 1e-3);
    }

    #[test]
    fn walking_backwards_does_not_lift_the_cape() {
        let angles = cape_angles(Vec3::new(0.0, 0.0, 0.2), 0.0, false);
        assert_eq!(angles.pitch, 6.0);
    }

    #[test]
    fn a_sideways_trail_sways_the_cape() {
        let angles = cape_angles(Vec3::new(0.1, 0.0, 0.0), 0.0, false);
        assert!(angles.roll.abs() > 4.0, "{angles:?}");
    }

    #[test]
    fn the_vertical_lift_is_clamped() {
        assert_eq!(
            cape_angles(Vec3::new(0.0, 10.0, 0.0), 0.0, false).pitch,
            38.0
        );
        assert_eq!(
            cape_angles(Vec3::new(0.0, -10.0, 0.0), 0.0, false).pitch,
            0.0
        );
    }
}
