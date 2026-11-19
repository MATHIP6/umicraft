// SPDX-License-Identifier: MIT OR Apache-2.0

//! Logging through the host.

use umicraft_modapi_sys::UmiLevel;

use crate::host;

/// Sends messages to the host's log, tagged with the mod's id by the host.
///
/// Cheap to copy and usable from any thread: it reads the host table kept by
/// the mod-side static (see the `host` module). Before `init` it does nothing.
#[derive(Debug, Clone, Copy)]
pub struct Logger;

impl Logger {
    fn send(self, level: UmiLevel, msg: &str) {
        // A refusal means "no host yet": there is nowhere to log to.
        let _ = host::call(|host, me| (host.log)(me, level, host::utf8(msg)));
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
