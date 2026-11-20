// SPDX-License-Identifier: MIT OR Apache-2.0

//! Time budget and statistics of a mod's handlers (ADR-0086).
//!
//! An intercept handler runs on the thread that owns the event, so a slow one
//! delays the game or the frame. The host measures every call: one over its
//! budget ([`BUDGET`] for handlers and commands, [`HUD_BUDGET`] for a widget's draw
//! function) is an overrun, logged on the first and then every
//! [`LOG_EVERY`]th; [`MAX_CONSECUTIVE_OVERRUNS`] in a row disable the mod. The
//! counters are atomics so the F3.8 profiler can read them from another thread.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Longest an intercept call may take before it counts as an overrun.
pub const BUDGET: Duration = Duration::from_millis(1);
/// Longest a HUD widget's draw function may take before it counts as an overrun:
/// it runs on the render thread, once per frame per widget (F3.4).
pub const HUD_BUDGET: Duration = Duration::from_micros(500);
/// Overruns in a row after which the mod is disabled.
pub const MAX_CONSECUTIVE_OVERRUNS: u64 = 50;
/// A repeated condition is logged on its first occurrence and then this often.
pub const LOG_EVERY: u64 = 100;

/// What a measured call means for the mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Within the budget.
    Within,
    /// Over the budget; `log` says whether this overrun is one to report.
    Overrun {
        /// Whether to log (first overrun, then every [`LOG_EVERY`]th).
        log: bool,
    },
    /// Too many overruns in a row: disable the mod.
    Exhausted,
}

/// A snapshot of a mod's counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModStats {
    /// Intercept calls made.
    pub calls: u64,
    /// Time spent in them, in nanoseconds.
    pub total_ns: u64,
    /// The slowest one, in nanoseconds.
    pub max_ns: u64,
    /// Calls over the budget.
    pub overruns: u64,
    /// Observed events dropped because the mod's queue was full or closed, plus
    /// injected packets refused with `BUSY`.
    pub dropped: u64,
    /// Packets accepted by `packet_send`.
    pub injected: u64,
}

/// The live counters of one mod.
#[derive(Debug, Default)]
pub(crate) struct Stats {
    calls: AtomicU64,
    total_ns: AtomicU64,
    max_ns: AtomicU64,
    overruns: AtomicU64,
    consecutive_overruns: AtomicU64,
    dropped: AtomicU64,
    injected: AtomicU64,
}

/// Whether the `count`th occurrence of a repeated condition is worth a log line.
pub(crate) fn should_log(count: u64) -> bool {
    count == 1 || count.is_multiple_of(LOG_EVERY)
}

impl Stats {
    /// Records one call that took `elapsed` against `budget`.
    ///
    /// Each kind of hook (event handlers, widget draws, commands) keeps its own
    /// `Stats`, so a streak of overruns in one never counts against another.
    pub(crate) fn record_call(&self, elapsed: Duration, budget: Duration) -> Verdict {
        let ns = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.total_ns.fetch_add(ns, Ordering::Relaxed);
        self.max_ns.fetch_max(ns, Ordering::Relaxed);
        if elapsed <= budget {
            self.consecutive_overruns.store(0, Ordering::Relaxed);
            return Verdict::Within;
        }
        let overruns = self.overruns.fetch_add(1, Ordering::Relaxed) + 1;
        let consecutive = self.consecutive_overruns.fetch_add(1, Ordering::Relaxed) + 1;
        if consecutive >= MAX_CONSECUTIVE_OVERRUNS {
            Verdict::Exhausted
        } else {
            Verdict::Overrun {
                log: should_log(overruns),
            }
        }
    }

    /// Records a dropped observation; returns the new total.
    pub(crate) fn record_drop(&self) -> u64 {
        self.dropped.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Records a packet accepted by `packet_send`.
    pub(crate) fn record_injection(&self) {
        self.injected.fetch_add(1, Ordering::Relaxed);
    }

    /// A consistent-enough copy of the counters (each is read atomically).
    pub(crate) fn snapshot(&self) -> ModStats {
        ModStats {
            calls: self.calls.load(Ordering::Relaxed),
            total_ns: self.total_ns.load(Ordering::Relaxed),
            max_ns: self.max_ns.load(Ordering::Relaxed),
            overruns: self.overruns.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            injected: self.injected.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOW: Duration = Duration::from_millis(2);

    #[test]
    fn a_fast_call_is_within_and_resets_the_streak() {
        let stats = Stats::default();
        stats.record_call(SLOW, BUDGET);
        assert_eq!(stats.record_call(BUDGET, BUDGET), Verdict::Within);
        for _ in 0..MAX_CONSECUTIVE_OVERRUNS - 1 {
            assert!(matches!(
                stats.record_call(SLOW, BUDGET),
                Verdict::Overrun { .. }
            ));
        }
        // The streak restarted after the fast call, so 49 in a row is still fine.
        assert_eq!(stats.snapshot().overruns, MAX_CONSECUTIVE_OVERRUNS);
    }

    #[test]
    fn the_budget_is_a_parameter() {
        let stats = Stats::default();
        let call = Duration::from_micros(700);
        assert_eq!(stats.record_call(call, BUDGET), Verdict::Within);
        assert!(matches!(
            stats.record_call(call, HUD_BUDGET),
            Verdict::Overrun { .. }
        ));
    }

    #[test]
    fn fifty_consecutive_overruns_exhaust_the_budget() {
        let stats = Stats::default();
        for _ in 0..MAX_CONSECUTIVE_OVERRUNS - 1 {
            assert!(matches!(
                stats.record_call(SLOW, BUDGET),
                Verdict::Overrun { .. }
            ));
        }
        assert_eq!(stats.record_call(SLOW, BUDGET), Verdict::Exhausted);
    }

    #[test]
    fn overruns_are_logged_first_then_every_hundredth() {
        let stats = Stats::default();
        let mut logged = Vec::new();
        for n in 1..=250 {
            // A fast call between two slow ones keeps the streak from disabling.
            if let Verdict::Overrun { log: true } = stats.record_call(SLOW, BUDGET) {
                logged.push(n);
            }
            stats.record_call(Duration::ZERO, BUDGET);
        }
        assert_eq!(logged, [1, 100, 200]);
    }

    #[test]
    fn snapshot_reports_calls_time_max_and_drops() {
        let stats = Stats::default();
        stats.record_call(Duration::from_nanos(300), BUDGET);
        stats.record_call(Duration::from_nanos(900), BUDGET);
        assert_eq!(stats.record_drop(), 1);
        let snapshot = stats.snapshot();
        assert_eq!(
            snapshot,
            ModStats {
                calls: 2,
                total_ns: 1200,
                max_ns: 900,
                overruns: 0,
                dropped: 1,
                injected: 0,
            }
        );
    }
}
