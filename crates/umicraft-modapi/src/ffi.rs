// SPDX-License-Identifier: MIT OR Apache-2.0

//! All `unsafe` of the crate: reading host-provided pointers and owning the
//! boxed handler closures behind the `user` pointers (ADR-0087).
#![allow(unsafe_code)]

use core::ffi::c_void;
use std::mem::size_of;

use umicraft_modapi_sys::{
    UmiEventHeader, UmiHost, UmiModHandle, UmiStatus, UMI_EVENT_CANCELLABLE, UMI_EVENT_CANCELLED,
};

use crate::event::{Control, Event};
use crate::private::guarded;

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

/// The boxed closure type behind every `user` pointer.
pub(crate) type Handler<E> = Box<dyn FnMut(&mut E, &mut Control) + Send>;

/// A boxed handler handed to the host as `user`; freed on drop.
#[derive(Debug)]
pub(crate) struct OwnedHandler {
    user: *mut c_void,
    free: unsafe fn(*mut c_void),
}

// SAFETY: the pointer owns a `Box<Handler<E>>`, and handlers are `Send`.
unsafe impl Send for OwnedHandler {}

impl OwnedHandler {
    pub(crate) fn new<E: Event>(handler: Handler<E>) -> Self {
        Self {
            user: Box::into_raw(Box::new(handler)).cast(),
            free: free_handler::<E>,
        }
    }

    /// The pointer to pass to `subscribe`; valid until `self` drops.
    pub(crate) fn user(&self) -> *mut c_void {
        self.user
    }
}

impl Drop for OwnedHandler {
    fn drop(&mut self) {
        // SAFETY: `user` came from `Box::into_raw` in `new` with the type
        // `free` was instantiated for, and `drop` runs once.
        unsafe { (self.free)(self.user) }
    }
}

/// # Safety
/// `user` must come from [`OwnedHandler::new`] for the same `E`, unfreed.
unsafe fn free_handler<E: Event>(user: *mut c_void) {
    // SAFETY: guaranteed by the caller.
    drop(unsafe { Box::from_raw(user.cast::<Handler<E>>()) });
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
        if event.is_null() || user.is_null() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        // SAFETY: non-null; the host passes a live payload starting with a
        // header and keeps it valid and unaliased for the call.
        let header = unsafe { event.read() };
        if header.id != E::ID || (header.size as usize) < size_of::<E>() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        let mut control = Control::new(header.flags & UMI_EVENT_CANCELLABLE != 0);
        // SAFETY: `user` is the pointer `OwnedHandler::new::<E>` produced and
        // the host passes it back for this subscription only; it stays alive
        // until shutdown and the host does not call a handler concurrently
        // with itself.
        let handler = unsafe { &mut *user.cast::<Handler<E>>() };
        // SAFETY: the id and size checks above show the payload holds an `E`
        // (sealed `Event` types are transparent over their payload).
        let view = unsafe { &mut *event.cast::<E>() };
        handler(view, &mut control);
        if control.is_cancelled() {
            // SAFETY: as for the read above; `view` is no longer used.
            unsafe { (*event).flags |= UMI_EVENT_CANCELLED };
        }
        UmiStatus::OK
    })
}
