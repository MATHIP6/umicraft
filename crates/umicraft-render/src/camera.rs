// SPDX-License-Identifier: MIT OR Apache-2.0

//! Free-flying camera and projection.

use glam::{Mat4, Vec3};

/// Vertical field of view in radians (70°).
const DEFAULT_FOV: f32 = 70.0_f32 * std::f32::consts::PI / 180.0;
const NEAR: f32 = 0.1;
const FAR: f32 = 512.0;
/// The near plane of the hand projection (MCP `EntityRenderer.java:1236`).
const HAND_NEAR: f32 = 0.05;
const HAND_FAR: f32 = 16.0;
/// The part of the depth range the hand is squeezed into.
pub const HAND_DEPTH_RANGE: f32 = 1.0e-4;
const MAX_PITCH: f32 = 89.0_f32 * std::f32::consts::PI / 180.0;

/// Perspective camera with a Minecraft-style yaw/pitch.
///
/// `yaw = 0` looks toward −Z, positive yaw turns toward +X (east); positive
/// pitch looks up. [`Camera::from_minecraft`] converts the server's
/// degrees-clockwise-from-south convention.
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// Eye position in world coordinates.
    pub position: Vec3,
    /// Horizontal rotation in radians.
    pub yaw: f32,
    /// Vertical rotation in radians, clamped to ±89°.
    pub pitch: f32,
    fov_y: f32,
    aspect: f32,
    /// The factor applied to the field of view (sprint, flight).
    fov_modifier: f32,
    /// The view bobbing, applied in camera space after the view rotation.
    view_bob: Mat4,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 80.0, 0.0),
            yaw: 0.0,
            pitch: -0.15,
            fov_y: DEFAULT_FOV,
            aspect: 16.0 / 9.0,
            fov_modifier: 1.0,
            view_bob: Mat4::IDENTITY,
        }
    }
}

impl Camera {
    /// Creates a camera at `position` with the given yaw/pitch in radians.
    #[must_use]
    pub fn new(position: Vec3, yaw: f32, pitch: f32) -> Self {
        Self {
            position,
            yaw,
            pitch: pitch.clamp(-MAX_PITCH, MAX_PITCH),
            ..Self::default()
        }
    }

    /// Builds a camera from a server placement (feet position and
    /// degrees-clockwise-from-south yaw / positive-down pitch) and puts the
    /// eye at the vanilla 1.62-block eye height.
    #[must_use]
    pub fn from_minecraft(x: f64, y: f64, z: f64, yaw_degrees: f32, pitch_degrees: f32) -> Self {
        Self::new(
            Vec3::new(x as f32, y as f32 + 1.62, z as f32),
            yaw_degrees.to_radians() + std::f32::consts::PI,
            -pitch_degrees.to_radians(),
        )
    }

    /// Sets the first-person effects of the frame.
    ///
    /// `fov_modifier` scales the base field of view (MCP
    /// `EntityRenderer.getFOVModifier`); `view_bob` is the camera-space
    /// transform of `EntityRenderer.setupViewBobbing`.
    pub fn set_view_effects(&mut self, fov_modifier: f32, view_bob: Mat4) {
        self.fov_modifier = fov_modifier;
        self.view_bob = view_bob;
    }

    /// Updates the aspect ratio (viewport width / height).
    pub fn set_aspect(&mut self, width: u32, height: u32) {
        self.aspect = width.max(1) as f32 / height.max(1) as f32;
    }

    /// Current aspect ratio.
    #[must_use]
    pub fn aspect(&self) -> f32 {
        self.aspect
    }

    /// Unit horizontal forward direction (ignores pitch).
    #[must_use]
    pub fn horizontal_forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos())
    }

    /// Unit horizontal right direction.
    #[must_use]
    pub fn horizontal_right(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin())
    }

    /// Full look direction including pitch.
    #[must_use]
    pub fn forward(&self) -> Vec3 {
        let cos = self.pitch.cos();
        Vec3::new(
            self.yaw.sin() * cos,
            self.pitch.sin(),
            -self.yaw.cos() * cos,
        )
    }

    /// Rotates by a mouse delta in radians and clamps the pitch.
    pub fn look(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw = (self.yaw + delta_yaw).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + delta_pitch).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Moves in the camera's local frame: `forward` along the horizontal
    /// view, `right` to the side, `up` along world Y. Inputs are normalised
    /// together, so diagonals are not faster.
    pub fn move_local(&mut self, forward: f32, right: f32, up: f32, speed: f32, dt: f32) {
        let direction =
            self.horizontal_forward() * forward + self.horizontal_right() * right + Vec3::Y * up;
        if let Some(direction) = direction.try_normalize() {
            self.position += direction * speed * dt;
        }
    }

    /// The projection of the first-person hand.
    ///
    /// MCP `EntityRenderer.renderHand` uses the base field of view (no
    /// sprint or flight modifier), a near plane of 0.05 and clears the depth
    /// buffer first. The depth is instead squeezed into the first
    /// [`HAND_DEPTH_RANGE`] of the range, so the hand always wins the depth
    /// test against the world while its own parts still sort correctly.
    #[must_use]
    pub fn hand_projection(&self) -> Mat4 {
        let projection = glam::camera::rh::proj::directx::perspective(
            self.fov_y,
            self.aspect,
            HAND_NEAR,
            HAND_FAR,
        );
        Mat4::from_scale(Vec3::new(1.0, 1.0, HAND_DEPTH_RANGE)) * projection
    }

    /// The view bobbing transform of this frame (identity when still).
    #[must_use]
    pub fn view_bob(&self) -> Mat4 {
        self.view_bob
    }

    /// View-projection matrix for wgpu's 0..1 depth range.
    #[must_use]
    pub fn view_projection(&self) -> Mat4 {
        // WebGPU NDC is Y-up with Z in 0..1, i.e. the DirectX convention.
        let view = self.view_bob
            * glam::camera::rh::view::look_to_mat4(self.position, self.forward(), Vec3::Y);
        let projection = glam::camera::rh::proj::directx::perspective(
            self.fov_y * self.fov_modifier,
            self.aspect,
            NEAR,
            FAR,
        );
        projection * view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaw_zero_looks_north_and_minecraft_yaw_zero_south() {
        let camera = Camera::new(Vec3::ZERO, 0.0, 0.0);
        assert!((camera.forward() - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-6);

        let placed = Camera::from_minecraft(0.0, 64.0, 0.0, 0.0, 0.0);
        assert!((placed.forward() - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-5);
        assert!((placed.position.y - 65.62).abs() < 1e-5);

        let west = Camera::from_minecraft(0.0, 64.0, 0.0, 90.0, 0.0);
        assert!((west.forward() - Vec3::new(-1.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn pitch_is_clamped() {
        let mut camera = Camera::default();
        camera.look(0.0, 10.0);
        assert!((camera.pitch - MAX_PITCH).abs() < 1e-6);
        camera.look(0.0, -20.0);
        assert!((camera.pitch + MAX_PITCH).abs() < 1e-6);
    }

    #[test]
    fn movement_is_normalised_diagonally() {
        let mut camera = Camera::new(Vec3::ZERO, 0.0, 0.0);
        camera.move_local(1.0, 1.0, 0.0, 10.0, 1.0);
        assert!((camera.position.length() - 10.0).abs() < 1e-5);
    }

    #[test]
    fn a_wider_field_of_view_shrinks_the_projected_offset() {
        let point = Vec3::new(3.0, 0.0, -10.0);
        let project = |camera: &Camera| camera.view_projection().project_point3(point).x;
        let mut camera = Camera::new(Vec3::ZERO, 0.0, 0.0);
        let normal = project(&camera);
        camera.set_view_effects(1.15, Mat4::IDENTITY);
        assert!(project(&camera) < normal);
    }

    #[test]
    fn the_view_bob_moves_the_scene_in_camera_space() {
        let mut camera = Camera::new(Vec3::ZERO, 0.0, 0.0);
        let point = Vec3::new(0.0, 0.0, -10.0);
        let before = camera.view_projection().project_point3(point);
        camera.set_view_effects(1.0, Mat4::from_translation(Vec3::new(0.0, -0.05, 0.0)));
        let after = camera.view_projection().project_point3(point);
        assert!(after.y < before.y, "the camera dips, the scene rises");
        // The eye position itself does not move.
        assert_eq!(camera.position, Vec3::ZERO);
    }

    #[test]
    fn the_hand_stays_in_front_of_any_world_depth() {
        let camera = Camera::default();
        // A point at the far plane of the hand still has a tiny depth.
        let far = camera
            .hand_projection()
            .project_point3(Vec3::new(0.0, 0.0, -10.0));
        assert!(far.z >= 0.0 && far.z <= HAND_DEPTH_RANGE, "{}", far.z);
        // The world's depth for a block 1 m away is far above that.
        let world = camera
            .view_projection()
            .project_point3(camera.position + camera.forward());
        assert!(world.z > HAND_DEPTH_RANGE * 10.0);
    }

    #[test]
    fn projection_is_finite() {
        let camera = Camera::default();
        assert!(camera.view_projection().is_finite());
    }
}
