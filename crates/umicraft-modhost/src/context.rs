// SPDX-License-Identifier: MIT OR Apache-2.0

//! The host's per-mod state, reached by mods through an opaque handle.

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use umicraft_modapi_sys::{UmiEventId, UmiHandlerFn, UmiLevel, UmiMode, UmiStatus};

use crate::ffi::UserPtr;
use crate::manifest::ModId;

/// A handler a mod registered through `subscribe`.
#[derive(Debug, Clone)]
pub struct Subscription {
    /// The mod that registered the handler.
    pub owner: ModId,
    /// The event it listens to.
    pub event: UmiEventId,
    /// Whether it observes or may change the event.
    pub mode: UmiMode,
    /// Lower runs first.
    pub priority: i32,
    /// The mod's callback.
    pub handler: UmiHandlerFn,
    /// Opaque pointer to give back to `handler`.
    pub user: UserPtr,
}

/// State of one loaded mod; its address is the `UmiModHandle` the mod holds.
///
/// Everything is behind a mutex or an atomic because `log` may be called from any
/// thread at any time.
#[derive(Debug)]
pub struct ModContext {
    id: ModId,
    subscriptions: Mutex<Vec<Subscription>>,
    init_done: AtomicBool,
    enabled: AtomicBool,
}

/// Longest log message forwarded, so a mod cannot flood the log with one call.
const MAX_LOG_BYTES: usize = 4096;

/// Decodes mod-provided bytes, replacing invalid UTF-8 and capping the length.
fn log_text(bytes: &[u8]) -> Cow<'_, str> {
    String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_LOG_BYTES)])
}

impl ModContext {
    /// Creates the context of mod `id`: not initialised, not enabled.
    pub fn new(id: ModId) -> Self {
        Self {
            id,
            subscriptions: Mutex::new(Vec::new()),
            init_done: AtomicBool::new(false),
            enabled: AtomicBool::new(false),
        }
    }

    /// The mod this context belongs to.
    pub fn id(&self) -> &ModId {
        &self.id
    }

    /// Whether `umi_mod_init` succeeded and the mod was not disabled since.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Subscription>> {
        // A panic while holding the lock cannot leave the list inconsistent.
        self.subscriptions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Forwards a mod's message to `tracing` with the mod id as a field.
    pub fn log(&self, level: UmiLevel, bytes: &[u8]) {
        let text = log_text(bytes);
        let mod_id = &self.id;
        match level {
            UmiLevel::ERROR => tracing::error!(%mod_id, "{text}"),
            UmiLevel::WARN => tracing::warn!(%mod_id, "{text}"),
            UmiLevel::DEBUG => tracing::debug!(%mod_id, "{text}"),
            UmiLevel::TRACE => tracing::trace!(%mod_id, "{text}"),
            // INFO and any level a newer mod invents.
            _ => tracing::info!(%mod_id, "{text}"),
        }
    }

    /// Records a handler.
    ///
    /// Refuses with `INVALID_ARGUMENT` a null handler, an unknown event or an
    /// unknown mode, and with `DENIED` any call after `init` returned: the
    /// dispatcher reads the lists without coordinating with late registrations.
    pub fn subscribe(
        &self,
        event: UmiEventId,
        mode: UmiMode,
        priority: i32,
        handler: Option<UmiHandlerFn>,
        user: UserPtr,
    ) -> UmiStatus {
        let known_event = matches!(event, UmiEventId::PRE_TICK | UmiEventId::POST_TICK);
        let known_mode = matches!(mode, UmiMode::OBSERVE | UmiMode::INTERCEPT);
        let Some(handler) = handler.filter(|_| known_event && known_mode) else {
            return UmiStatus::INVALID_ARGUMENT;
        };
        if self.init_done.load(Ordering::Acquire) {
            return UmiStatus::DENIED;
        }
        self.lock().push(Subscription {
            owner: self.id.clone(),
            event,
            mode,
            priority,
            handler,
            user,
        });
        UmiStatus::OK
    }

    /// Closes the registration window once `umi_mod_init` returned. On success the
    /// mod becomes enabled; on failure its partial registrations are dropped.
    pub fn finish_init(&self, succeeded: bool) {
        self.init_done.store(true, Ordering::Release);
        if succeeded {
            self.enabled.store(true, Ordering::Release);
        } else {
            self.lock().clear();
        }
    }

    /// Disables the mod and drops its subscriptions. Returns whether it was
    /// enabled, so the caller runs `umi_mod_shutdown` exactly once.
    pub fn disable(&self) -> bool {
        let was_enabled = self.enabled.swap(false, Ordering::AcqRel);
        if was_enabled {
            self.lock().clear();
        }
        was_enabled
    }

    /// A copy of the subscriptions to `event`, in registration order.
    pub fn subscriptions(&self, event: UmiEventId) -> Vec<Subscription> {
        self.lock()
            .iter()
            .filter(|s| s.event == event)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use core::ffi::c_void;

    use umicraft_modapi_sys::UmiEventHeader;

    use super::*;

    extern "C" fn noop(_: *mut UmiEventHeader, _: *mut c_void) -> UmiStatus {
        UmiStatus::OK
    }

    fn context() -> ModContext {
        ModContext::new("hud".to_owned().try_into().unwrap())
    }

    fn subscribe(cx: &ModContext, event: UmiEventId, handler: Option<UmiHandlerFn>) -> UmiStatus {
        cx.subscribe(
            event,
            UmiMode::OBSERVE,
            0,
            handler,
            UserPtr::new(std::ptr::null_mut()),
        )
    }

    #[test]
    fn subscribe_records_a_valid_handler() {
        let cx = context();
        assert_eq!(
            subscribe(&cx, UmiEventId::PRE_TICK, Some(noop)),
            UmiStatus::OK
        );
        assert_eq!(cx.subscriptions(UmiEventId::PRE_TICK).len(), 1);
        assert!(cx.subscriptions(UmiEventId::POST_TICK).is_empty());
    }

    #[test]
    fn subscribe_refuses_null_handler_unknown_event_and_unknown_mode() {
        let cx = context();
        assert_eq!(
            subscribe(&cx, UmiEventId::PRE_TICK, None),
            UmiStatus::INVALID_ARGUMENT
        );
        assert_eq!(
            subscribe(&cx, UmiEventId(999), Some(noop)),
            UmiStatus::INVALID_ARGUMENT
        );
        let bad_mode = cx.subscribe(
            UmiEventId::PRE_TICK,
            UmiMode(7),
            0,
            Some(noop),
            UserPtr::new(std::ptr::null_mut()),
        );
        assert_eq!(bad_mode, UmiStatus::INVALID_ARGUMENT);
        assert!(cx.subscriptions(UmiEventId::PRE_TICK).is_empty());
    }

    #[test]
    fn subscribe_after_init_is_denied() {
        let cx = context();
        cx.finish_init(true);
        assert_eq!(
            subscribe(&cx, UmiEventId::PRE_TICK, Some(noop)),
            UmiStatus::DENIED
        );
    }

    #[test]
    fn failed_init_drops_subscriptions_and_stays_disabled() {
        let cx = context();
        subscribe(&cx, UmiEventId::PRE_TICK, Some(noop));
        cx.finish_init(false);
        assert!(!cx.is_enabled());
        assert!(cx.subscriptions(UmiEventId::PRE_TICK).is_empty());
        assert!(!cx.disable());
    }

    #[test]
    fn disable_reports_the_first_call_only() {
        let cx = context();
        subscribe(&cx, UmiEventId::PRE_TICK, Some(noop));
        cx.finish_init(true);
        assert!(cx.disable());
        assert!(!cx.disable());
        assert!(cx.subscriptions(UmiEventId::PRE_TICK).is_empty());
    }

    #[test]
    fn log_text_replaces_invalid_utf8_and_caps_length() {
        assert_eq!(log_text(b"ok\xff"), "ok\u{fffd}");
        assert_eq!(log_text(&vec![b'a'; 10_000]).len(), MAX_LOG_BYTES);
    }
}
