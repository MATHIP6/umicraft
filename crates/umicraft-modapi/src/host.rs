// SPDX-License-Identifier: MIT OR Apache-2.0

//! Host functions that event views call after `init` returned.
//!
//! A view is a transparent cast of the payload, so it has no room for a
//! pointer to the host table. The function lives in a process-wide static
//! instead. That is sound on the mod side: every mod library links its own copy
//! of this crate, and one library is one mod talking to one host (ADR-0086).
//! The host side keeps no such global.

use std::sync::{PoisonError, RwLock};

use umicraft_modapi_sys::{UmiEventHeader, UmiHost, UmiStatus, UmiStr};

use crate::ModError;

type SetTextFn = extern "C" fn(*mut UmiEventHeader, UmiStr) -> UmiStatus;

/// Replaced (not set-once) so a mod that is re-initialised after a shutdown
/// talks to its current host.
static SET_TEXT: RwLock<Option<SetTextFn>> = RwLock::new(None);

/// Keeps what views need from `host` for the life of the library.
pub(crate) fn remember(host: &UmiHost) {
    *SET_TEXT.write().unwrap_or_else(PoisonError::into_inner) = Some(host.event_set_text);
}

/// Replaces the text of the event at `event` through the host.
///
/// The host is the authority on whether the event is mutable, so this does not
/// check the flag itself and cannot disagree with it.
pub(crate) fn set_text(event: *mut UmiEventHeader, text: &str) -> Result<(), ModError> {
    let set_text = (*SET_TEXT.read().unwrap_or_else(PoisonError::into_inner))
        .ok_or(ModError::HostRefused(UmiStatus::UNSUPPORTED))?;
    let text = UmiStr {
        ptr: text.as_ptr(),
        len: text.len(),
    };
    match set_text(event, text) {
        status if status.is_ok() => Ok(()),
        status => Err(ModError::HostRefused(status)),
    }
}
