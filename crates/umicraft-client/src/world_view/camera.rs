// SPDX-License-Identifier: MIT OR Apache-2.0

//! The camera, the hand and the environment of a frame, interpolated for the
//! time of the frame.

use std::time::Instant;

use umicraft_game::hand::HandPose;
use umicraft_render::{Camera, RenderEnvironment};

use super::WorldView;
use crate::game::camera_angles;

/// How far, in blocks, the camera has to be from the eye of the player to count
/// as the third person: the first person has it on the eye, the third person an
/// orbit of up to four blocks, which a wall shortens.
const THIRD_PERSON_MIN_ORBIT: f64 = 0.05;

impl WorldView {
    /// Points `camera` at the player, interpolated for `now` (GAM-8).
    /// `view_bobbing` is the "View Bobbing" option: off, the camera and the
    /// hand keep still while walking (`EntityRenderer.setupCameraTransform`).
    /// `dynamic_fov` is the "Dynamic FOV" option: off, the field of view stays
    /// what it is set to, whatever the player does. `hurt_cam` is whether a hit
    /// tilts the view; the No Hurt Cam mod turns it off.
    pub fn update_camera(
        &self,
        camera: &mut Camera,
        now: Instant,
        (view_bobbing, dynamic_fov, hurt_cam): (bool, bool, bool),
    ) {
        let Some(state) = self.view.sample(now) else {
            return;
        };
        camera.set_eye(state.position);
        let (yaw, pitch) = camera_angles(&state);
        camera.yaw = yaw;
        camera.pitch = pitch;
        // The hurt roll stays with the option off, the bobbing does not.
        let mut effects = state.effects;
        if !hurt_cam {
            effects.hurt_roll_degrees = 0.0;
        }
        let bob = effects.camera_matrix(view_bobbing);
        let fov_modifier = if dynamic_fov {
            state.effects.fov_modifier
        } else {
            1.0
        };
        camera.set_view_effects(fov_modifier, bob);
    }

    /// Immediately applies relative yaw and pitch deltas (in radians) to the active camera.
    pub fn apply_look_delta(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.view.apply_look_delta(yaw_delta, pitch_delta);
    }

    /// The player's own view for `now`, whatever the render camera does: the eye
    /// and the angles in the renderer's convention (radians, see
    /// [`camera_angles`]). The third person and the mods' camera layers move the
    /// camera, never this; `None` with no frame yet.
    #[must_use]
    pub fn player_view(&self, now: Instant) -> Option<(glam::DVec3, f32, f32)> {
        let state = self.view.sample(now)?;
        let (yaw, pitch) = camera_angles(&state);
        Some((state.eye, yaw, pitch))
    }

    /// The eye of the local player while the camera shows the player from
    /// outside (the third person), `None` in the first person and with no frame.
    /// The camera is the orbit, so it differs from the eye by the orbit distance.
    #[must_use]
    pub fn third_person_body(&self, now: Instant) -> Option<glam::DVec3> {
        let state = self.view.sample(now)?;
        (state.position.distance(state.eye) > THIRD_PERSON_MIN_ORBIT).then_some(state.eye)
    }

    /// The pose of the first-person hand for `now`.
    #[must_use]
    pub fn hand_pose(&self, now: Instant) -> HandPose {
        // The sway reads the live view (`ItemRenderer.func_178110_a` uses the
        // `rotationYaw` of the frame), not the one of the last tick.
        self.view
            .sample(now)
            .map_or_else(HandPose::default, |state| {
                state
                    .effects
                    .hand
                    .swayed_by(state.yaw.to_degrees(), state.pitch.to_degrees())
            })
    }

    /// The sky, fog and light environment for `now`, in step with the camera.
    #[must_use]
    pub fn environment(&self, now: Instant, view_distance: u8) -> RenderEnvironment {
        self.view.environment(now, view_distance)
    }
}
