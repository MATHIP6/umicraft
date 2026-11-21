// SPDX-License-Identifier: MIT OR Apache-2.0

//! Dynamic Updates (ADR-0089, F6 phase 3): while the player neither moves nor
//! turns, the frame may upload more chunk geometry.
//!
//! OptiFine's option of the same name lets the chunk updater run harder while
//! the player stands still, because a still view is where a late chunk is most
//! visible and a longer frame costs nothing the player can feel (behaviour
//! only; no code of the reference is used).

use std::time::{Duration, Instant};

use glam::{DVec3, Vec2};

/// How long the pose must stay unchanged before the budget grows.
const STILL_AFTER: Duration = Duration::from_millis(500);

/// The factor by which a still player's upload budget grows.
const STILL_BOOST: usize = 2;

/// The upload budget of a frame, in bytes: `base`, doubled once the player has
/// been still for [`STILL_AFTER`] and `enabled` is set.
#[must_use]
pub(super) fn dynamic_budget(base: usize, enabled: bool, still_for: Duration) -> usize {
    if enabled && still_for >= STILL_AFTER {
        base.saturating_mul(STILL_BOOST)
    } else {
        base
    }
}

/// Remembers when the eye and the view direction last changed.
#[derive(Debug, Default)]
pub(super) struct Stillness {
    pose: Option<(DVec3, Vec2)>,
    since: Option<Instant>,
}

impl Stillness {
    /// Records the pose of the frame at `now`.
    pub(super) fn observe(&mut self, eye: DVec3, yaw_pitch: Vec2, now: Instant) {
        let pose = (eye, yaw_pitch);
        if self.pose != Some(pose) {
            self.pose = Some(pose);
            self.since = Some(now);
        }
    }

    /// How long the pose has been unchanged as of `now`; zero before the first
    /// observation.
    pub(super) fn still_for(&self, now: Instant) -> Duration {
        self.since
            .map_or(Duration::ZERO, |since| now.saturating_duration_since(since))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: usize = 1 << 20;

    #[test]
    fn a_moving_player_keeps_the_base_budget() {
        assert_eq!(dynamic_budget(BASE, true, Duration::from_millis(100)), BASE);
    }

    #[test]
    fn a_still_player_doubles_the_budget() {
        assert_eq!(dynamic_budget(BASE, true, STILL_AFTER), 2 * BASE);
    }

    #[test]
    fn the_option_off_never_boosts() {
        assert_eq!(dynamic_budget(BASE, false, Duration::from_secs(10)), BASE);
    }

    #[test]
    fn turning_or_moving_restarts_the_clock() {
        let t0 = Instant::now();
        let mut still = Stillness::default();
        assert_eq!(still.still_for(t0), Duration::ZERO);
        still.observe(DVec3::ZERO, Vec2::ZERO, t0);
        let t1 = t0 + Duration::from_secs(1);
        still.observe(DVec3::ZERO, Vec2::ZERO, t1);
        assert_eq!(still.still_for(t1), Duration::from_secs(1));
        still.observe(DVec3::ZERO, Vec2::new(0.1, 0.0), t1);
        assert_eq!(still.still_for(t1), Duration::ZERO);
        still.observe(DVec3::X, Vec2::new(0.1, 0.0), t1 + STILL_AFTER);
        assert_eq!(still.still_for(t1 + STILL_AFTER), Duration::ZERO);
    }
}
