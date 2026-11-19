//! Implementation behind `umicraft_mod!`; not public API.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, MutexGuard, PoisonError};

use umicraft_modapi_sys::UMI_API_VERSION;
pub use umicraft_modapi_sys::{UmiHost, UmiModHandle, UmiStatus, UmiVersion};

use crate::ffi::{self, OwnedHandler};
use crate::{InitCx, Mod};

/// Runs `body`, turning a panic into [`UmiStatus::PANICKED`]: unwinding out
/// of an `extern "C"` function aborts the whole client.
pub(crate) fn guarded(body: impl FnOnce() -> UmiStatus) -> UmiStatus {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(UmiStatus::PANICKED)
}

struct Running<M> {
    instance: M,
    // Kept alive until after `Mod::shutdown`: the host may still call them.
    _handlers: Vec<OwnedHandler>,
}

/// Holds the mod between `umi_mod_init` and `umi_mod_shutdown`.
pub struct ModState<M> {
    running: Mutex<Option<Running<M>>>,
}

impl<M: Mod> ModState<M> {
    /// An empty state, usable in a `static`.
    pub const fn new() -> Self {
        Self {
            running: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Running<M>>> {
        // A panic in a mod must not wedge later calls; the data stays valid.
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn init(&self, host: *const UmiHost, me: UmiModHandle) -> UmiStatus {
        let host = match ffi::host_ref(host) {
            Ok(host) if host.version.accepts(UMI_API_VERSION) => host,
            Ok(_) => return UmiStatus::UNSUPPORTED,
            Err(status) => return status,
        };
        let mut slot = self.lock();
        if slot.is_some() {
            return UmiStatus::CONFLICT;
        }
        let mut cx = InitCx::new(host, me);
        match M::init(&mut cx) {
            Ok(instance) => {
                *slot = Some(Running {
                    instance,
                    _handlers: cx.into_handlers(),
                });
                UmiStatus::OK
            }
            Err(error) => {
                cx.logger().error(&error.to_string());
                UmiStatus::ERROR
            }
        }
    }

    fn shutdown(&self) {
        if let Some(mut running) = self.lock().take() {
            running.instance.shutdown();
        }
    }
}

impl<M: Mod> Default for ModState<M> {
    fn default() -> Self {
        Self::new()
    }
}

/// Body of `umi_mod_api_version`.
pub fn api_version() -> UmiVersion {
    UMI_API_VERSION
}

/// Body of `umi_mod_init`.
pub fn init<M: Mod>(state: &ModState<M>, host: *const UmiHost, me: UmiModHandle) -> UmiStatus {
    guarded(|| state.init(host, me))
}

/// Body of `umi_mod_shutdown`.
pub fn shutdown<M: Mod>(state: &ModState<M>) {
    // A panicking `Mod::shutdown` has nowhere to report to; containing it is
    // what matters.
    let _ = guarded(|| {
        state.shutdown();
        UmiStatus::OK
    });
}
