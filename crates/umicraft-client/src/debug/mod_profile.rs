// SPDX-License-Identifier: MIT OR Apache-2.0

//! The per-mod profiler of the F3 screen (F3.8, spec §8.5).
//!
//! The host keeps cumulative counters per mod; once a second the profiler takes a
//! snapshot and turns the difference with the previous one into rates, so a line
//! says what the mod costs now, not since the start. Between two snapshots the
//! lines are reused as they are: the F3 screen redraws every frame and a line that
//! changes 60 times a second cannot be read anyway.

use std::time::{Duration, Instant};

use umicraft_modhost::{ModHost, ModProfile, ModState, ModStats};

/// How often the lines are recomputed.
const PERIOD: Duration = Duration::from_secs(1);

/// The F3 lines of the external mods and what they are computed from.
#[derive(Debug, Default)]
pub struct ModProfiler {
    last: Option<(Instant, Vec<ModProfile>)>,
    lines: Vec<String>,
}

impl ModProfiler {
    /// The lines to show at `now`, recomputed when a period has passed.
    pub fn lines(&mut self, host: &ModHost, now: Instant) -> &[String] {
        let due = self
            .last
            .as_ref()
            .is_none_or(|(at, _)| now.duration_since(*at) >= PERIOD);
        if due {
            let current = host.profile();
            let previous = self.last.take();
            self.lines = current
                .iter()
                .map(|profile| {
                    let before = previous.as_ref().and_then(|(at, old)| {
                        old.iter()
                            .find(|old| old.id == profile.id)
                            .map(|old| (now.duration_since(*at), old.total()))
                    });
                    line(profile, before)
                })
                .collect();
            self.last = Some((now, current));
        }
        &self.lines
    }
}

/// One mod's line: rates over `before` (the elapsed time and the counters of the
/// previous snapshot), or the totals for a mod seen for the first time.
fn line(profile: &ModProfile, before: Option<(Duration, ModStats)>) -> String {
    let now = profile.total();
    let (seconds, base) = before.map_or((None, ModStats::default()), |(elapsed, stats)| {
        (Some(elapsed.as_secs_f64().max(1e-3)), stats)
    });
    let calls = now.calls.saturating_sub(base.calls);
    let nanos = now.total_ns.saturating_sub(base.total_ns);
    let mean_us = if calls == 0 {
        0.0
    } else {
        nanos as f64 / calls as f64 / 1_000.0
    };
    let rate = seconds.map_or(calls as f64, |s| calls as f64 / s);
    let state = match &profile.state {
        ModState::Enabled => "",
        ModState::SwitchedOff => ", off",
        ModState::HeldByCompat => ", held",
        ModState::Failed(_) => ", failed",
    };
    format!(
        "mod {}: {rate:.0} calls/s, {mean_us:.1} us avg, {:.1} us max, {} over, {} dropped{state}",
        profile.id,
        now.max_ns as f64 / 1_000.0,
        now.overruns,
        now.dropped,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(calls: u64, total_ns: u64, state: ModState) -> ModProfile {
        let stats = ModStats {
            calls,
            total_ns,
            max_ns: 2_500,
            overruns: 1,
            ..ModStats::default()
        };
        ModProfile {
            id: "hello".to_owned().try_into().expect("a valid id"),
            name: "Hello".to_owned(),
            version: "0.1.0".to_owned(),
            state,
            events: stats,
            widgets: ModStats::default(),
            commands: ModStats::default(),
            physics: ModStats::default(),
            camera: ModStats::default(),
        }
    }

    #[test]
    fn a_line_shows_the_rate_since_the_last_snapshot() {
        let before = profile(100, 100_000, ModState::Enabled).total();
        let now = profile(140, 180_000, ModState::Enabled);
        let text = line(&now, Some((Duration::from_secs(2), before)));
        assert_eq!(
            text,
            "mod hello: 20 calls/s, 2.0 us avg, 2.5 us max, 1 over, 0 dropped"
        );
    }

    #[test]
    fn a_failed_mod_says_so() {
        let failed = profile(0, 0, ModState::Failed("panicked".to_owned()));
        assert!(line(&failed, None).ends_with(", failed"));
    }
}
