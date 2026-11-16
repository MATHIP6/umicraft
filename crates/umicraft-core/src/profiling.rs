// SPDX-License-Identifier: MIT OR Apache-2.0

//! CPU profiling spans and zero-dependency heap allocation tracking (F2-0a, ADR-0059).
//!
//! With the `profiling` cargo feature, [`profile_scope!`](crate::profile_scope)
//! records a named span with [puffin](https://crates.io/crates/puffin) when
//! runtime profiling is enabled, and [`serve`] streams the data to
//! `puffin_viewer`. Without the feature the macro expands to nothing and
//! [`serve`] is a no-op, so release builds pay nothing.
//!
//! This module also provides [`CountingAllocator`], a thin wrapper around
//! [`std::alloc::System`] that counts heap allocations and bytes per frame
//! when [`set_alloc_tracking`] is enabled, with a single relaxed boolean load
//! on the disabled path.
//!
//! ```
//! umicraft_core::profile_scope!("example");
//! umicraft_core::profiling::new_frame();
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::sync::CachePadded;

/// Whether runtime profiling collection is currently active.
static PROFILING_ENABLED: CachePadded<AtomicBool> = CachePadded::new(AtomicBool::new(false));

/// Whether [`CountingAllocator`] records per-allocation atomic counters.
///
/// Placed on its own 64-byte cache line so the fast disabled path in
/// [`CountingAllocator`] reads an uncontended read-only cache line.
static ALLOC_TRACKING_ENABLED: CachePadded<AtomicBool> = CachePadded::new(AtomicBool::new(false));

static ALLOC_COUNT: CachePadded<AtomicU64> = CachePadded::new(AtomicU64::new(0));
static DEALLOC_COUNT: CachePadded<AtomicU64> = CachePadded::new(AtomicU64::new(0));
static REALLOC_COUNT: CachePadded<AtomicU64> = CachePadded::new(AtomicU64::new(0));
static ALLOC_BYTES: CachePadded<AtomicU64> = CachePadded::new(AtomicU64::new(0));
static DEALLOC_BYTES: CachePadded<AtomicU64> = CachePadded::new(AtomicU64::new(0));

/// Opens a span named `$name` that ends at the end of the enclosing block.
///
/// The name must be a string literal (puffin stores the pointer, not a copy).
#[macro_export]
macro_rules! profile_scope {
    ($name:expr) => {
        #[cfg(feature = "profiling")]
        $crate::profiling::puffin::profile_scope!($name);
    };
}

/// Re-export so [`profile_scope!`] works in crates that do not depend on puffin.
#[cfg(feature = "profiling")]
#[doc(hidden)]
pub use puffin;

/// Returns whether runtime profiling is currently enabled.
#[inline]
#[must_use]
pub fn is_enabled() -> bool {
    PROFILING_ENABLED.load(Ordering::Relaxed)
}

/// Enables or disables runtime profiling (and allocation tracking) at once.
pub fn set_enabled(enabled: bool) {
    PROFILING_ENABLED.store(enabled, Ordering::Relaxed);
    set_alloc_tracking(enabled);
    #[cfg(feature = "profiling")]
    puffin::set_scopes_on(enabled);
}

/// Toggles runtime profiling and returns the new state.
pub fn toggle() -> bool {
    let next = !is_enabled();
    set_enabled(next);
    next
}

/// Returns whether per-allocation counter tracking is currently enabled.
#[inline]
#[must_use]
pub fn alloc_tracking_enabled() -> bool {
    ALLOC_TRACKING_ENABLED.load(Ordering::Relaxed)
}

/// Enables or disables heap allocation counting in [`CountingAllocator`].
pub fn set_alloc_tracking(enabled: bool) {
    ALLOC_TRACKING_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Marks the end of a frame so the viewer can split the timeline.
///
/// Call once per rendered frame (or game tick for the game thread).
pub fn new_frame() {
    #[cfg(feature = "profiling")]
    if is_enabled() {
        puffin::GlobalProfiler::lock().new_frame();
    }
}

/// Turns the profiler on and serves its data on `127.0.0.1:8585`, the
/// `puffin_viewer` default.
///
/// Returns the server guard; dropping it stops the stream. `None` when the
/// feature is off or the port is busy (a warning is logged in that case).
#[must_use]
pub fn serve() -> Option<Box<dyn std::any::Any>> {
    #[cfg(feature = "profiling")]
    {
        match puffin_http::Server::new("127.0.0.1:8585") {
            Ok(server) => {
                set_enabled(true);
                tracing::info!("puffin server on 127.0.0.1:8585 (run `puffin_viewer`)");
                return Some(Box::new(server));
            }
            Err(error) => tracing::warn!("cannot start the puffin server: {error}"),
        }
    }
    None
}

/// Monotonic snapshot of heap allocation counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AllocSnapshot {
    /// Total `alloc` calls recorded while tracking was enabled.
    pub alloc_count: u64,
    /// Total `dealloc` calls recorded while tracking was enabled.
    pub dealloc_count: u64,
    /// Total `realloc` calls recorded while tracking was enabled.
    pub realloc_count: u64,
    /// Total bytes requested through `alloc` and `realloc` growth.
    pub alloc_bytes: u64,
    /// Total bytes released through `dealloc` and `realloc` shrinkage.
    pub dealloc_bytes: u64,
}

impl AllocSnapshot {
    /// Reads the current allocation counters.
    #[inline]
    #[must_use]
    pub fn now() -> Self {
        Self {
            alloc_count: ALLOC_COUNT.load(Ordering::Relaxed),
            dealloc_count: DEALLOC_COUNT.load(Ordering::Relaxed),
            realloc_count: REALLOC_COUNT.load(Ordering::Relaxed),
            alloc_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
            dealloc_bytes: DEALLOC_BYTES.load(Ordering::Relaxed),
        }
    }

    /// Computes the difference between `self` (later) and `earlier`.
    #[inline]
    #[must_use]
    pub fn delta_since(self, earlier: Self) -> AllocDelta {
        AllocDelta {
            allocs: self.alloc_count.saturating_sub(earlier.alloc_count),
            deallocs: self.dealloc_count.saturating_sub(earlier.dealloc_count),
            reallocs: self.realloc_count.saturating_sub(earlier.realloc_count),
            bytes_allocated: self.alloc_bytes.saturating_sub(earlier.alloc_bytes),
            bytes_deallocated: self.dealloc_bytes.saturating_sub(earlier.dealloc_bytes),
        }
    }
}

/// Heap allocation activity between two [`AllocSnapshot`]s (e.g. one frame).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AllocDelta {
    /// Number of `alloc` calls in the interval.
    pub allocs: u64,
    /// Number of `dealloc` calls in the interval.
    pub deallocs: u64,
    /// Number of `realloc` calls in the interval.
    pub reallocs: u64,
    /// Bytes allocated in the interval.
    pub bytes_allocated: u64,
    /// Bytes deallocated in the interval.
    pub bytes_deallocated: u64,
}

impl AllocDelta {
    /// Total allocation operations (`allocs + reallocs`).
    #[inline]
    #[must_use]
    pub fn total_ops(&self) -> u64 {
        self.allocs + self.reallocs
    }
}

/// Resets all allocation counters to zero (useful before a benchmark run).
pub fn reset_alloc_counters() {
    ALLOC_COUNT.store(0, Ordering::Relaxed);
    DEALLOC_COUNT.store(0, Ordering::Relaxed);
    REALLOC_COUNT.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
    DEALLOC_BYTES.store(0, Ordering::Relaxed);
}

/// Global allocator wrapper around [`std::alloc::System`] that counts heap
/// allocations when [`alloc_tracking_enabled`] is true.
///
/// When tracking is disabled (the default in production), each call performs
/// a single relaxed load of a static boolean before delegating to [`System`].
#[derive(Debug, Default, Clone, Copy)]
pub struct CountingAllocator;

impl CountingAllocator {
    /// Creates a new counting allocator instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for CountingAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ALLOC_TRACKING_ENABLED.load(Ordering::Relaxed) {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: Forwarded directly to the standard system allocator.
        unsafe { System.alloc(layout) }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ALLOC_TRACKING_ENABLED.load(Ordering::Relaxed) {
            DEALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            DEALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: Forwarded directly to the standard system allocator.
        unsafe { System.dealloc(ptr, layout) }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ALLOC_TRACKING_ENABLED.load(Ordering::Relaxed) {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: Forwarded directly to the standard system allocator.
        unsafe { System.alloc_zeroed(layout) }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ALLOC_TRACKING_ENABLED.load(Ordering::Relaxed) {
            REALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            if new_size >= layout.size() {
                ALLOC_BYTES.fetch_add((new_size - layout.size()) as u64, Ordering::Relaxed);
            } else {
                DEALLOC_BYTES.fetch_add((layout.size() - new_size) as u64, Ordering::Relaxed);
            }
        }
        // SAFETY: Forwarded directly to the standard system allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_compile_and_frames_advance_without_a_viewer() {
        crate::profile_scope!("test");
        super::new_frame();
        // Port availability is not guaranteed in CI: only check it never panics.
        drop(super::serve());
    }

    #[test]
    fn toggle_switches_the_profiling_and_alloc_tracking_state() {
        set_enabled(false);
        assert!(!is_enabled());
        assert!(!alloc_tracking_enabled());
        assert!(toggle());
        assert!(is_enabled());
        assert!(alloc_tracking_enabled());
        assert!(!toggle());
        assert!(!is_enabled());
    }

    #[test]
    fn delta_since_computes_saturating_differences() {
        let earlier = AllocSnapshot {
            alloc_count: 10,
            dealloc_count: 8,
            realloc_count: 2,
            alloc_bytes: 1024,
            dealloc_bytes: 512,
        };
        let later = AllocSnapshot {
            alloc_count: 25,
            dealloc_count: 20,
            realloc_count: 5,
            alloc_bytes: 4096,
            dealloc_bytes: 2048,
        };
        let delta = later.delta_since(earlier);
        assert_eq!(delta.allocs, 15);
        assert_eq!(delta.deallocs, 12);
        assert_eq!(delta.reallocs, 3);
        assert_eq!(delta.total_ops(), 18);
        assert_eq!(delta.bytes_allocated, 3072);
        assert_eq!(delta.bytes_deallocated, 1536);
    }
}
