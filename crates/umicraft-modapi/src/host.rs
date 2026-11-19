// SPDX-License-Identifier: MIT OR Apache-2.0

//! The host table and this mod's handle, kept for everything that runs after
//! `init` returned.
//!
//! Event views are transparent casts of the payload, and settings, chat and
//! the logger are `Copy` handles, so none has room for a pointer to the host
//! table. It lives in a process-wide static instead. That is sound on the mod
//! side: every mod library links its own copy of this crate, and one library
//! is one mod talking to one host (ADR-0086). The host side keeps no such
//! global; it finds the mod through the handle.

use std::sync::{PoisonError, RwLock};

use umicraft_modapi_sys::{UmiHost, UmiModHandle, UmiStatus, UmiStr};

use crate::ffi::SharedHandle;
use crate::ModError;

#[derive(Clone, Copy)]
struct Remembered {
    host: UmiHost,
    me: SharedHandle,
}

/// Replaced (not set-once) so a mod that is re-initialised after a shutdown
/// talks to its current host.
static HOST: RwLock<Option<Remembered>> = RwLock::new(None);

/// Keeps a copy of the host table and the mod's handle for the life of the
/// library. Called once per `umi_mod_init`, before `Mod::init` runs.
pub(crate) fn remember(host: &UmiHost, me: UmiModHandle) {
    *HOST.write().unwrap_or_else(PoisonError::into_inner) = Some(Remembered {
        host: *host,
        me: SharedHandle::new(me),
    });
}

/// A copy of the remembered table and handle, or `None` before `init`.
///
/// A copy, not a guard: the lock is not held while the host runs, so a host
/// function may call back into the mod without deadlocking.
pub(crate) fn current() -> Option<(UmiHost, UmiModHandle)> {
    let remembered = (*HOST.read().unwrap_or_else(PoisonError::into_inner))?;
    Some((remembered.host, remembered.me.get()))
}

/// Runs `call` with the host table and handle.
///
/// # Errors
/// `HostRefused(UNSUPPORTED)` before `init`: there is no host to talk to.
pub(crate) fn call<R>(call: impl FnOnce(&UmiHost, UmiModHandle) -> R) -> Result<R, ModError> {
    let (host, me) = current().ok_or(ModError::HostRefused(UmiStatus::UNSUPPORTED))?;
    Ok(call(&host, me))
}

/// Borrows `text` as an ABI string for one call, as the ABI requires.
pub(crate) fn utf8(text: &str) -> UmiStr {
    UmiStr {
        ptr: text.as_ptr(),
        len: text.len(),
    }
}
