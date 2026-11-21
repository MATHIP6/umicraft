// SPDX-License-Identifier: MIT OR Apache-2.0

//! The timers behind the Lagometer and the Show FPS line (ADR-0089, F6 phase 3).
//!
//! OptiFine's Lagometer keeps 512 samples per timer in ring buffers and graphs
//! them; this one follows the same idea (behaviour only, no reference code):
//! a fixed array per timer, written once per frame, so recording costs a
//! store and never allocates. The graph reads the samples oldest first through
//! [`Ring::copy_into`], into vectors the caller keeps between frames.

use std::time::Duration;

/// Samples kept per timer, about eight seconds at 60 fps.
pub(crate) const CAPACITY: usize = 512;

/// A ring buffer of the last [`CAPACITY`] timer readings, in microseconds.
#[derive(Debug, Clone)]
pub(crate) struct Ring {
    samples: [u32; CAPACITY],
    /// The slot the next sample goes to.
    next: usize,
    len: usize,
}

impl Default for Ring {
    fn default() -> Self {
        Self {
            samples: [0; CAPACITY],
            next: 0,
            len: 0,
        }
    }
}

impl Ring {
    /// Records one reading, dropping the oldest when the ring is full.
    pub(crate) fn push(&mut self, reading: Duration) {
        self.samples[self.next] = u32::try_from(reading.as_micros()).unwrap_or(u32::MAX);
        self.next = (self.next + 1) % CAPACITY;
        self.len = (self.len + 1).min(CAPACITY);
    }

    /// Replaces the content of `out` with the readings, oldest first. Does not
    /// allocate once `out` has the capacity of a full ring.
    pub(crate) fn copy_into(&self, out: &mut Vec<u32>) {
        out.clear();
        let start = (self.next + CAPACITY - self.len) % CAPACITY;
        let (tail, head) = self.samples.split_at(start);
        if self.len == CAPACITY {
            out.extend_from_slice(head);
            out.extend_from_slice(tail);
        } else {
            out.extend_from_slice(&self.samples[start..self.next]);
        }
    }
}

/// The three timers of the Lagometer.
#[derive(Debug, Clone, Default)]
pub(crate) struct Lagometer {
    pub(crate) frame: Ring,
    pub(crate) sync: Ring,
    pub(crate) upload: Ring,
}

/// The compact readout of the Show FPS option: the frame rate, the chunk
/// columns drawn (`C`), the entities drawn (`E`) and the chunk meshes uploaded
/// by the last frame (`U`), after OptiFine's `Config.drawFps` (behaviour only).
#[must_use]
pub(crate) fn compact_line(fps: f32, chunks: usize, entities: usize, updates: usize) -> String {
    format!(
        "{} fps, C: {chunks}, E: {entities}, U: {updates}",
        fps.round() as u32
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn read(ring: &Ring) -> Vec<u32> {
        let mut out = Vec::new();
        ring.copy_into(&mut out);
        out
    }

    #[test]
    fn a_partly_filled_ring_reads_oldest_first() {
        let mut ring = Ring::default();
        assert!(read(&ring).is_empty());
        ring.push(ms(1));
        ring.push(ms(2));
        assert_eq!(read(&ring), [1000, 2000]);
    }

    #[test]
    fn a_full_ring_drops_the_oldest_and_keeps_the_order() {
        let mut ring = Ring::default();
        for n in 0..CAPACITY as u64 + 3 {
            ring.push(Duration::from_micros(n));
        }
        let samples = read(&ring);
        assert_eq!(samples.len(), CAPACITY);
        assert_eq!(samples[0], 3);
        assert_eq!(samples[CAPACITY - 1], CAPACITY as u32 + 2);
    }

    #[test]
    fn the_compact_line_lists_rate_chunks_entities_and_updates() {
        assert_eq!(compact_line(59.6, 120, 7, 2), "60 fps, C: 120, E: 7, U: 2");
    }

    #[test]
    fn a_huge_reading_saturates() {
        let mut ring = Ring::default();
        ring.push(Duration::from_secs(10_000));
        assert_eq!(read(&ring), [u32::MAX]);
    }

    #[test]
    fn copying_reuses_the_capacity() {
        let mut ring = Ring::default();
        for _ in 0..CAPACITY {
            ring.push(ms(1));
        }
        let mut out = Vec::with_capacity(CAPACITY);
        let pointer = out.as_ptr();
        ring.copy_into(&mut out);
        ring.push(ms(2));
        ring.copy_into(&mut out);
        assert_eq!(out.as_ptr(), pointer);
    }
}
