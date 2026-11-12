// SPDX-License-Identifier: MIT OR Apache-2.0

//! Counts quick successive clicks on a text field: one places the caret, two
//! select a word, three select the whole line.
//!
//! Vanilla `GuiTextField` has neither; this is a Umicraft addition. The count
//! takes the time as a parameter so it is testable without a clock.

use std::time::{Duration, Instant};

/// Longest gap between two clicks that still count as one series.
const SERIES_GAP: Duration = Duration::from_millis(500);
/// Clicks after which the series starts over.
const MAX_CLICKS: u8 = 3;

/// The last click of a field.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ClickSeries {
    last: Option<(Instant, usize, u8)>,
}

impl ClickSeries {
    /// Registers a click at `now` on the character boundary `caret` and
    /// returns its rank in the series (`1..=3`). A click elsewhere in the
    /// field starts a new series.
    pub fn register(&mut self, now: Instant, caret: usize) -> u8 {
        let rank = match self.last {
            Some((at, last_caret, rank))
                if last_caret == caret && now.saturating_duration_since(at) <= SERIES_GAP =>
            {
                if rank >= MAX_CLICKS {
                    1
                } else {
                    rank + 1
                }
            }
            _ => 1,
        };
        self.last = Some((now, caret, rank));
        rank
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_clicks_count_up_then_start_over() {
        let start = Instant::now();
        let mut series = ClickSeries::default();
        let ranks: Vec<u8> = (0..4)
            .map(|n| series.register(start + Duration::from_millis(100 * n), 4))
            .collect();
        assert_eq!(ranks, [1, 2, 3, 1]);
    }

    #[test]
    fn a_slow_click_starts_a_new_series() {
        let start = Instant::now();
        let mut series = ClickSeries::default();
        series.register(start, 0);
        assert_eq!(series.register(start + Duration::from_millis(900), 0), 1);
    }

    #[test]
    fn a_click_on_another_character_starts_a_new_series() {
        let start = Instant::now();
        let mut series = ClickSeries::default();
        series.register(start, 2);
        assert_eq!(series.register(start + Duration::from_millis(100), 3), 1);
    }
}
