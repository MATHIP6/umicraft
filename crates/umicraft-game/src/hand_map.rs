// SPDX-License-Identifier: MIT OR Apache-2.0

//! The pose of a filled map in the first-person hand.
//!
//! Port of MCP `ItemRenderer.renderItemMap`: the map is held in both hands
//! (`renderPlayerArms`), tilts with the view pitch (`func_178100_c`) and is
//! drawn on a 128 unit grid scaled to `0.38 * 1/64` blocks per unit.

use glam::{Mat4, Vec3};

use crate::hand::{rotate, translate, HandPose};

/// The scale of the held map item.
const MAP_SCALE: f32 = 0.38;
/// One map unit: the 128 pixel grid is scaled by `1/64`.
const GRID_UNIT: f32 = 0.015_625;

impl HandPose {
    /// The transform shared by the map and the arms that hold it: the sway,
    /// the swing shift, the equip drop and the tilt with the view pitch.
    fn map_base(&self) -> Mat4 {
        let lift = map_lift(self.view_pitch);
        rotate(self.sway_pitch, Vec3::X)
            * rotate(self.sway_yaw, Vec3::Y)
            * self.swing_shift()
            * translate(0.0, 0.04, -0.72)
            * translate(0.0, self.equip * -1.2, 0.0)
            * translate(0.0, lift * -0.5, 0.0)
            * rotate(90.0, Vec3::Y)
            * rotate(lift * -85.0, Vec3::Z)
    }

    /// The transforms from the arm model (vanilla model space) to the camera
    /// space for the right and the left arm holding a map.
    ///
    /// MCP `renderPlayerArms`, `renderRightArm` and `renderLeftArm`.
    #[must_use]
    pub fn map_arm_matrices(&self) -> [Mat4; 2] {
        let base = self.map_base();
        [
            base * rotate(54.0, Vec3::Y)
                * rotate(64.0, Vec3::X)
                * rotate(-62.0, Vec3::Z)
                * translate(0.25, -0.85, 0.75),
            base * rotate(92.0, Vec3::Y)
                * rotate(45.0, Vec3::X)
                * rotate(41.0, Vec3::Z)
                * translate(-0.3, -1.1, 0.45),
        ]
    }

    /// The transform from the map grid (units, x right and y down, 128 wide,
    /// the background from -7 to 135) to the camera space.
    #[must_use]
    pub fn map_matrix(&self) -> Mat4 {
        let swing = self.swing;
        let turn = (swing * swing * std::f32::consts::PI).sin();
        let roll = (swing.sqrt() * std::f32::consts::PI).sin();
        self.map_base()
            * rotate(turn * -20.0, Vec3::Y)
            * rotate(roll * -20.0, Vec3::Z)
            * rotate(roll * -80.0, Vec3::X)
            * Mat4::from_scale(Vec3::splat(MAP_SCALE))
            * rotate(90.0, Vec3::Y)
            * rotate(180.0, Vec3::Z)
            * translate(-1.0, -1.0, 0.0)
            * Mat4::from_scale(Vec3::splat(GRID_UNIT))
    }
}

/// MCP `ItemRenderer.func_178100_c`: how far the map is raised towards the
/// face when the player looks up, `0` to `1`.
fn map_lift(pitch: f32) -> f32 {
    let t = (1.0 - pitch / 45.0 + 0.1).clamp(0.0, 1.0);
    -(t * std::f32::consts::PI).cos() * 0.5 + 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_rest() -> HandPose {
        HandPose {
            visible: true,
            ..HandPose::default()
        }
    }

    #[test]
    fn looking_up_lifts_the_map_and_looking_down_leaves_it_low() {
        assert!((map_lift(-90.0) - 1.0).abs() < 1e-6);
        assert!(map_lift(50.0).abs() < 1e-6);
        assert!(map_lift(0.0) > map_lift(20.0));
    }

    #[test]
    fn the_map_is_a_few_tenths_of_a_block_in_front_of_the_camera() {
        let pose = at_rest();
        let centre = pose
            .map_matrix()
            .transform_point3(Vec3::new(64.0, 64.0, 0.0));
        assert!(centre.z < -0.4 && centre.z > -1.0, "{centre:?}");
        assert!(centre.x.abs() < 0.5, "{centre:?}");
    }

    #[test]
    fn the_128_unit_map_is_about_a_third_of_a_block_wide() {
        let pose = at_rest();
        let matrix = pose.map_matrix();
        let a = matrix.transform_point3(Vec3::new(0.0, 64.0, 0.0));
        let b = matrix.transform_point3(Vec3::new(128.0, 64.0, 0.0));
        let width = a.distance(b);
        assert!(
            (width - 128.0 * MAP_SCALE * GRID_UNIT).abs() < 1e-3,
            "{width}"
        );
    }

    #[test]
    fn lowering_the_hand_drops_the_map() {
        let up = at_rest().map_matrix().transform_point3(Vec3::ZERO);
        let down = HandPose {
            equip: 1.0,
            ..at_rest()
        }
        .map_matrix()
        .transform_point3(Vec3::ZERO);
        assert!(down.y < up.y - 1.0);
    }

    #[test]
    fn both_arms_hold_the_map_one_on_each_side() {
        let [right, left] = at_rest().map_arm_matrices();
        let r = right.transform_point3(Vec3::ZERO);
        let l = left.transform_point3(Vec3::ZERO);
        assert!(r.x > l.x, "{r:?} {l:?}");
    }
}
