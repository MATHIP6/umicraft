// SPDX-License-Identifier: MIT OR Apache-2.0

//! Free-flying camera and projection.

use glam::{DVec3, Mat4, Vec3, Vec4};

/// Vertical field of view in radians (70°).
const DEFAULT_FOV: f32 = 70.0_f32 * std::f32::consts::PI / 180.0;
pub(crate) const NEAR: f32 = 0.1;
pub(crate) const FAR: f32 = 512.0;
/// The near plane of the hand projection (MCP `EntityRenderer.java:1236`).
const HAND_NEAR: f32 = 0.05;
/// The field of view of the hand: fixed, whatever the "FOV" option says
/// (MCP `EntityRenderer.java:842` asks `getFOVModifier(partialTicks, false)`,
/// which starts from `70.0F`, and `:558`).
const HAND_FOV: f32 = DEFAULT_FOV;
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
    /// Eye position in world coordinates, in f64: an f32 moves in 6-12 cm
    /// steps around x = 1 000 000 (F2-1a), so it is only ever cast after the
    /// render origin is subtracted.
    eye: DVec3,
    /// A fixed render origin, or `None` for the usual `floor(eye)`.
    pinned_origin: Option<DVec3>,
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
            eye: DVec3::new(0.0, 80.0, 0.0),
            pinned_origin: None,
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
            eye: position.as_dvec3(),
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
        let mut camera = Self::new(
            Vec3::ZERO,
            yaw_degrees.to_radians() + std::f32::consts::PI,
            -pitch_degrees.to_radians(),
        );
        camera.eye = DVec3::new(x, y + 1.62, z);
        camera
    }

    /// The eye in world coordinates, at full precision.
    #[must_use]
    pub fn eye(&self) -> DVec3 {
        self.eye
    }

    /// Places the eye. Prefer this over [`Camera::set_position`] whenever
    /// the position comes from the game, which works in f64.
    pub fn set_eye(&mut self, eye: DVec3) {
        self.eye = eye;
    }

    /// The eye rounded to f32. Exact near the world origin; around
    /// x = 1 000 000 it has a 6 cm grain, so use it only for things that do
    /// not need a smooth position (sound, a rough distance).
    #[must_use]
    pub fn position(&self) -> Vec3 {
        self.eye.as_vec3()
    }

    /// Places the eye from an f32 position (tests, demos, free flight).
    pub fn set_position(&mut self, position: Vec3) {
        self.eye = position.as_dvec3();
    }

    /// The integer block corner that every GPU position is made relative to.
    ///
    /// It is a whole number below 2^24, so it is exact in f32 and the
    /// subtraction in the shader loses nothing; the matrix carries only the
    /// small remainder [`Camera::relative_eye`].
    #[must_use]
    pub fn render_origin(&self) -> DVec3 {
        self.pinned_origin.unwrap_or_else(|| self.eye.floor())
    }

    /// Fixes the render origin instead of following the eye.
    ///
    /// For scenes built in world coordinates near the world origin (the
    /// acceptance demos and tests): with `DVec3::ZERO` their geometry stays
    /// valid as it is. The game never pins it.
    pub fn pin_render_origin(&mut self, origin: DVec3) {
        self.pinned_origin = Some(origin);
    }

    /// The eye relative to [`Camera::render_origin`]: within 0..1 on each
    /// axis (unless the origin is pinned), computed in f64 before the cast.
    #[must_use]
    pub fn relative_eye(&self) -> Vec3 {
        (self.eye - self.render_origin()).as_vec3()
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

    /// Sets the vertical field of view, in degrees. A value outside
    /// `1..=170` (or not a number) is ignored so the projection stays valid.
    pub fn set_fov_degrees(&mut self, degrees: f32) {
        if (1.0..=170.0).contains(&degrees) {
            self.fov_y = degrees.to_radians();
        }
    }

    /// The vertical field of view the projection uses, in radians: the base one
    /// times the factor of sprinting or flying.
    #[must_use]
    pub(crate) fn effective_fov(&self) -> f32 {
        self.fov_y * self.fov_modifier
    }

    /// The vertical field of view, in degrees.
    #[must_use]
    pub fn fov_degrees(&self) -> f32 {
        self.fov_y.to_degrees()
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
            self.eye += (direction * speed * dt).as_dvec3();
        }
    }

    /// The projection of the first-person hand.
    ///
    /// MCP `EntityRenderer.renderHand` uses a fixed 70 degree field of view
    /// (not the "FOV" option, no sprint or flight modifier), a near plane of
    /// 0.05 and clears the depth buffer first. The depth is instead squeezed
    /// into the first
    /// [`HAND_DEPTH_RANGE`] of the range, so the hand always wins the depth
    /// test against the world while its own parts still sort correctly.
    #[must_use]
    pub fn hand_projection(&self) -> Mat4 {
        let projection = glam::camera::rh::proj::directx::perspective(
            HAND_FOV,
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

    /// View-projection matrix for wgpu's 0..1 depth range, for points in
    /// absolute world coordinates. Only precise near the world origin: the
    /// renderer uses [`Camera::view_projection_from`].
    #[must_use]
    pub fn view_projection(&self) -> Mat4 {
        self.view_projection_from(DVec3::ZERO)
    }

    /// The projection and the view rotation without the eye translation, for
    /// things drawn around the eye (sky, clouds, world border), which the
    /// camera never gets closer to or farther from.
    ///
    /// Built from the relative eye so that no large number is multiplied.
    #[must_use]
    pub fn view_projection_around_eye(&self) -> Mat4 {
        self.view_projection_from(self.render_origin())
            * Mat4::from_translation(self.relative_eye())
    }

    /// The view-projection for points given relative to `origin`.
    ///
    /// Far from the world origin an f32 translation in the matrix is too
    /// coarse (about 0.002 blocks at x = 30000) and every vertex rounds
    /// differently, so the blocks shimmer. Shaders subtract the integer
    /// `origin` from each position instead (exact in f32) and the matrix only
    /// carries the small remainder `eye - origin`, subtracted in f64 so the
    /// eye keeps its full precision (F2-1a).
    #[must_use]
    pub fn view_projection_from(&self, origin: DVec3) -> Mat4 {
        // WebGPU NDC is Y-up with Z in 0..1, i.e. the DirectX convention.
        let view = self.view_bob
            * glam::camera::rh::view::look_to_mat4(
                (self.eye - origin).as_vec3(),
                self.forward(),
                Vec3::Y,
            );
        let projection = glam::camera::rh::proj::directx::perspective(
            self.fov_y * self.fov_modifier,
            self.aspect,
            NEAR,
            FAR,
        );
        projection * view
    }
}

/// The six clipping planes of a view-projection, for culling boxes the
/// camera cannot see.
///
/// Extracted with the Gribb–Hartmann method for wgpu's `0..1` depth range
/// (near plane is row 2 alone). The vanilla client culls render chunks against
/// its frustum too (`ViewFrustum`/`ICamera`), so drawing only what intersects
/// it changes no pixel.
#[derive(Debug, Clone, Copy)]
pub struct Frustum {
    /// `(a, b, c, d)` with `ax + by + cz + d >= 0` inside, normals pointing in.
    planes: [Vec4; 6],
}

impl Frustum {
    /// Builds the frustum of `view_projection` (world space in, clip out).
    #[must_use]
    pub fn from_view_projection(view_projection: Mat4) -> Self {
        let [r0, r1, r2, r3] = [0, 1, 2, 3].map(|row| view_projection.row(row));
        Self {
            planes: [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2],
        }
    }

    /// False only when the axis-aligned box lies entirely outside a plane.
    ///
    /// Conservative: a box near a frustum corner may be reported as visible.
    #[must_use]
    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        self.planes.iter().all(|plane| {
            // The corner farthest along the plane normal decides.
            let corner = Vec3::new(
                if plane.x >= 0.0 { max.x } else { min.x },
                if plane.y >= 0.0 { max.y } else { min.y },
                if plane.z >= 0.0 { max.z } else { min.z },
            );
            plane.truncate().dot(corner) + plane.w >= 0.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frustum_keeps_boxes_ahead_and_drops_boxes_behind() {
        let camera = Camera::new(Vec3::ZERO, 0.0, 0.0);
        let frustum = Frustum::from_view_projection(camera.view_projection());
        // Yaw 0 looks toward -Z.
        assert!(frustum.intersects_aabb(Vec3::new(-1.0, -1.0, -20.0), Vec3::new(1.0, 1.0, -18.0)));
        assert!(!frustum.intersects_aabb(Vec3::new(-1.0, -1.0, 18.0), Vec3::new(1.0, 1.0, 20.0)));
        assert!(
            !frustum.intersects_aabb(Vec3::new(500.0, -1.0, -20.0), Vec3::new(502.0, 1.0, -18.0))
        );
    }

    #[test]
    fn yaw_zero_looks_north_and_minecraft_yaw_zero_south() {
        let camera = Camera::new(Vec3::ZERO, 0.0, 0.0);
        assert!((camera.forward() - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-6);

        let placed = Camera::from_minecraft(0.0, 64.0, 0.0, 0.0, 0.0);
        assert!((placed.forward() - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-5);
        assert!((placed.position().y - 65.62).abs() < 1e-5);

        let west = Camera::from_minecraft(0.0, 64.0, 0.0, 90.0, 0.0);
        assert!((west.forward() - Vec3::new(-1.0, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn a_relative_projection_matches_the_absolute_one_and_stays_exact_far_away() {
        let mut camera = Camera::new(Vec3::ZERO, 0.7, -0.2);
        camera.set_eye(DVec3::new(29_993.4, 65.6, -8.3));
        let origin = camera.render_origin();
        let point = DVec3::new(29_996.0, 64.0, -12.0);
        let relative = camera
            .view_projection_from(origin)
            .project_point3((point - origin).as_vec3());
        // The same point seen from a camera near the world origin.
        let near = Camera::new(camera.relative_eye(), 0.7, -0.2);
        let expected = near
            .view_projection()
            .project_point3((point - origin).as_vec3());
        assert!((relative - expected).abs().max_element() < 1e-6);
    }

    #[test]
    fn a_small_move_far_from_the_origin_changes_the_view() {
        // F2-1d: at x = 1 000 000.5 an f32 eye has a 6 cm grain, so a 1 cm
        // step used to vanish and the camera moved in jumps.
        let mut camera = Camera::new(Vec3::ZERO, 0.0, 0.0);
        camera.set_eye(DVec3::new(1_000_000.5, 65.0, 1_000_000.5));
        let before = camera.view_projection_from(camera.render_origin());
        camera.set_eye(DVec3::new(1_000_000.51, 65.0, 1_000_000.5));
        let after = camera.view_projection_from(camera.render_origin());
        assert_ne!(before, after);
        // A point 10 blocks ahead shifts by the step: 0.01 block at distance
        // 10 is well above f32 noise in NDC.
        let point = Vec3::new(0.5, 0.5, -9.5);
        let shift = before.project_point3(point).x - after.project_point3(point).x;
        assert!(shift > 1e-4, "{shift}");
    }

    #[test]
    fn the_relative_eye_is_a_small_exact_remainder() {
        let mut camera = Camera::new(Vec3::ZERO, 0.0, 0.0);
        camera.set_eye(DVec3::new(-1_000_000.25, 64.5, 30_000_000.75));
        assert_eq!(
            camera.render_origin(),
            DVec3::new(-1_000_001.0, 64.0, 30_000_000.0)
        );
        assert_eq!(camera.relative_eye(), Vec3::new(0.75, 0.5, 0.75));
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
        assert!((camera.position().length() - 10.0).abs() < 1e-5);
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
        assert_eq!(camera.position(), Vec3::ZERO);
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
            .project_point3(camera.position() + camera.forward());
        assert!(world.z > HAND_DEPTH_RANGE * 10.0);
    }

    /// The hand keeps its 70 degrees whatever the "FOV" option is, as in the
    /// reference: a wide FOV only widens the world.
    #[test]
    fn the_hand_ignores_the_fov_option() {
        let mut camera = Camera::default();
        let point = Vec3::new(0.3, -0.2, -1.0);
        let before = camera.hand_projection().project_point3(point);
        let world_before = camera.view_projection().project_point3(point);
        camera.set_fov_degrees(110.0);
        assert_eq!(camera.hand_projection().project_point3(point), before);
        assert_ne!(camera.view_projection().project_point3(point), world_before);
    }

    #[test]
    fn projection_is_finite() {
        let camera = Camera::default();
        assert!(camera.view_projection().is_finite());
    }

    #[test]
    fn the_field_of_view_is_set_in_degrees_and_bad_values_are_ignored() {
        let mut camera = Camera::default();
        assert!((camera.fov_degrees() - 70.0).abs() < 1e-3);
        camera.set_fov_degrees(95.0);
        assert!((camera.fov_degrees() - 95.0).abs() < 1e-3);
        for bad in [0.0, -10.0, 180.0, f32::NAN, f32::INFINITY] {
            camera.set_fov_degrees(bad);
            assert!((camera.fov_degrees() - 95.0).abs() < 1e-3, "{bad}");
        }
        assert!(camera.view_projection().is_finite());
    }
}
