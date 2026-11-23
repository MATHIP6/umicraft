// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `render_camera` service (API 1.1, ADR-0094): mods wrap or replace the
//! placement of the camera a frame is drawn with, without moving the player.
//!
//! A mod declares `render_camera` in `mod.toml` (`wraps` or `replaces`) and the
//! `camera` permission. Layers nest like the `player_physics` ones.

use core::ffi::c_void;

use crate::UmiStatus;

/// The camera of one frame, lent to a `render_camera` layer.
///
/// Angles are Minecraft degrees (yaw clockwise from south, pitch positive
/// down), as everywhere in the API.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UmiRenderCamera {
    /// Size in bytes of the struct, so later minors can append fields.
    pub size: u32,
    /// Reserved, 0.
    pub flags: u32,
    /// Eye position, in blocks. Writable.
    pub eye: [f64; 3],
    /// View yaw in degrees. Writable.
    pub yaw: f32,
    /// View pitch in degrees, positive looks down. Writable; the host clamps
    /// it to ±90.
    pub pitch: f32,
    /// Effective vertical field of view in degrees. Writable; the host clamps
    /// it to 1..=170.
    pub fov_y: f32,
    /// Viewport width over height. Read-only.
    pub aspect: f32,
    /// Column-major camera-space view bobbing, identity when still. Writable.
    pub view_bob: [[f32; 4]; 4],
}

/// What the player asks of the view this frame, from the same aggregated input
/// the built-in view consumes; read-only. There is no raw key state.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UmiCameraInput {
    /// Size in bytes of the struct.
    pub size: u32,
    /// Bit set of `UMI_INPUT_JUMP` / `UMI_INPUT_SNEAK`.
    pub flags: u32,
    /// Degrees the mouse turned the view this frame, positive to the right.
    pub look_dx: f32,
    /// Degrees the mouse turned the view this frame, positive down.
    pub look_dy: f32,
    /// Forward movement, `-1.0` (backwards) ..= `1.0`.
    pub forward: f32,
    /// Sideways movement, `-1.0` (right) ..= `1.0` (left).
    pub strafe: f32,
    /// Vertical movement, `-1.0` (sneak) ..= `1.0` (jump).
    pub up: f32,
    /// Duration of the previous frame, in seconds.
    pub frame_seconds: f32,
}

/// The host's state for one call of a camera layer; opaque to mods.
#[repr(C)]
pub struct UmiCameraCx {
    _private: [u8; 0],
}

/// One `render_camera` layer: changes `camera` for this frame, calling
/// `UmiHost::camera_next(cx)` at most once to run the inner layers (a
/// wrapper). Called on the render thread within a time budget.
pub type UmiCameraFn = extern "C" fn(
    cx: *mut UmiCameraCx,
    camera: *mut UmiRenderCamera,
    input: *const UmiCameraInput,
    user: *mut c_void,
) -> UmiStatus;
