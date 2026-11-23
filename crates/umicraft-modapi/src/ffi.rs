// SPDX-License-Identifier: MIT OR Apache-2.0

//! All `unsafe` of the crate: reading host-provided pointers and owning the
//! boxed closures behind the `user` pointers (ADR-0087).
#![allow(unsafe_code)]

use core::ffi::c_void;
use std::mem::size_of;

use core::ptr::NonNull;
use umicraft_modapi_sys::{
    UmiBytes, UmiCameraCx, UmiCameraInput, UmiEntity, UmiEventHeader, UmiHost, UmiHudCx,
    UmiModHandle, UmiPhysicsCx, UmiPhysicsInput, UmiPlayerPhysics, UmiRenderCamera, UmiStatus,
    UmiStr, UMI_EVENT_CANCELLABLE, UMI_EVENT_CANCELLED,
};

use crate::camera::CameraHandler;
use crate::event::{Control, Event};
use crate::private::guarded;
use crate::{host, Camera, CameraInput, Entity, Hud, Input, ModError, Physics};

/// A mod handle that can be shared across threads.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SharedHandle(UmiModHandle);

// SAFETY: the host keeps the context behind the handle alive and at a fixed
// address for the whole process and its functions are thread-safe (ADR-0086:
// unload means disable). The mod never dereferences the pointer itself.
unsafe impl Send for SharedHandle {}
// SAFETY: same guarantee as `Send` above.
unsafe impl Sync for SharedHandle {}

impl SharedHandle {
    pub(crate) fn new(handle: UmiModHandle) -> Self {
        Self(handle)
    }

    pub(crate) fn get(self) -> UmiModHandle {
        self.0
    }
}

/// The boxed closure type behind an event `user` pointer.
pub(crate) type Handler<E> = Box<dyn FnMut(&mut E, &mut Control) + Send>;

/// The boxed closure type behind a HUD widget's `user` pointer.
pub(crate) type DrawHandler = Box<dyn FnMut(&mut Hud<'_>) + Send>;

/// The boxed closure type behind a command's `user` pointer.
pub(crate) type CommandHandler = Box<dyn FnMut(&str) -> Result<(), ModError> + Send>;

/// A boxed value handed to the host as a `user` pointer; freed on drop.
#[derive(Debug)]
pub(crate) struct OwnedUser {
    user: *mut c_void,
    free: unsafe fn(*mut c_void),
}

// SAFETY: the pointer owns a `Box<T>` where `T: Send`.
unsafe impl Send for OwnedUser {}

impl OwnedUser {
    pub(crate) fn new<T: Send + 'static>(value: T) -> Self {
        Self {
            user: Box::into_raw(Box::new(value)).cast(),
            free: free_user::<T>,
        }
    }

    /// The pointer to pass to the host; valid until `self` drops.
    pub(crate) fn user(&self) -> *mut c_void {
        self.user
    }
}

impl Drop for OwnedUser {
    fn drop(&mut self) {
        // SAFETY: `user` came from `Box::into_raw` in `new` with the type
        // `free` was instantiated for, and `drop` runs once.
        unsafe { (self.free)(self.user) }
    }
}

/// # Safety
/// `user` must come from [`OwnedUser::new`] for the same `T`, unfreed.
unsafe fn free_user<T>(user: *mut c_void) {
    // SAFETY: guaranteed by the caller.
    drop(unsafe { Box::from_raw(user.cast::<T>()) });
}

/// Runs `f` on the `T` behind a `user` pointer the host passed back; `None`
/// for a null pointer.
///
/// `T` must be the type the pointer was registered with. Every trampoline
/// names its own `T`, and the host returns each `user` only to the callback
/// it was registered with.
fn with_user<T, R>(user: *mut c_void, f: impl FnOnce(&mut T) -> R) -> Option<R> {
    if user.is_null() {
        return None;
    }
    // SAFETY: non-null and, per the contract above, a live `T` made by
    // `OwnedUser::new::<T>` that stays alive until shutdown; the host does not
    // call a callback concurrently with itself (shutdown-quiescence rule).
    Some(f(unsafe { &mut *user.cast::<T>() }))
}

/// Reads a host string as `&str`; the borrow of `text` bounds the lifetime,
/// matching the ABI rule that payload strings live for the handler call.
///
/// Null, empty and invalid UTF-8 all read as `""`: the host is trusted to send
/// UTF-8, but a non-conforming one must not make a mod panic or read garbage.
pub(crate) fn str_of(text: &UmiStr) -> &str {
    if text.ptr.is_null() || text.len == 0 {
        return "";
    }
    // SAFETY: non-null with `len` bytes the host promises are readable for the
    // payload's lifetime, which the borrow of `text` stays within (ADR-0087).
    let bytes = unsafe { core::slice::from_raw_parts(text.ptr, text.len) };
    core::str::from_utf8(bytes).unwrap_or("")
}

/// Reads host bytes as `&[u8]`; the borrow of `bytes` bounds the lifetime,
/// matching the ABI rule that payload bytes live for the handler call.
///
/// Null and empty both read as an empty slice, so a non-conforming host that
/// sends a null pointer with a length cannot make a mod read address zero.
pub(crate) fn bytes_of(bytes: &UmiBytes) -> &[u8] {
    if bytes.ptr.is_null() || bytes.len == 0 {
        return &[];
    }
    // SAFETY: non-null with `len` bytes the host promises are readable for the
    // payload's lifetime, which the borrow of `bytes` stays within (ADR-0087).
    unsafe { core::slice::from_raw_parts(bytes.ptr, bytes.len) }
}

/// Validates the host table: non-null and large enough for every field this
/// crate reads, so an older host is refused instead of read out of bounds.
pub(crate) fn host_ref<'a>(host: *const UmiHost) -> Result<&'a UmiHost, UmiStatus> {
    if host.is_null() {
        return Err(UmiStatus::INVALID_ARGUMENT);
    }
    // SAFETY: non-null, and `size` is the first field of every table version,
    // so reading it is valid whatever size the host built.
    let size = unsafe { core::ptr::addr_of!((*host).size).read() };
    if size < size_of::<UmiHost>() {
        return Err(UmiStatus::UNSUPPORTED);
    }
    // SAFETY: the host promises `size` bytes of a valid, immutable table that
    // outlives the process (ADR-0086), and `size` covers the whole struct.
    Ok(unsafe { &*host })
}

/// The `extern "C"` handler the host calls; runs the boxed closure behind
/// `user` on a typed view of the payload.
pub(crate) extern "C" fn trampoline<E: Event>(
    event: *mut UmiEventHeader,
    user: *mut c_void,
) -> UmiStatus {
    guarded(|| {
        if event.is_null() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        // SAFETY: non-null; the host passes a live payload starting with a
        // header and keeps it valid and unaliased for the call.
        let header = unsafe { event.read() };
        if header.id != E::ID || (header.size as usize) < size_of::<E>() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        with_user(user, |handler: &mut Handler<E>| {
            let mut control = Control::new(header.flags & UMI_EVENT_CANCELLABLE != 0);
            // SAFETY: the id and size checks above show the payload holds an
            // `E` (sealed `Event` types are transparent over their payload).
            let view = unsafe { &mut *event.cast::<E>() };
            handler(view, &mut control);
            if control.is_cancelled() {
                // SAFETY: as for the read above; `view` is no longer used.
                unsafe { (*event).flags |= UMI_EVENT_CANCELLED };
            }
            UmiStatus::OK
        })
        .unwrap_or(UmiStatus::INVALID_ARGUMENT)
    })
}

/// The draw callback of every HUD widget; hands the closure a [`Hud`] that
/// lives for this call only.
pub(crate) extern "C" fn draw_trampoline(hud: *mut UmiHudCx, user: *mut c_void) -> UmiStatus {
    guarded(|| {
        let Some((host, _)) = host::current() else {
            return UmiStatus::UNSUPPORTED;
        };
        if hud.is_null() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        with_user(user, |draw: &mut DrawHandler| {
            draw(&mut Hud::new(hud, host));
            UmiStatus::OK
        })
        .unwrap_or(UmiStatus::INVALID_ARGUMENT)
    })
}

/// The run callback of every command; a failing closure is reported to the
/// host as `ERROR`, which reports it to the player.
pub(crate) extern "C" fn command_trampoline(args: UmiStr, user: *mut c_void) -> UmiStatus {
    guarded(|| {
        with_user(user, |run: &mut CommandHandler| match run(str_of(&args)) {
            Ok(()) => UmiStatus::OK,
            Err(_) => UmiStatus::ERROR,
        })
        .unwrap_or(UmiStatus::INVALID_ARGUMENT)
    })
}

/// The boxed closure type behind a physics layer's `user` pointer.
pub(crate) type PhysicsHandler = Box<dyn FnMut(&mut Physics<'_>) -> Result<(), ModError> + Send>;

/// The state the host lent for one service call (the player state of a physics
/// layer, the camera of a camera layer), seen as the safe view `T`.
///
/// Holds a raw pointer, not a reference, because the host writes through the
/// same memory during `*_next`: a reference kept across that call would alias.
/// Every access re-borrows through `&self`/`&mut self`, so the borrow checker
/// keeps `next` and the state accessors apart.
#[derive(Debug)]
pub(crate) struct StatePtr<T>(NonNull<T>);

impl<T> StatePtr<T> {
    /// # Safety
    /// `ptr` must point to a valid, host-owned state that stays valid and is not
    /// accessed by anything else, except the host during `*_next`, while the
    /// returned value (or any copy of the borrow it hands out) lives; `T` must be
    /// `#[repr(transparent)]` over `R`.
    pub(crate) unsafe fn new<R>(ptr: NonNull<R>) -> Self {
        Self(ptr.cast())
    }

    pub(crate) fn get(&self) -> &T {
        // SAFETY: `new` guarantees validity; the `&self` borrow bounds the
        // reference and excludes `get_mut` meanwhile.
        unsafe { self.0.as_ref() }
    }

    pub(crate) fn get_mut(&mut self) -> &mut T {
        // SAFETY: as in `get`; `&mut self` makes the borrow unique.
        unsafe { self.0.as_mut() }
    }
}

/// The physics callback of every layer: validates the lent state and input,
/// then runs the closure on a [`Physics`] that lives for this call only.
pub(crate) extern "C" fn physics_trampoline(
    cx: *mut UmiPhysicsCx,
    state: *mut UmiPlayerPhysics,
    input: *const UmiPhysicsInput,
    user: *mut c_void,
) -> UmiStatus {
    guarded(|| {
        let (Some(state), Some(input)) = (NonNull::new(state), NonNull::new(input.cast_mut()))
        else {
            return UmiStatus::INVALID_ARGUMENT;
        };
        // SAFETY: non-null and, per the ABI, valid for reads of its `size`
        // field's first bytes; `size` is the first field of every version.
        let sizes = unsafe { (state.as_ref().size, input.as_ref().size) };
        if (sizes.0 as usize) < size_of::<UmiPlayerPhysics>()
            || (sizes.1 as usize) < size_of::<UmiPhysicsInput>()
        {
            return UmiStatus::INVALID_ARGUMENT;
        }
        // SAFETY: the sizes cover the structs; the host keeps the input valid
        // and unmodified for the call.
        let input: &Input = unsafe { &*input.as_ptr().cast::<Input>() };
        with_user(user, |layer: &mut PhysicsHandler| {
            // SAFETY: the host lends the state exclusively for this call and
            // only touches it again inside `physics_next`.
            let state = unsafe { StatePtr::new(state) };
            match layer(&mut Physics::new(cx, state, input)) {
                Ok(()) => UmiStatus::OK,
                Err(_) => UmiStatus::ERROR,
            }
        })
        .unwrap_or(UmiStatus::INVALID_ARGUMENT)
    })
}

/// The camera callback of every layer: validates the lent camera and input, then
/// runs the closure on a [`Camera`] that lives for this call only.
pub(crate) extern "C" fn camera_trampoline(
    cx: *mut UmiCameraCx,
    camera: *mut UmiRenderCamera,
    input: *const UmiCameraInput,
    user: *mut c_void,
) -> UmiStatus {
    guarded(|| {
        let (Some(camera), Some(input)) = (NonNull::new(camera), NonNull::new(input.cast_mut()))
        else {
            return UmiStatus::INVALID_ARGUMENT;
        };
        // SAFETY: non-null and, per the ABI, valid for reads of its `size`
        // field's first bytes; `size` is the first field of every version.
        let sizes = unsafe { (camera.as_ref().size, input.as_ref().size) };
        if (sizes.0 as usize) < size_of::<UmiRenderCamera>()
            || (sizes.1 as usize) < size_of::<UmiCameraInput>()
        {
            return UmiStatus::INVALID_ARGUMENT;
        }
        // SAFETY: the sizes cover the structs; the host keeps the input valid
        // and unmodified for the call, and `CameraInput` is transparent over it.
        let input: &CameraInput = unsafe { &*input.as_ptr().cast::<CameraInput>() };
        with_user(user, |layer: &mut CameraHandler| {
            // SAFETY: the host lends the camera exclusively for this call and
            // only touches it again inside `camera_next`; `CameraState` is
            // transparent over `UmiRenderCamera`.
            let state = unsafe { StatePtr::new(camera) };
            match layer(&mut Camera::new(cx, state, input)) {
                Ok(()) => UmiStatus::OK,
                Err(_) => UmiStatus::ERROR,
            }
        })
        .unwrap_or(UmiStatus::INVALID_ARGUMENT)
    })
}

/// A visitor closure and whether it asked to stop.
pub(crate) struct Visit<'a> {
    visitor: &'a mut dyn FnMut(&Entity<'_>) -> bool,
    stopped: bool,
}

impl<'a> Visit<'a> {
    pub(crate) fn new(visitor: &'a mut dyn FnMut(&Entity<'_>) -> bool) -> Self {
        Self {
            visitor,
            stopped: false,
        }
    }

    /// Whether the visitor returned `false` or panicked.
    pub(crate) fn stopped(&self) -> bool {
        self.stopped
    }
}

/// The visit callback of `world_entities`; any non-`OK` status stops the
/// host's loop, and a panic counts as a stop.
pub(crate) extern "C" fn entity_trampoline(
    entity: *const UmiEntity,
    user: *mut c_void,
) -> UmiStatus {
    guarded(|| {
        if entity.is_null() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        // SAFETY: non-null; the host passes a live entity valid for the call
        // and `size` is the first field of every version.
        if (unsafe { (*entity).size } as usize) < size_of::<UmiEntity>() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        // SAFETY: the size check shows the whole struct is there; `UmiEntity`
        // is `Copy`, and its strings stay valid for this call.
        let raw = unsafe { entity.read() };
        with_user(user, |visit: &mut Visit<'_>| {
            // Set before the call so a panic leaves the visit stopped.
            visit.stopped = true;
            if (visit.visitor)(&Entity::new(raw)) {
                visit.stopped = false;
                UmiStatus::OK
            } else {
                UmiStatus::ERROR
            }
        })
        .unwrap_or(UmiStatus::INVALID_ARGUMENT)
    })
}
