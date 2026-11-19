// SPDX-License-Identifier: MIT OR Apache-2.0

//! All `unsafe` of the crate: reading host-provided pointers and owning the
//! boxed closures behind the `user` pointers (ADR-0087).
#![allow(unsafe_code)]

use core::ffi::c_void;
use std::mem::size_of;

use umicraft_modapi_sys::{
    UmiEventHeader, UmiHost, UmiHudCx, UmiModHandle, UmiStatus, UmiStr, UMI_EVENT_CANCELLABLE,
    UMI_EVENT_CANCELLED,
};

use crate::event::{Control, Event};
use crate::private::guarded;
use crate::{host, Hud, ModError};

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
