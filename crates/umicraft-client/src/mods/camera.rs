// SPDX-License-Identifier: MIT OR Apache-2.0

//! The render camera as the `render_camera` mods see it (API 1.1, ADR-0094).
//!
//! The renderer's camera turns the opposite way from the game: its yaw is the
//! Minecraft yaw plus half a turn, its pitch is the Minecraft pitch negated
//! (`crate::game::camera_angles`). Mods get Minecraft degrees, like everywhere in
//! the API, and the conversion back only writes what a layer changed, so a
//! wrapper that changes nothing leaves the frame drawn bit for bit as without it.

use std::f32::consts::PI;
use std::mem::size_of;

use umicraft_modapi_sys::{UmiCameraInput, UmiRenderCamera, UMI_INPUT_JUMP, UMI_INPUT_SNEAK};
use umicraft_modhost::CameraChain;
use umicraft_render::Camera;

use crate::input::InputState;

/// The ABI view of `camera`.
#[must_use]
pub fn camera_of(camera: &Camera) -> UmiRenderCamera {
    UmiRenderCamera {
        size: size_of::<UmiRenderCamera>() as u32,
        flags: 0,
        eye: camera.eye().to_array(),
        yaw: (camera.yaw - PI).to_degrees(),
        pitch: -camera.pitch.to_degrees(),
        fov_y: camera.effective_fov_degrees(),
        aspect: camera.aspect(),
        view_bob: camera.view_bob().to_cols_array_2d(),
    }
}

/// What the player asks of the view this frame: the movement keys of `keys`, the
/// `look` the mouse turned (Minecraft degrees, yaw then pitch) and the duration
/// of the previous frame.
#[must_use]
pub fn camera_input(keys: &InputState, look: (f32, f32), frame_seconds: f32) -> UmiCameraInput {
    let flag = |on: bool, bit: u32| if on { bit } else { 0 };
    UmiCameraInput {
        size: size_of::<UmiCameraInput>() as u32,
        flags: flag(keys.jump, UMI_INPUT_JUMP) | flag(keys.sneak, UMI_INPUT_SNEAK),
        look_dx: look.0,
        look_dy: look.1,
        forward: keys.forward,
        strafe: keys.strafe,
        up: f32::from(i8::from(keys.jump) - i8::from(keys.sneak)),
        frame_seconds,
    }
}

/// Lets the mods' layers move `camera` for this frame. Only the render camera
/// changes: the player, its view and what the server is told stay as they are.
///
/// Returns whether a layer captured the input (API 1.3, ADR-0096): the next frame's
/// movement keys and mouse look then go to the camera only ([`without_movement`]).
pub fn place_camera(chain: &CameraChain, camera: &mut Camera, input: &UmiCameraInput) -> bool {
    if !chain.is_active() {
        return false;
    }
    let before = camera_of(camera);
    let mut placed = before;
    let captured = chain.place(&mut placed, input);
    apply(camera, &before, &placed);
    captured
}

/// How far a camera layer must move the eye, in blocks, for the camera to count as
/// away from the player (the third person's own threshold).
const DETACHED_DISTANCE: f64 = 0.05;

/// Whether the layers moved the eye from `before` (where the client put it) to
/// `after` far enough that the player's body should be drawn and the hand not.
#[must_use]
pub fn camera_moved(before: glam::DVec3, after: glam::DVec3) -> bool {
    before.distance(after) > DETACHED_DISTANCE
}

/// What the player is given while a camera layer captures the input: `keys` with
/// no movement key held, as if the player let go of them. Attack and use stay, so
/// the player can still act where it looks.
#[must_use]
pub fn without_movement(keys: InputState) -> InputState {
    InputState {
        attack: keys.attack,
        use_item: keys.use_item,
        ..InputState::default()
    }
}

/// Writes into `camera` the fields of `placed` that differ from `before`.
fn apply(camera: &mut Camera, before: &UmiRenderCamera, placed: &UmiRenderCamera) {
    let changed = |a: f32, b: f32| a.to_bits() != b.to_bits();
    if placed
        .eye
        .iter()
        .zip(&before.eye)
        .any(|(a, b)| a.to_bits() != b.to_bits())
    {
        camera.set_eye(glam::DVec3::from_array(placed.eye));
    }
    if changed(placed.yaw, before.yaw) {
        camera.yaw = placed.yaw.to_radians() + PI;
    }
    if changed(placed.pitch, before.pitch) {
        camera.pitch = -placed.pitch.to_radians();
    }
    let bob_changed = placed
        .view_bob
        .iter()
        .flatten()
        .zip(before.view_bob.iter().flatten())
        .any(|(a, b)| changed(*a, *b));
    if changed(placed.fov_y, before.fov_y) {
        // The layer gave the final field of view: no sprint or flight factor on top.
        camera.set_fov_degrees(placed.fov_y);
        camera.set_view_effects(1.0, glam::Mat4::from_cols_array_2d(&placed.view_bob));
    } else if bob_changed {
        camera.set_view_effects(
            camera.fov_modifier(),
            glam::Mat4::from_cols_array_2d(&placed.view_bob),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Camera {
        let mut camera = Camera::from_minecraft(100.5, 70.0, -20.25, 30.0, 15.0);
        camera.set_aspect(1920, 1080);
        camera.set_fov_degrees(70.0);
        camera.set_view_effects(1.1, glam::Mat4::IDENTITY);
        camera
    }

    #[test]
    fn mods_see_minecraft_degrees_and_the_effective_fov() {
        let view = camera_of(&camera());
        assert!((view.yaw - 30.0).abs() < 1e-3, "{}", view.yaw);
        assert!((view.pitch - 15.0).abs() < 1e-3, "{}", view.pitch);
        assert!((view.fov_y - 77.0).abs() < 1e-3);
        assert!((view.aspect - 1920.0 / 1080.0).abs() < 1e-6);
    }

    #[test]
    fn an_unchanged_camera_is_left_bit_for_bit() {
        let mut moved = camera();
        let before = camera_of(&moved);
        apply(&mut moved, &before, &before);
        let original = camera();
        assert_eq!(moved.eye(), original.eye());
        assert_eq!(
            (moved.yaw.to_bits(), moved.pitch.to_bits()),
            (original.yaw.to_bits(), original.pitch.to_bits())
        );
        assert_eq!(
            moved.view_projection_from(moved.render_origin()),
            original.view_projection_from(original.render_origin())
        );
    }

    #[test]
    fn a_layer_change_lands_in_the_render_camera() {
        let mut moved = camera();
        let before = camera_of(&moved);
        let placed = UmiRenderCamera {
            eye: [0.0, 100.0, 0.0],
            yaw: 90.0,
            pitch: -45.0,
            fov_y: 50.0,
            ..before
        };
        apply(&mut moved, &before, &placed);
        let after = camera_of(&moved);
        assert_eq!(after.eye, [0.0, 100.0, 0.0]);
        assert!((after.yaw - 90.0).abs() < 1e-3 && (after.pitch + 45.0).abs() < 1e-3);
        assert!((after.fov_y - 50.0).abs() < 1e-3, "no sprint factor on top");
    }

    #[test]
    fn only_a_real_move_of_the_eye_detaches_the_camera() {
        let eye = glam::DVec3::new(10.0, 64.0, -3.0);
        assert!(!camera_moved(eye, eye));
        assert!(!camera_moved(eye, eye + glam::DVec3::Y * 0.01));
        assert!(camera_moved(eye, eye + glam::DVec3::X * 2.0));
    }

    #[test]
    fn a_captured_frame_gives_the_player_no_movement_but_keeps_its_actions() {
        let keys = InputState {
            forward: 1.0,
            strafe: 1.0,
            jump: true,
            sneak: true,
            sprint: true,
            attack: true,
            use_item: true,
        };
        let player = without_movement(keys);
        assert_eq!(
            player,
            InputState {
                attack: true,
                use_item: true,
                ..InputState::default()
            }
        );
    }

    #[test]
    fn the_input_is_the_movement_intent_without_raw_keys() {
        let keys = InputState {
            forward: 1.0,
            strafe: -1.0,
            jump: true,
            ..InputState::default()
        };
        let input = camera_input(&keys, (3.0, -1.5), 0.02);
        assert_eq!((input.forward, input.strafe, input.up), (1.0, -1.0, 1.0));
        assert_eq!((input.look_dx, input.look_dy), (3.0, -1.5));
        assert_eq!(input.flags, UMI_INPUT_JUMP);
    }
}
