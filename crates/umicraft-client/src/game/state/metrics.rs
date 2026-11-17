// SPDX-License-Identifier: MIT OR Apache-2.0

//! The timings of the game thread that the F3 screen shows.

use std::time::Instant;

/// The timings of the game thread: tick, network and input times and the TPS.
pub(in crate::game) struct TickMetrics {
    /// Execution time of the previous 20 Hz simulation tick, in microseconds.
    pub(in crate::game) last_tick_micros: u32,
    /// Maximum tick execution time observed, in microseconds.
    pub(in crate::game) max_tick_micros: u32,
    /// Delay of the latest tick from its scheduled instant, in microseconds.
    pub(in crate::game) last_tick_delay_us: u32,
    /// Maximum tick delay observed, in microseconds.
    pub(in crate::game) max_tick_delay_us: u32,
    /// Cumulative catch-up simulation ticks executed.
    pub(in crate::game) catch_up_ticks_total: u64,
    /// Measured simulation TPS.
    pub(in crate::game) real_tps: f32,
    /// Last instant TPS was recalculated.
    pub(in crate::game) last_tps_instant: Instant,
    /// Tick count at last TPS recalculation.
    pub(in crate::game) last_tps_ticks: u64,
    /// Current number of unread events in the network inbox.
    pub(in crate::game) net_queue_len: usize,
    /// Cumulative microseconds spent handling network events since the last tick.
    pub(in crate::game) net_micros_accum: u32,
    /// Microseconds spent handling network events in the previous tick window.
    pub(in crate::game) last_net_micros: u32,
    /// Execution time of the most recent input handling pass, in microseconds.
    pub(in crate::game) last_input_processing_us: u32,
    /// Queuing latency of the most recent input update, in microseconds.
    pub(in crate::game) last_input_queue_latency_us: u32,
}

impl Default for TickMetrics {
    fn default() -> Self {
        Self {
            last_tick_micros: 0,
            max_tick_micros: 0,
            last_tick_delay_us: 0,
            max_tick_delay_us: 0,
            catch_up_ticks_total: 0,
            real_tps: 20.0,
            last_tps_instant: Instant::now(),
            last_tps_ticks: 0,
            net_queue_len: 0,
            net_micros_accum: 0,
            last_net_micros: 0,
            last_input_processing_us: 0,
            last_input_queue_latency_us: 0,
        }
    }
}
