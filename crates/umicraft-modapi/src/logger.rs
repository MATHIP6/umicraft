//! Logging through the host.

use umicraft_modapi_sys::{UmiLevel, UmiModHandle, UmiStr};

use crate::ffi::SharedHandle;

type LogFn = extern "C" fn(UmiModHandle, UmiLevel, UmiStr);

/// Sends messages to the host's log, tagged with the mod's id by the host.
///
/// Cheap to copy and usable from any thread: the host table and the mod
/// context live for the whole process (ADR-0086: unload means disable).
#[derive(Debug, Clone, Copy)]
pub struct Logger {
    log: LogFn,
    me: SharedHandle,
}

impl Logger {
    pub(crate) fn new(log: LogFn, me: UmiModHandle) -> Self {
        Self {
            log,
            me: SharedHandle::new(me),
        }
    }

    fn send(&self, level: UmiLevel, msg: &str) {
        // The `UmiStr` only borrows `msg` for the duration of the call, as the
        // ABI requires.
        let msg = UmiStr {
            ptr: msg.as_ptr(),
            len: msg.len(),
        };
        (self.log)(self.me.get(), level, msg);
    }

    /// Logs something that failed.
    pub fn error(&self, msg: &str) {
        self.send(UmiLevel::ERROR, msg);
    }

    /// Logs something suspicious.
    pub fn warn(&self, msg: &str) {
        self.send(UmiLevel::WARN, msg);
    }

    /// Logs normal progress.
    pub fn info(&self, msg: &str) {
        self.send(UmiLevel::INFO, msg);
    }

    /// Logs developer diagnostics.
    pub fn debug(&self, msg: &str) {
        self.send(UmiLevel::DEBUG, msg);
    }

    /// Logs very verbose diagnostics.
    pub fn trace(&self, msg: &str) {
        self.send(UmiLevel::TRACE, msg);
    }
}
