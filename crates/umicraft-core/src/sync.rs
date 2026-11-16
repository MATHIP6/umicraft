// SPDX-License-Identifier: MIT OR Apache-2.0

//! Low-level concurrency helpers for preventing false sharing across CPU cores.

use std::ops::{Deref, DerefMut};

/// Pads and aligns a value to a 64-byte CPU cache line boundary.
///
/// Wrapping frequently mutated atomics (such as worker pool counters or packet
/// telemetry) in `CachePadded` prevents **false sharing**: two cores updating
/// independent atomics that happen to sit on the same 64-byte cache line would
/// otherwise invalidate each other's L1/L2 cache lines on every store.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(align(64))]
pub struct CachePadded<T> {
    value: T,
}

impl<T> CachePadded<T> {
    /// Pads and aligns `value` to a 64-byte cache line.
    #[inline]
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self { value }
    }

    /// Unwraps the inner value.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for CachePadded<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for CachePadded<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CachePadded").field(&self.value).finish()
    }
}

impl<T> From<T> for CachePadded<T> {
    #[inline]
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;

    #[test]
    fn cache_padded_is_64_byte_aligned() {
        assert_eq!(std::mem::align_of::<CachePadded<AtomicU64>>(), 64);
        assert!(std::mem::size_of::<CachePadded<AtomicU64>>() >= 64);
    }

    #[test]
    fn adjacent_fields_occupy_distinct_cache_lines() {
        struct Pair {
            a: CachePadded<AtomicU64>,
            b: CachePadded<AtomicU64>,
        }
        let pair = Pair {
            a: CachePadded::new(AtomicU64::new(1)),
            b: CachePadded::new(AtomicU64::new(2)),
        };
        let addr_a = std::ptr::from_ref(&pair.a) as usize;
        let addr_b = std::ptr::from_ref(&pair.b) as usize;
        assert!(addr_b.abs_diff(addr_a) >= 64);
    }
}
