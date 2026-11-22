// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client's copy of the world time of day.
//!
//! The reference client keeps its own clock between two `Time Update`
//! packets: `WorldClient.tick` adds one tick while the `doDaylightCycle` rule
//! is on, and `WorldClient.setWorldTime` reads a negative time as "the cycle
//! is frozen at this time" (the server negates the value, and sends `-1` for
//! a frozen time 0). Without the local tick the sky would only move once a
//! second, when a packet arrives, and render interpolation would have nothing
//! to interpolate.

/// The time of day as the client sees it (`WorldClient.getWorldTime`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::game) struct WorldClock {
    /// The time of day in ticks, never negative.
    time: i64,
    /// Whether the time advances each tick (`doDaylightCycle`).
    cycle: bool,
}

impl Default for WorldClock {
    /// A new world starts at time 0 with the cycle on, as a fresh
    /// `WorldClient` and its default game rules do.
    fn default() -> Self {
        Self {
            time: 0,
            cycle: true,
        }
    }
}

impl WorldClock {
    /// Applies the `time_of_day` of a `Time Update` (`setWorldTime`): a
    /// negative value freezes the cycle at its absolute value.
    pub(in crate::game) fn sync(&mut self, time_of_day: i64) {
        self.cycle = time_of_day >= 0;
        self.time = time_of_day.saturating_abs();
    }

    /// One client tick (`WorldClient.tick`).
    pub(in crate::game) fn tick(&mut self) {
        if self.cycle {
            self.time = self.time.saturating_add(1);
        }
    }

    /// The time of day in ticks, never negative.
    pub(in crate::game) fn time(self) -> i64 {
        self.time
    }

    /// Whether the server froze the day cycle.
    #[cfg(test)]
    pub(in crate::game) fn frozen(self) -> bool {
        !self.cycle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_advances_one_tick_at_a_time_between_updates() {
        // The bug: the time only changed on each `Time Update` (once a
        // second), so the sun and the custom sky moved in jumps.
        let mut clock = WorldClock::default();
        clock.sync(1_000);
        for _ in 0..20 {
            clock.tick();
        }
        assert_eq!(clock.time(), 1_020);
        clock.sync(1_019);
        assert_eq!(clock.time(), 1_019);
    }

    #[test]
    fn a_negative_time_freezes_the_cycle_at_its_absolute_value() {
        // The server sends `-6000` for a day frozen at noon: the sky must show
        // noon (6000), not 18000 (midnight) as a Euclidean modulo gives.
        let mut clock = WorldClock::default();
        clock.sync(-6_000);
        assert!(clock.frozen());
        clock.tick();
        assert_eq!(clock.time(), 6_000);
        // A frozen time 0 is sent as -1.
        clock.sync(-1);
        assert_eq!(clock.time(), 1);
        // A positive time turns the cycle back on.
        clock.sync(6_000);
        assert!(!clock.frozen());
        clock.tick();
        assert_eq!(clock.time(), 6_001);
    }

    #[test]
    fn the_extreme_time_does_not_overflow() {
        let mut clock = WorldClock::default();
        clock.sync(i64::MIN);
        assert_eq!(clock.time(), i64::MAX);
        clock.sync(i64::MAX);
        clock.tick();
        assert_eq!(clock.time(), i64::MAX);
    }
}
