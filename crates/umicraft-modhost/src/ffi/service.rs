// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unsafe edge shared by the services (ADR-0087, ADR-0094): the pointer to the
//! state a layer edits and the opaque `cx` its `*_next` host function takes.
//!
//! The `cx` a layer receives points at the [`LayerRun`] on the host's stack that
//! describes the running layer. It is valid for the call and the calling thread
//! only, so `*_next` compares it with the run registered as active on the current
//! thread for that service and refuses (`DENIED`) anything else: a mod that kept
//! the pointer, or passed it to another thread or to another service's `*_next`,
//! cannot reach freed memory.

use core::ffi::c_void;
use core::marker::PhantomData;
use std::panic::{catch_unwind, AssertUnwindSafe};

use umicraft_modapi_sys::UmiStatus;

use crate::service::{LayerRun, Service};

/// The state of the run in progress, as the raw pointer layers receive.
///
/// Built from an exclusive borrow, so the pointer is valid for `'a`; the host uses
/// the state only through this handle while a run is in progress, one access at a
/// time.
pub(crate) struct StatePtr<'a, T> {
    ptr: *mut T,
    _borrow: PhantomData<&'a mut T>,
}

// Derived `Clone`/`Copy` would ask `T` itself to be `Copy`; the handle is a pointer.
impl<T> Clone for StatePtr<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for StatePtr<'_, T> {}

impl<'a, T: Copy> StatePtr<'a, T> {
    /// Takes over `state` for the duration of a run.
    pub(crate) fn new(state: &'a mut T) -> Self {
        Self {
            ptr: core::ptr::from_mut(state),
            _borrow: PhantomData,
        }
    }

    /// The pointer to give to a layer.
    pub(crate) fn as_ptr(self) -> *mut T {
        self.ptr
    }

    /// A copy of the state as it is now.
    pub(crate) fn read(self) -> T {
        // SAFETY: `ptr` came from a live `&'a mut` (see `new`). A layer that holds the
        // pointer only uses it during its own call, and the host reads between
        // calls, never while a layer runs.
        unsafe { *self.ptr }
    }

    /// Overwrites the state.
    pub(crate) fn write(self, value: T) {
        // SAFETY: as in `read`.
        unsafe { *self.ptr = value };
    }

    /// Lends the state to `f` as a mutable reference.
    pub(crate) fn with_mut<R>(self, f: impl FnOnce(&mut T) -> R) -> R {
        // SAFETY: as in `read`; the reference ends with `f`, which the host calls
        // only when no layer is running on this state.
        f(unsafe { &mut *self.ptr })
    }
}

/// Restores the previously active run when a layer's call ends, even by unwinding;
/// nested layers (a wrapper calling `*_next`) each restore their parent's.
struct Restore<S: Service> {
    previous: *mut c_void,
    _service: PhantomData<S>,
}

impl<S: Service> Drop for Restore<S> {
    fn drop(&mut self) {
        S::active().set(self.previous);
    }
}

/// Makes `run` the target of the service's `*_next` while `call` runs, handing it
/// the opaque pointer a layer takes.
pub(crate) fn with_service_cx<S: Service>(
    run: &mut LayerRun<'_, '_, S>,
    call: impl FnOnce(*mut c_void) -> UmiStatus,
) -> UmiStatus {
    let cx = core::ptr::from_mut(run).cast::<c_void>();
    let _restore = Restore::<S> {
        previous: S::active().replace(cx),
        _service: PhantomData,
    };
    call(cx)
}

/// The body of a `*_next` host function: runs the rest of the chain for the layer
/// `cx` describes, if it is the one running on this thread.
pub(crate) fn service_next<S: Service>(cx: *mut c_void) -> UmiStatus {
    catch_unwind(AssertUnwindSafe(|| {
        if cx.is_null() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        if S::active().get() != cx {
            return UmiStatus::DENIED;
        }
        // SAFETY: `cx` equals the address of the `LayerRun<S>` that `with_service_cx`
        // put in this service's `active` cell for this thread and removes when the
        // layer's call ends, so it is live and of type `LayerRun<S>`. The host holds
        // it only as the raw pointer given to the layer (the `&mut` passed to
        // `with_service_cx` is not used until the call returns), and `next` is the
        // only reference made from it, ended on return.
        let run = unsafe { &mut *cx.cast::<LayerRun<'_, '_, S>>() };
        run.next()
    }))
    .unwrap_or(UmiStatus::PANICKED)
}
