// SPDX-License-Identifier: MIT OR Apache-2.0

//! Download progress reporting (spec AST-1, "show progress").
//!
//! The transport layer never talks to the UI directly: it updates a
//! [`Progress`] handle, and the UI polls [`Progress::snapshot`] whenever it
//! renders a frame. The handle is cloneable and cheap to clone, so the
//! download workers and the UI can share the same state without a lock on a
//! hot path.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Shared progress state of an asset download.
///
/// Cloning a `Progress` shares the same counters.
///
/// # Examples
///
/// ```
/// use umicraft_assets::progress::Progress;
///
/// let progress = Progress::new();
/// progress.set_bytes_total(100);
/// progress.add_bytes(40);
/// assert_eq!(progress.snapshot().fraction(), 0.4);
/// ```
#[derive(Debug, Clone, Default)]
pub struct Progress {
    inner: Arc<ProgressInner>,
}

#[derive(Debug, Default)]
struct ProgressInner {
    bytes_done: AtomicU64,
    bytes_total: AtomicU64,
    objects_done: AtomicU64,
    objects_total: AtomicU64,
    current: Mutex<Option<String>>,
}

impl Progress {
    /// A fresh, empty progress state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the number of bytes the run is expected to transfer.
    pub fn set_bytes_total(&self, total: u64) {
        self.inner.bytes_total.store(total, Ordering::Relaxed);
    }

    /// Adds `bytes` to the transferred counter.
    pub fn add_bytes(&self, bytes: u64) {
        self.inner.bytes_done.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Sets the number of objects the run is expected to transfer.
    pub fn set_objects_total(&self, total: u64) {
        self.inner.objects_total.store(total, Ordering::Relaxed);
    }

    /// Marks one object as finished.
    pub fn object_done(&self) {
        self.inner.objects_done.fetch_add(1, Ordering::Relaxed);
    }

    /// Sets (or clears) the object that is currently being transferred.
    pub fn set_current(&self, name: Option<String>) {
        if let Ok(mut current) = self.inner.current.lock() {
            *current = name;
        }
    }

    /// Resets every counter.
    pub fn reset(&self) {
        self.inner.bytes_done.store(0, Ordering::Relaxed);
        self.inner.bytes_total.store(0, Ordering::Relaxed);
        self.inner.objects_done.store(0, Ordering::Relaxed);
        self.inner.objects_total.store(0, Ordering::Relaxed);
        self.set_current(None);
    }

    /// The current state, as the UI sees it.
    #[must_use]
    pub fn snapshot(&self) -> ProgressSnapshot {
        let current = self
            .inner
            .current
            .lock()
            .ok()
            .and_then(|current| current.clone());
        ProgressSnapshot {
            bytes_done: self.inner.bytes_done.load(Ordering::Relaxed),
            bytes_total: self.inner.bytes_total.load(Ordering::Relaxed),
            objects_done: self.inner.objects_done.load(Ordering::Relaxed),
            objects_total: self.inner.objects_total.load(Ordering::Relaxed),
            current_object: current,
        }
    }
}

/// An immutable view of the download progress.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgressSnapshot {
    /// Bytes transferred so far.
    pub bytes_done: u64,
    /// Bytes the run is expected to transfer in total.
    pub bytes_total: u64,
    /// Objects finished so far.
    pub objects_done: u64,
    /// Objects the run is expected to transfer in total.
    pub objects_total: u64,
    /// The object that is currently being transferred, if any.
    pub current_object: Option<String>,
}

impl ProgressSnapshot {
    /// The completed fraction of the run, in `0.0 ..= 1.0`.
    ///
    /// Returns `0.0` while the total is unknown.
    #[must_use]
    pub fn fraction(&self) -> f64 {
        if self.bytes_total == 0 {
            return 0.0;
        }
        (self.bytes_done as f64 / self.bytes_total as f64).clamp(0.0, 1.0)
    }

    /// Whether every expected object has been transferred.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.objects_total > 0 && self.objects_done >= self.objects_total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_are_shared_between_clones() {
        let progress = Progress::new();
        let clone = progress.clone();
        clone.add_bytes(10);
        assert_eq!(progress.snapshot().bytes_done, 10);
    }

    #[test]
    fn the_current_object_is_visible_in_the_snapshot() {
        let progress = Progress::new();
        progress.set_current(Some("minecraft/sounds/step/grass1.ogg".to_string()));
        assert_eq!(
            progress.snapshot().current_object.as_deref(),
            Some("minecraft/sounds/step/grass1.ogg")
        );
        progress.set_current(None);
        assert_eq!(progress.snapshot().current_object, None);
    }

    #[test]
    fn fraction_is_zero_without_a_known_total() {
        assert_eq!(ProgressSnapshot::default().fraction(), 0.0);
    }

    #[test]
    fn reset_clears_every_counter() {
        let progress = Progress::new();
        progress.set_bytes_total(10);
        progress.add_bytes(10);
        progress.set_objects_total(2);
        progress.object_done();
        progress.reset();
        let snapshot = progress.snapshot();
        assert_eq!(snapshot.bytes_done, 0);
        assert_eq!(snapshot.bytes_total, 0);
        assert_eq!(snapshot.objects_done, 0);
        assert!(!snapshot.is_complete());
    }
}
