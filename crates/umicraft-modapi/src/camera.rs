// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wrapping or replacing where the camera of each frame is placed
//! (`render_camera`, API 1.1).
//!
//! A camera layer only moves what is drawn: the player stays where the physics
//! put it, and the server never hears of the camera.

use umicraft_modapi_sys::{
    UmiCameraCx, UmiCameraInput, UmiRenderCamera, UMI_INPUT_JUMP, UMI_INPUT_SNEAK,
};

use crate::ffi::StatePtr;
use crate::{host, ModError};

/// The camera of one frame, as a camera layer sees it.
///
/// Angles are Minecraft degrees: yaw clockwise from south, pitch positive
/// down. The host clamps the pitch to ±90 and the field of view to 1..=170
/// after the layers ran, and ignores non-finite values.
#[repr(transparent)]
#[derive(Debug)]
pub struct CameraState(UmiRenderCamera);

impl CameraState {
    /// The eye, in blocks.
    pub fn eye(&self) -> [f64; 3] {
        self.0.eye
    }

    /// Moves the eye to `eye`, in blocks.
    pub fn set_eye(&mut self, eye: [f64; 3]) {
        self.0.eye = eye;
    }

    /// The view yaw in degrees.
    pub fn yaw(&self) -> f32 {
        self.0.yaw
    }

    /// Turns the view to `yaw` degrees.
    pub fn set_yaw(&mut self, yaw: f32) {
        self.0.yaw = yaw;
    }

    /// The view pitch in degrees, positive looks down.
    pub fn pitch(&self) -> f32 {
        self.0.pitch
    }

    /// Tilts the view to `pitch` degrees.
    pub fn set_pitch(&mut self, pitch: f32) {
        self.0.pitch = pitch;
    }

    /// The effective vertical field of view, in degrees.
    pub fn fov_y(&self) -> f32 {
        self.0.fov_y
    }

    /// Sets the vertical field of view, in degrees.
    pub fn set_fov_y(&mut self, fov_y: f32) {
        self.0.fov_y = fov_y;
    }

    /// The viewport width over its height (read-only).
    pub fn aspect(&self) -> f32 {
        self.0.aspect
    }

    /// The column-major camera-space view bobbing, identity when still.
    pub fn view_bob(&self) -> [[f32; 4]; 4] {
        self.0.view_bob
    }

    /// Replaces the view bobbing; the identity matrix turns it off.
    pub fn set_view_bob(&mut self, view_bob: [[f32; 4]; 4]) {
        self.0.view_bob = view_bob;
    }
}

/// What the player asks of the view this frame, from the same input the
/// built-in view uses. There is no raw key state.
#[repr(transparent)]
#[derive(Debug)]
pub struct CameraInput(UmiCameraInput);

impl CameraInput {
    /// Degrees the mouse turned the view this frame, positive to the right.
    pub fn look_dx(&self) -> f32 {
        self.0.look_dx
    }

    /// Degrees the mouse turned the view this frame, positive down.
    pub fn look_dy(&self) -> f32 {
        self.0.look_dy
    }

    /// Forward movement, `-1.0` (backwards) ..= `1.0`.
    pub fn forward(&self) -> f32 {
        self.0.forward
    }

    /// Sideways movement, `-1.0` (right) ..= `1.0` (left).
    pub fn strafe(&self) -> f32 {
        self.0.strafe
    }

    /// Vertical movement, `-1.0` (sneak) ..= `1.0` (jump).
    pub fn up(&self) -> f32 {
        self.0.up
    }

    /// Whether the jump key is held.
    pub fn jump(&self) -> bool {
        self.0.flags & UMI_INPUT_JUMP != 0
    }

    /// Whether the sneak key is held.
    pub fn sneak(&self) -> bool {
        self.0.flags & UMI_INPUT_SNEAK != 0
    }

    /// Duration of the previous frame, in seconds.
    pub fn frame_seconds(&self) -> f32 {
        self.0.frame_seconds
    }
}

/// One call of a camera layer: the camera to change and the way to run what
/// is inside the layer.
///
/// It only lives for that call. A wrapper that changes nothing leaves the
/// camera as the client placed it.
///
/// ```no_run
/// # use umicraft_modapi::{InitCx, ModError};
/// # fn init(cx: &mut InitCx<'_>) -> Result<(), ModError> {
/// // Lift the camera two blocks above wherever the client puts it.
/// cx.wrap_camera(|camera| {
///     camera.next()?;
///     let [x, y, z] = camera.state().eye();
///     camera.state_mut().set_eye([x, y + 2.0, z]);
///     Ok(())
/// })?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Camera<'a> {
    cx: *mut UmiCameraCx,
    state: StatePtr<CameraState>,
    input: &'a CameraInput,
}

impl<'a> Camera<'a> {
    pub(crate) fn new(
        cx: *mut UmiCameraCx,
        state: StatePtr<CameraState>,
        input: &'a CameraInput,
    ) -> Self {
        Self { cx, state, input }
    }

    /// The camera as it is now: what the layer received, or what the inner
    /// layers made of it after [`next`](Self::next).
    pub fn state(&self) -> &CameraState {
        self.state.get()
    }

    /// The camera, to change before or after [`next`](Self::next).
    pub fn state_mut(&mut self) -> &mut CameraState {
        self.state.get_mut()
    }

    /// What the player asks of the view this frame.
    pub fn input(&self) -> &CameraInput {
        self.input
    }

    /// Runs the inner layers on the camera, once.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] with `DENIED` for a second call and
    /// `UNSUPPORTED` in a replacing layer.
    // The name mirrors the ABI's `camera_next`; this type is not an iterator.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<(), ModError> {
        host::call(|host, _| (host.camera_next)(self.cx)).and_then(ModError::check)
    }

    /// Takes the player's movement keys and mouse look for the camera, from the
    /// next frame on (API 1.3): [`input`](Self::input) still reports them, but the
    /// player stands as if no movement key were held and does not turn. Attack,
    /// use and every other key still act. For a free camera.
    ///
    /// The capture lasts one frame: call it on every frame the camera wants the
    /// input. When the layer stops asking, or its mod is switched off or fails,
    /// the player gets the input back.
    ///
    /// # Errors
    /// [`ModError::HostRefused`] with `DENIED` outside this layer's own call.
    pub fn capture_input(&mut self) -> Result<(), ModError> {
        host::call(|host, _| (host.camera_capture)(self.cx)).and_then(ModError::check)
    }
}

/// The boxed closure type behind a camera layer's `user` pointer.
pub(crate) type CameraHandler = Box<dyn FnMut(&mut Camera<'_>) -> Result<(), ModError> + Send>;

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    fn camera() -> CameraState {
        CameraState(UmiRenderCamera {
            size: core::mem::size_of::<UmiRenderCamera>() as u32,
            flags: 0,
            eye: [1.0, 2.0, 3.0],
            yaw: 45.0,
            pitch: -10.0,
            fov_y: 70.0,
            aspect: 2.0,
            view_bob: IDENTITY,
        })
    }

    #[test]
    fn the_writable_fields_have_setters_that_land_in_the_abi_struct() {
        let mut camera = camera();
        camera.set_eye([4.0, 5.0, 6.0]);
        camera.set_yaw(90.0);
        camera.set_pitch(30.0);
        camera.set_fov_y(50.0);
        let mut bob = IDENTITY;
        bob[3][1] = 0.1;
        camera.set_view_bob(bob);
        assert_eq!(camera.eye(), [4.0, 5.0, 6.0]);
        assert_eq!(
            (camera.yaw(), camera.pitch(), camera.fov_y()),
            (90.0, 30.0, 50.0)
        );
        assert_eq!(camera.view_bob(), bob);
        assert_eq!(camera.aspect(), 2.0);
        assert_eq!(camera.0.eye, [4.0, 5.0, 6.0]);
    }

    #[test]
    fn the_input_reads_the_intent_and_the_flags() {
        let input = CameraInput(UmiCameraInput {
            size: core::mem::size_of::<UmiCameraInput>() as u32,
            flags: UMI_INPUT_SNEAK,
            look_dx: 1.5,
            look_dy: -0.5,
            forward: 1.0,
            strafe: -1.0,
            up: -1.0,
            frame_seconds: 0.016,
        });
        assert_eq!((input.look_dx(), input.look_dy()), (1.5, -0.5));
        assert_eq!(
            (input.forward(), input.strafe(), input.up()),
            (1.0, -1.0, -1.0)
        );
        assert!(input.sneak() && !input.jump());
        assert_eq!(input.frame_seconds(), 0.016);
    }
}
