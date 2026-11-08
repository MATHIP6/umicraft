// SPDX-License-Identifier: MIT OR Apache-2.0

//! GPU time per group of draws (F2-0b).
//!
//! The frame is one render pass, so the groups are timed with
//! `write_timestamp` between draws. The adapter must offer
//! `TIMESTAMP_QUERY` and `TIMESTAMP_QUERY_INSIDE_PASSES`; otherwise
//! [`GpuTimer::new`] returns `None` and the F3 screen shows "n/a".
//!
//! Results come back two to three frames late through a small ring of
//! readback buffers, polled without blocking, so the timer never stalls the
//! frame. A frame is skipped when every slot is still in flight.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The groups of draws, in frame order.
pub const GROUP_NAMES: [&str; 5] = ["sky", "opaque", "entities", "translucent", "ui"];
/// Timestamps per frame: the start plus the end of every group.
const STAMPS: u32 = GROUP_NAMES.len() as u32 + 1;
/// Bytes of one resolved frame (`u64` per stamp).
const FRAME_BYTES: u64 = STAMPS as u64 * 8;
/// Frames in flight before a new one is skipped.
const SLOTS: usize = 3;

/// The device features [`GpuTimer`] needs.
pub(crate) const REQUIRED_FEATURES: wgpu::Features = wgpu::Features::TIMESTAMP_QUERY
    .union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS)
    .union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES);

/// Milliseconds the GPU spent on each group of the last measured frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuTimings {
    /// One entry per name of [`GROUP_NAMES`].
    pub ms: [f32; GROUP_NAMES.len()],
}

impl GpuTimings {
    /// The sum of the groups.
    #[must_use]
    pub fn total_ms(&self) -> f32 {
        self.ms.iter().sum()
    }

    /// Converts raw GPU ticks into milliseconds.
    ///
    /// `period_ns` is the duration of one tick (`Queue::get_timestamp_period`).
    /// A stamp that went backwards (a counter glitch) counts as zero.
    fn from_ticks(stamps: &[u64], period_ns: f32) -> Self {
        let mut ms = [0.0; GROUP_NAMES.len()];
        for (slot, pair) in ms.iter_mut().zip(stamps.windows(2)) {
            let ticks = pair[1].saturating_sub(pair[0]);
            *slot = ticks as f32 * period_ns / 1.0e6;
        }
        Self { ms }
    }
}

struct Slot {
    buffer: wgpu::Buffer,
    /// Set by the map callback once the data can be read.
    ready: Arc<AtomicBool>,
    in_flight: bool,
}

/// Timestamp queries and their readback ring.
pub(crate) struct GpuTimer {
    queries: wgpu::QuerySet,
    resolved: wgpu::Buffer,
    slots: Vec<Slot>,
    /// The slot the frame being recorded resolves into.
    current: Option<usize>,
    period_ns: f32,
    latest: Option<GpuTimings>,
}

impl GpuTimer {
    /// Creates the timer when the device was created with
    /// [`REQUIRED_FEATURES`].
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(REQUIRED_FEATURES) {
            return None;
        }
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("frame timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: STAMPS,
        });
        let resolved = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timestamp resolve"),
            size: FRAME_BYTES,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let slots = (0..SLOTS)
            .map(|_| Slot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("timestamp readback"),
                    size: FRAME_BYTES,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                ready: Arc::new(AtomicBool::new(false)),
                in_flight: false,
            })
            .collect();
        Some(Self {
            queries,
            resolved,
            slots,
            current: None,
            period_ns: queue.get_timestamp_period(),
            latest: None,
        })
    }

    /// Picks the readback slot of the frame about to be recorded.
    pub(crate) fn begin_frame(&mut self) {
        self.current = self.slots.iter().position(|slot| !slot.in_flight);
    }

    /// Writes the stamp `index` (0 = start of the frame, `n` = end of group
    /// `n - 1`). Does nothing when every slot is in flight.
    pub(crate) fn stamp(&self, pass: &mut wgpu::RenderPass<'_>, index: u32) {
        if self.current.is_some() {
            pass.write_timestamp(&self.queries, index);
        }
    }

    /// Copies the stamps into the frame's readback slot.
    pub(crate) fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(index) = self.current else {
            return;
        };
        encoder.resolve_query_set(&self.queries, 0..STAMPS, &self.resolved, 0);
        let slot = &mut self.slots[index];
        encoder.copy_buffer_to_buffer(&self.resolved, 0, &slot.buffer, 0, FRAME_BYTES);
        slot.in_flight = true;
    }

    /// Starts mapping the slot of the submitted frame and reads the slots
    /// that finished. Never blocks.
    pub(crate) fn after_submit(&mut self, device: &wgpu::Device) {
        if let Some(index) = self.current.take() {
            let slot = &self.slots[index];
            let ready = Arc::clone(&slot.ready);
            slot.buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    ready.store(result.is_ok(), Ordering::Release);
                });
        }
        // A failed poll only delays the result: the next frame polls again.
        let _ = device.poll(wgpu::PollType::Poll);
        for slot in &mut self.slots {
            if slot.in_flight && slot.ready.swap(false, Ordering::Acquire) {
                let stamps = read_stamps(&slot.buffer);
                slot.buffer.unmap();
                slot.in_flight = false;
                self.latest = Some(GpuTimings::from_ticks(&stamps, self.period_ns));
            }
        }
    }

    /// The newest finished measurement.
    pub(crate) fn latest(&self) -> Option<GpuTimings> {
        self.latest
    }
}

/// Reads the mapped slot; the buffer must be mapped.
fn read_stamps(buffer: &wgpu::Buffer) -> Vec<u64> {
    let Ok(mapped) = buffer.slice(..).get_mapped_range() else {
        return Vec::new();
    };
    mapped
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap_or([0; 8])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_become_milliseconds_per_group() {
        // 1 tick = 1000 ns: groups of 1000, 2000, 0, 500 and 500 ticks.
        let stamps = [0, 1000, 3000, 3000, 3500, 4000];
        let timings = GpuTimings::from_ticks(&stamps, 1000.0);
        assert_eq!(timings.ms, [1.0, 2.0, 0.0, 0.5, 0.5]);
        assert!((timings.total_ms() - 4.0).abs() < 1e-6);
    }

    #[test]
    fn a_backwards_stamp_counts_as_zero() {
        let timings = GpuTimings::from_ticks(&[10, 5, 15, 15, 15, 15], 1.0e6);
        assert_eq!(timings.ms[0], 0.0);
        assert_eq!(timings.ms[1], 10.0);
    }

    #[test]
    fn missing_stamps_leave_the_groups_at_zero() {
        assert_eq!(GpuTimings::from_ticks(&[], 1.0), GpuTimings::default());
    }
}
