// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `render_camera` service: mods wrap or replace where the camera of a frame is
//! placed, without moving the player (API 1.1, ADR-0094).
//!
//! Layers run on the render thread once per drawn frame, after the built-in camera
//! is placed and before it is handed to the GPU. The built-in step is the identity:
//! the state a chain starts from already is the built-in camera. The chain itself
//! (nesting, budgets, `next`) is the shared [`ServiceChain`].

use core::ffi::c_void;
use std::cell::Cell;
use std::sync::Arc;
use std::thread::LocalKey;

use umicraft_modapi_sys::{UmiCameraCx, UmiCameraFn, UmiCameraInput, UmiRenderCamera, UmiStatus};

use crate::context::ModContext;
use crate::ffi::CAMERA_ACTIVE;
use crate::manifest::RENDER_CAMERA;
use crate::service::{Service, ServiceChain, ServiceSlot};

/// The `render_camera` service.
#[derive(Debug)]
pub(crate) struct RenderCamera;

impl Service for RenderCamera {
    type State = UmiRenderCamera;
    type Input = UmiCameraInput;
    type Step = UmiCameraFn;
    const ID: &'static str = RENDER_CAMERA;
    const LAYER: &'static str = "its `render_camera` layer";

    fn call(
        step: UmiCameraFn,
        cx: *mut c_void,
        state: *mut UmiRenderCamera,
        input: *const UmiCameraInput,
        user: *mut c_void,
    ) -> UmiStatus {
        step(cx.cast::<UmiCameraCx>(), state, input, user)
    }

    fn slot(context: &ModContext) -> &ServiceSlot<Self> {
        context.camera_slot()
    }

    fn active() -> &'static LocalKey<Cell<*mut c_void>> {
        &CAMERA_ACTIVE
    }
}

/// Largest field of view a layer may ask for, in degrees (the option's own range).
const FOV_RANGE: (f32, f32) = (1.0, 170.0);
/// Steepest pitch, in degrees.
const MAX_PITCH: f32 = 90.0;

/// The `render_camera` layers of every loaded mod, ready to run on a frame.
///
/// Cheap to clone. Layers are looked up per frame, so a mod that is switched off or
/// fails takes effect at once.
#[derive(Debug, Clone)]
pub struct CameraChain(ServiceChain<RenderCamera>);

impl CameraChain {
    /// The chain of the layers `contexts` registered, in load order, with replacing
    /// layers after the wrappers that surround them. `None` when no context
    /// registered one.
    pub fn new<'a>(contexts: impl IntoIterator<Item = &'a Arc<ModContext>>) -> Option<Self> {
        ServiceChain::new(contexts).map(Self)
    }

    /// Whether some layer is enabled now; when not, [`place`](Self::place) changes
    /// nothing.
    pub fn is_active(&self) -> bool {
        self.0.is_active()
    }

    /// Lets the layers move `camera`, which holds the built-in camera of the frame.
    ///
    /// A failing layer fails its mod and its own changes are undone, as for
    /// `player_physics`. What the layers leave is then made safe to render: a
    /// non-finite number takes the built-in value back, the pitch is clamped to ±90
    /// and the field of view to 1..=170 degrees, and the read-only fields (`size`,
    /// `flags`, `aspect`) are restored.
    pub fn place(&self, camera: &mut UmiRenderCamera, input: &UmiCameraInput) {
        let builtin = *camera;
        self.0.run(camera, input, &mut |_| {});
        *camera = sanitized(*camera, &builtin);
    }
}

/// `camera` with every field a layer may not set, or set to something unusable,
/// taken from `builtin`.
fn sanitized(camera: UmiRenderCamera, builtin: &UmiRenderCamera) -> UmiRenderCamera {
    let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };
    let eye = if camera.eye.iter().all(|v| v.is_finite()) {
        camera.eye
    } else {
        builtin.eye
    };
    let bob_finite = camera.view_bob.iter().flatten().all(|v| v.is_finite());
    UmiRenderCamera {
        size: builtin.size,
        flags: builtin.flags,
        eye,
        yaw: finite(camera.yaw, builtin.yaw),
        pitch: finite(camera.pitch, builtin.pitch).clamp(-MAX_PITCH, MAX_PITCH),
        fov_y: finite(camera.fov_y, builtin.fov_y).clamp(FOV_RANGE.0, FOV_RANGE.1),
        aspect: builtin.aspect,
        view_bob: if bob_finite {
            camera.view_bob
        } else {
            builtin.view_bob
        },
    }
}

#[cfg(test)]
mod tests;
