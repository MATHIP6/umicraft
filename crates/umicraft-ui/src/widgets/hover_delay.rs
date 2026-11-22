// SPDX-License-Identifier: MIT OR Apache-2.0

//! A hover that must rest before it counts, for option tooltips.
//!
//! OptiFine shows the tooltip of a video option once the pointer has rested
//! on it for 700 ms, and a move of more than 5 GUI pixels starts the wait
//! again (`TooltipManager`, ADR-0088). [`HoverDelay`] keeps that state. The
//! caller gives it the hovered target, the pointer in GUI units and the time,
//! so it reads no clock and is tested with plain instants.

use std::time::{Duration, Instant};

/// How long the pointer must rest on a target.
pub const HOVER_DELAY: Duration = Duration::from_millis(700);
/// How far the pointer may move, in GUI units, without restarting the wait.
pub const HOVER_SLOP: f32 = 5.0;

/// The resting state of the pointer over targets of type `K`.
#[derive(Debug, Clone)]
pub struct HoverDelay<K> {
    rest: Option<Rest<K>>,
}

#[derive(Debug, Clone)]
struct Rest<K> {
    target: K,
    since: Instant,
    anchor: [f32; 2],
}

impl<K> Default for HoverDelay<K> {
    fn default() -> Self {
        Self { rest: None }
    }
}

impl<K: PartialEq> HoverDelay<K> {
    /// Updates the state with this frame's hovered `target` (`None` over
    /// nothing) and `pointer` in GUI units, at `now`. Returns the target once
    /// the pointer has rested on it for [`HOVER_DELAY`].
    pub fn update(&mut self, target: Option<K>, pointer: [f32; 2], now: Instant) -> Option<&K> {
        let Some(target) = target else {
            self.rest = None;
            return None;
        };
        let restarted = self.rest.as_ref().is_none_or(|rest| {
            rest.target != target || distance(rest.anchor, pointer) > HOVER_SLOP
        });
        if restarted {
            self.rest = Some(Rest {
                target,
                since: now,
                anchor: pointer,
            });
        }
        self.rest
            .as_ref()
            .filter(|rest| now.saturating_duration_since(rest.since) >= HOVER_DELAY)
            .map(|rest| &rest.target)
    }
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_counts_after_the_delay() {
        let start = Instant::now();
        let mut hover = HoverDelay::default();
        assert_eq!(hover.update(Some(1), [0.0, 0.0], start), None);
        assert_eq!(
            hover.update(Some(1), [1.0, 1.0], start + Duration::from_millis(699)),
            None
        );
        assert_eq!(
            hover.update(Some(1), [1.0, 1.0], start + HOVER_DELAY),
            Some(&1)
        );
    }

    #[test]
    fn a_move_beyond_the_slop_or_another_target_restarts() {
        let start = Instant::now();
        let mut hover = HoverDelay::default();
        hover.update(Some(1), [0.0, 0.0], start);
        let late = start + HOVER_DELAY;
        assert_eq!(
            hover.update(Some(1), [6.0, 0.0], late),
            None,
            "moved too far"
        );
        assert_eq!(
            hover.update(Some(2), [6.0, 0.0], late + HOVER_DELAY / 2),
            None
        );
        assert_eq!(
            hover.update(Some(2), [6.0, 0.0], late + HOVER_DELAY * 2),
            Some(&2)
        );
    }

    #[test]
    fn leaving_every_target_clears_the_wait() {
        let start = Instant::now();
        let mut hover = HoverDelay::default();
        hover.update(Some(1), [0.0, 0.0], start);
        assert_eq!(hover.update(None, [0.0, 0.0], start + HOVER_DELAY), None);
        assert_eq!(hover.update(Some(1), [0.0, 0.0], start + HOVER_DELAY), None);
    }
}
