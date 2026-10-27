// SPDX-License-Identifier: MIT OR Apache-2.0

//! The first-person camera effects: view bobbing and the field-of-view
//! modifier.
//!
//! The simulation keeps the vanilla inputs of both effects on the
//! [`Player`](crate::Player) (walked distance, smoothed camera sway, FOV
//! modifier, each with its previous-tick value); [`ViewEffects::at`] turns
//! them into the values of one frame. They are plain data so that the render
//! thread can interpolate two frames without game types.

use glam::{DVec3, Mat4, Vec3};

use crate::physics::Player;
use crate::raycast::{look_direction, raycast};
use crate::shapes::BlockSource;

/// The distance of the third-person camera behind the eyes, in blocks
/// (MCP `EntityRenderer.thirdPersonDistance`, line 123).
pub const THIRD_PERSON_DISTANCE: f64 = 4.0;

/// The half-size of the box the third-person camera keeps clear of blocks
/// (MCP `EntityRenderer.orientCamera`, line 679: the eight corners at ±0.1).
const CAMERA_MARGIN: f64 = 0.1;

/// The smoothing factor of the sway amplitude (MCP `EntityPlayer.java:653`).
pub const SWAY_SMOOTHING: f32 = 0.4;

/// The smoothing factor of the vertical sway (MCP `EntityPlayer.java:654`).
pub const TILT_SMOOTHING: f32 = 0.8;

/// The largest horizontal speed that feeds the sway (MCP `EntityPlayer.java:639`).
pub const MAX_SWAY_SPEED: f32 = 0.1;

/// The scale of the vertical-speed tilt in degrees (MCP `EntityPlayer.java:630`).
pub const TILT_DEGREES: f64 = 15.0;

/// The part of the FOV modifier the hand FOV closes each tick
/// (MCP `EntityRenderer.updateFovModifierHand`, line 533).
pub const FOV_SMOOTHING: f32 = 0.5;

/// The bounds of the FOV modifier (MCP `EntityRenderer.java:535-542`).
pub const FOV_MODIFIER_RANGE: (f32, f32) = (0.1, 1.5);

/// The extra FOV factor of a flying player (MCP `AbstractClientPlayer.java:115`).
const FLYING_FOV_FACTOR: f32 = 1.1;

/// Where the camera sits (`F5`, MCP `GameSettings.thirdPersonView`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Perspective {
    /// In the eyes of the player.
    #[default]
    FirstPerson,
    /// Behind the player, looking the same way.
    ThirdPersonBack,
    /// In front of the player, looking back at it.
    ThirdPersonFront,
}

impl Perspective {
    /// The next perspective of the `F5` cycle
    /// (MCP `Minecraft.java`: `thirdPersonView = (thirdPersonView + 1) % 3`).
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::FirstPerson => Self::ThirdPersonBack,
            Self::ThirdPersonBack => Self::ThirdPersonFront,
            Self::ThirdPersonFront => Self::FirstPerson,
        }
    }

    /// Whether the player body is drawn.
    #[must_use]
    pub fn shows_player(self) -> bool {
        self != Self::FirstPerson
    }
}

/// The position and angles the camera renders from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPose {
    /// The camera position.
    pub position: DVec3,
    /// The yaw in radians, in the angles of the game logic.
    pub yaw: f32,
    /// The pitch in radians, in the angles of the game logic.
    pub pitch: f32,
}

/// The pose of the camera for a perspective.
///
/// MCP `EntityRenderer.orientCamera`: the third-person camera moves 4 blocks
/// back along the view (forward in front view, where the view also turns by
/// half a turn and the pitch flips). Eight rays from the corners of a 0.2
/// box shorten the distance when a block is in the way, so the camera never
/// ends up inside a wall.
#[must_use]
pub fn camera_pose<S: BlockSource + ?Sized>(
    perspective: Perspective,
    eye: DVec3,
    yaw: f32,
    pitch: f32,
    world: &S,
) -> CameraPose {
    if perspective == Perspective::FirstPerson {
        return CameraPose {
            position: eye,
            yaw,
            pitch,
        };
    }
    let front = perspective == Perspective::ThirdPersonFront;
    let direction = look_direction(yaw, pitch) * if front { 1.0 } else { -1.0 };
    let distance = clear_distance(eye, direction, world);
    CameraPose {
        position: eye + direction * distance,
        yaw: if front {
            yaw + std::f32::consts::PI
        } else {
            yaw
        },
        pitch: if front { -pitch } else { pitch },
    }
}

/// How far the camera can move from `eye` along `direction` before a block
/// stops one of the corners of its box.
fn clear_distance<S: BlockSource + ?Sized>(eye: DVec3, direction: DVec3, world: &S) -> f64 {
    let mut distance = THIRD_PERSON_DISTANCE;
    for corner in 0..8 {
        let offset = DVec3::new(
            f64::from((corner & 1) * 2 - 1),
            f64::from((corner >> 1 & 1) * 2 - 1),
            f64::from((corner >> 2 & 1) * 2 - 1),
        ) * CAMERA_MARGIN;
        let target = direction * THIRD_PERSON_DISTANCE;
        if let Some(hit) = raycast(eye + offset, target, THIRD_PERSON_DISTANCE, world) {
            distance = distance.min(hit.position.distance(eye));
        }
    }
    distance
}

/// The camera sway of a walking player, as the vanilla view-bobbing matrix
/// parts (MCP `EntityRenderer.setupViewBobbing`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ViewBob {
    /// The sideways shift of the camera, in blocks.
    pub shift_x: f32,
    /// The vertical shift of the camera, in blocks (never positive).
    pub shift_y: f32,
    /// The roll around the view axis, in degrees.
    pub roll_degrees: f32,
    /// The pitch added to the view, in degrees.
    pub pitch_degrees: f32,
}

impl ViewBob {
    /// The bobbing at `walk_distance` blocks walked, with the given sway
    /// amplitude and vertical tilt (MCP `EntityRenderer.java:618-625`).
    #[must_use]
    pub fn new(walk_distance: f32, sway: f32, tilt: f32) -> Self {
        let phase = -walk_distance * std::f32::consts::PI;
        Self {
            shift_x: phase.sin() * sway * 0.5,
            shift_y: -(phase.cos() * sway).abs(),
            roll_degrees: phase.sin() * sway * 3.0,
            pitch_degrees: ((phase - 0.2).cos() * sway).abs() * 5.0 + tilt,
        }
    }

    /// The transform applied to the camera space after the view rotation.
    ///
    /// The order is the one of the successive `GlStateManager` calls:
    /// translation, roll, then the two pitches.
    #[must_use]
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_translation(Vec3::new(self.shift_x, self.shift_y, 0.0))
            * Mat4::from_rotation_z(self.roll_degrees.to_radians())
            * Mat4::from_rotation_x(self.pitch_degrees.to_radians())
    }

    /// The component-wise blend of two bobbings.
    #[must_use]
    pub fn lerp(self, other: Self, alpha: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * alpha;
        Self {
            shift_x: mix(self.shift_x, other.shift_x),
            shift_y: mix(self.shift_y, other.shift_y),
            roll_degrees: mix(self.roll_degrees, other.roll_degrees),
            pitch_degrees: mix(self.pitch_degrees, other.pitch_degrees),
        }
    }
}

/// The effects of the first-person camera for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewEffects {
    /// The factor applied to the field of view (`1.0` is the base FOV).
    pub fov_modifier: f32,
    /// The view bobbing.
    pub bob: ViewBob,
}

impl Default for ViewEffects {
    fn default() -> Self {
        Self {
            fov_modifier: 1.0,
            bob: ViewBob::default(),
        }
    }
}

impl ViewEffects {
    /// The effects of `player` between the previous tick (`alpha = 0`) and
    /// the current one (`alpha = 1`).
    #[must_use]
    pub fn at(player: &Player, alpha: f32) -> Self {
        let mix = |previous: f32, current: f32| previous + (current - previous) * alpha;
        Self {
            fov_modifier: mix(player.prev_fov_modifier, player.fov_modifier),
            bob: ViewBob::new(
                mix(player.prev_walk_distance, player.walk_distance),
                mix(player.prev_camera_sway, player.camera_sway),
                mix(player.prev_camera_tilt, player.camera_tilt),
            ),
        }
    }

    /// The blend of two frames of effects.
    #[must_use]
    pub fn lerp(self, other: Self, alpha: f32) -> Self {
        Self {
            fov_modifier: self.fov_modifier + (other.fov_modifier - self.fov_modifier) * alpha,
            bob: self.bob.lerp(other.bob, alpha),
        }
    }
}

/// The FOV modifier a player aims for.
///
/// MCP `AbstractClientPlayer.getFovModifier`: 1.1 while flying, times the
/// mean of 1 and the ratio of the movement speed to the walking speed (a
/// sprint adds 30 % to the speed).
#[must_use]
pub fn target_fov_modifier(flying: bool, speed_ratio: f64) -> f32 {
    let flight = if flying { FLYING_FOV_FACTOR } else { 1.0 };
    flight * ((speed_ratio as f32 + 1.0) / 2.0)
}

/// Moves a FOV modifier towards its target for one tick.
#[must_use]
pub fn smooth_fov_modifier(current: f32, target: f32) -> f32 {
    (current + (target - current) * FOV_SMOOTHING).clamp(FOV_MODIFIER_RANGE.0, FOV_MODIFIER_RANGE.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Blocks(std::collections::HashSet<(i32, i32, i32)>);

    impl BlockSource for Blocks {
        fn block(&self, x: i32, y: i32, z: i32) -> umicraft_world::BlockState {
            if self.0.contains(&(x, y, z)) {
                umicraft_world::BlockState::new(1, 0)
            } else {
                umicraft_world::BlockState::AIR
            }
        }
    }

    #[test]
    fn f5_cycles_through_the_three_perspectives() {
        let mut perspective = Perspective::default();
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push(perspective);
            perspective = perspective.next();
        }
        assert_eq!(seen[0], seen[3]);
        assert_ne!(seen[0], seen[1]);
        assert_ne!(seen[1], seen[2]);
        assert!(!Perspective::FirstPerson.shows_player());
        assert!(Perspective::ThirdPersonBack.shows_player());
    }

    #[test]
    fn the_first_person_pose_is_the_eye() {
        let eye = DVec3::new(1.0, 65.0, 2.0);
        let pose = camera_pose(Perspective::FirstPerson, eye, 0.3, 0.2, &Blocks::default());
        assert_eq!(pose.position, eye);
        assert_eq!((pose.yaw, pose.pitch), (0.3, 0.2));
    }

    #[test]
    fn the_back_view_sits_four_blocks_behind_the_eyes() {
        // Yaw zero looks towards +Z, so the camera is at -Z.
        let eye = DVec3::new(0.5, 65.0, 0.5);
        let pose = camera_pose(
            Perspective::ThirdPersonBack,
            eye,
            0.0,
            0.0,
            &Blocks::default(),
        );
        assert!((pose.position - DVec3::new(0.5, 65.0, -3.5)).length() < 1e-9);
        assert_eq!((pose.yaw, pose.pitch), (0.0, 0.0));
    }

    #[test]
    fn the_front_view_looks_back_at_the_player() {
        let eye = DVec3::new(0.5, 65.0, 0.5);
        let pose = camera_pose(
            Perspective::ThirdPersonFront,
            eye,
            0.0,
            0.2,
            &Blocks::default(),
        );
        assert!(pose.position.z > eye.z + 3.0);
        assert!((pose.yaw - std::f32::consts::PI).abs() < 1e-6);
        assert!((pose.pitch + 0.2).abs() < 1e-6);
    }

    #[test]
    fn a_wall_pulls_the_third_person_camera_in() {
        let eye = DVec3::new(0.5, 65.5, 0.5);
        let mut world = Blocks::default();
        for y in 60..70 {
            for x in -3..4 {
                world.0.insert((x, y, -2));
            }
        }
        let pose = camera_pose(Perspective::ThirdPersonBack, eye, 0.0, 0.0, &world);
        assert!(pose.position.z > -1.1, "z = {}", pose.position.z);
        assert!(pose.position.z < eye.z);
    }

    #[test]
    fn a_standing_player_has_no_bobbing() {
        let bob = ViewBob::new(12.3, 0.0, 0.0);
        assert_eq!(bob.shift_x, 0.0);
        assert_eq!(bob.shift_y, 0.0);
        assert_eq!(bob.roll_degrees, 0.0);
        assert_eq!(bob.pitch_degrees, 0.0);
    }

    #[test]
    fn the_camera_never_rises_above_the_eye() {
        for step in 0..40 {
            let bob = ViewBob::new(step as f32 * 0.13, 0.1, 0.0);
            assert!(bob.shift_y <= 0.0);
            assert!(bob.shift_x.abs() <= 0.05 + 1e-6);
            assert!(bob.roll_degrees.abs() <= 0.3 + 1e-6);
        }
    }

    #[test]
    fn the_bobbing_matches_the_reference_formula() {
        // MCP `EntityRenderer.java:622-625` at a quarter step.
        let bob = ViewBob::new(0.25, 0.1, 2.0);
        let phase = -0.25 * std::f32::consts::PI;
        assert!((bob.shift_x - phase.sin() * 0.05).abs() < 1e-6);
        assert!((bob.shift_y + (phase.cos() * 0.1).abs()).abs() < 1e-6);
        assert!((bob.pitch_degrees - ((phase - 0.2).cos() * 0.1).abs() * 5.0 - 2.0).abs() < 1e-5);
    }

    #[test]
    fn the_identity_bob_is_the_identity_matrix() {
        assert!(ViewBob::default()
            .matrix()
            .abs_diff_eq(Mat4::IDENTITY, 1e-6));
    }

    #[test]
    fn sprinting_widens_the_field_of_view() {
        let walking = target_fov_modifier(false, 1.0);
        let sprinting = target_fov_modifier(false, 1.3);
        assert_eq!(walking, 1.0);
        assert!((sprinting - 1.15).abs() < 1e-6);
        assert!(target_fov_modifier(true, 1.0) > walking);
    }

    #[test]
    fn the_modifier_converges_by_halves_within_its_bounds() {
        assert!((smooth_fov_modifier(1.0, 1.15) - 1.075).abs() < 1e-6);
        assert_eq!(smooth_fov_modifier(1.0, 10.0), 1.5);
        assert_eq!(smooth_fov_modifier(1.0, -10.0), 0.1);
    }

    #[test]
    fn the_effects_blend_between_two_ticks() {
        let a = ViewEffects::default();
        let b = ViewEffects {
            fov_modifier: 1.2,
            bob: ViewBob::new(0.3, 0.1, 0.0),
        };
        assert!((a.lerp(b, 0.5).fov_modifier - 1.1).abs() < 1e-6);
        assert_eq!(a.lerp(b, 1.0), b);
    }
}
