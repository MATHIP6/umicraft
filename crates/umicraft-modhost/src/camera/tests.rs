// SPDX-License-Identifier: MIT OR Apache-2.0

use core::ffi::c_void;
use std::sync::Mutex;

use umicraft_modapi_sys::{UmiHost, UmiPhysicsCx, UmiServiceMode};

use super::*;
use crate::ffi::camera_testing::{raise_eye, set_angles};
use crate::ffi::{host_table, UserPtr};
use crate::manifest::{Load, Permissions};
use crate::ui::shared::HostShared;

const IDENTITY: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn camera() -> UmiRenderCamera {
    UmiRenderCamera {
        size: core::mem::size_of::<UmiRenderCamera>() as u32,
        flags: 0,
        eye: [0.5, 65.62, 0.5],
        yaw: 90.0,
        pitch: 10.0,
        fov_y: 70.0,
        aspect: 16.0 / 9.0,
        view_bob: IDENTITY,
    }
}

fn input() -> UmiCameraInput {
    UmiCameraInput {
        size: core::mem::size_of::<UmiCameraInput>() as u32,
        flags: 0,
        look_dx: 0.0,
        look_dy: 0.0,
        forward: 0.0,
        strafe: 0.0,
        up: 0.0,
        frame_seconds: 0.016,
    }
}

fn table() -> UmiHost {
    host_table()
}

fn load(wraps: bool) -> Load {
    Load {
        wraps: if wraps {
            vec![RENDER_CAMERA.to_owned().try_into().unwrap()]
        } else {
            Vec::new()
        },
        ..Load::default()
    }
}

fn mod_with(camera: bool, movement: bool, load: &Load) -> Arc<ModContext> {
    let permissions = Permissions {
        camera,
        movement,
        ..Permissions::default()
    };
    let id = "cam".to_owned().try_into().unwrap();
    ModContext::with_services(id, permissions, load, Arc::new(HostShared::default()))
}

fn wrapper(step: UmiCameraFn) -> Arc<ModContext> {
    let cx = mod_with(true, false, &load(true));
    let user = UserPtr::new(core::ptr::null_mut());
    assert_eq!(
        cx.register_camera(UmiServiceMode::WRAP, Some(step), user),
        UmiStatus::OK
    );
    cx.finish_init(true);
    cx
}

fn place(context: &Arc<ModContext>) -> UmiRenderCamera {
    let chain = CameraChain::new([context]).unwrap();
    let mut placed = camera();
    chain.place(&mut placed, &input());
    placed
}

/// Calls `next`, then raises the eye by four blocks (an orbit, say).
extern "C" fn raiser(
    cx: *mut UmiCameraCx,
    camera: *mut UmiRenderCamera,
    _: *const UmiCameraInput,
    _: *mut c_void,
) -> UmiStatus {
    let status = (table().camera_next)(cx);
    raise_eye(camera, 4.0);
    status
}

/// Writes nonsense into the angles, the field of view and the read-only aspect.
extern "C" fn vandal(
    _: *mut UmiCameraCx,
    camera: *mut UmiRenderCamera,
    _: *const UmiCameraInput,
    _: *mut c_void,
) -> UmiStatus {
    set_angles(camera, 1_000.0);
    UmiStatus::OK
}

static CROSSED: Mutex<Option<UmiStatus>> = Mutex::new(None);

/// Hands its camera `cx` to `physics_next`, which must refuse it.
extern "C" fn crosser(
    cx: *mut UmiCameraCx,
    _: *mut UmiRenderCamera,
    _: *const UmiCameraInput,
    _: *mut c_void,
) -> UmiStatus {
    *CROSSED.lock().unwrap() = Some((table().physics_next)(cx.cast::<UmiPhysicsCx>()));
    UmiStatus::OK
}

#[test]
fn a_wrapper_moves_the_camera_and_the_builtin_step_changes_nothing() {
    let cx = wrapper(raiser);
    let placed = place(&cx);
    assert_eq!(placed.eye, [0.5, 69.62, 0.5]);
    assert_eq!((placed.yaw, placed.pitch, placed.fov_y), (90.0, 10.0, 70.0));
    assert_eq!(cx.camera_stats().calls, 1);
}

#[test]
fn a_switched_off_layer_leaves_the_builtin_camera() {
    let cx = wrapper(raiser);
    let chain = CameraChain::new([&cx]).unwrap();
    assert!(chain.is_active());
    cx.set_wanted(false);
    assert!(!chain.is_active());
    let mut placed = camera();
    chain.place(&mut placed, &input());
    assert_eq!(placed, camera());
}

#[test]
fn what_a_layer_leaves_is_made_safe_to_render() {
    let placed = place(&wrapper(vandal));
    assert_eq!((placed.pitch, placed.fov_y), (90.0, 170.0));
    assert_eq!(placed.yaw, 1_000.0, "any finite yaw is a direction");
    assert_eq!(placed.aspect, camera().aspect, "read-only");
}

#[test]
fn a_camera_cx_is_refused_by_physics_next() {
    place(&wrapper(crosser));
    assert_eq!(*CROSSED.lock().unwrap(), Some(UmiStatus::DENIED));
}

#[test]
fn registration_needs_the_permission_and_the_manifest_entry() {
    let user = || UserPtr::new(core::ptr::null_mut());
    let wrap = UmiServiceMode::WRAP;
    // `movement` is not `camera`.
    let cx = mod_with(false, true, &load(true));
    assert_eq!(
        cx.register_camera(wrap, Some(raiser), user()),
        UmiStatus::DENIED
    );
    let cx = mod_with(true, false, &load(false));
    assert_eq!(
        cx.register_camera(wrap, Some(raiser), user()),
        UmiStatus::DENIED
    );
    let cx = mod_with(true, false, &load(true));
    assert_eq!(
        cx.register_camera(wrap, None, user()),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(
        cx.register_camera(wrap, Some(raiser), user()),
        UmiStatus::OK
    );
    assert_eq!(
        cx.register_camera(wrap, Some(raiser), user()),
        UmiStatus::CONFLICT
    );
    assert!(CameraChain::new([&mod_with(true, false, &load(true))]).is_none());
}

#[test]
fn a_camera_mod_is_not_held_by_server_compat() {
    let cx = wrapper(raiser);
    cx.shared.set_server_compat(true);
    assert!(cx.is_enabled(), "the owner's decision of ADR-0094");
}
