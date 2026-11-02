// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Local Difficulty:` of the F3 screen (MCP `DifficultyInstance`).
//!
//! The client never learns the inhabited time of a chunk (it is not in the
//! chunk packet), so it is zero here, exactly as in the reference client
//! world.

/// `WorldProvider.moonPhaseFactors`.
const MOON_PHASE_FACTORS: [f32; 8] = [1.0, 0.75, 0.5, 0.25, 0.0, 0.25, 0.5, 0.75];

/// `DifficultyInstance.getAdditionalDifficulty` for `difficulty` (0 peaceful
/// to 3 hard) at `world_time` ticks, with no inhabited time.
///
/// `World.getDifficultyForLocation` reads the moon phase of the time of day
/// (`WorldProvider.getMoonPhase`).
#[must_use]
pub fn local_difficulty(difficulty: u8, world_time: i64) -> f32 {
    if difficulty == 0 {
        return 0.0;
    }
    let phase = (world_time / 24_000 % 8 + 8) % 8;
    let moon = MOON_PHASE_FACTORS[phase as usize];
    let time_part = ((world_time as f32 - 72_000.0) / 1_440_000.0).clamp(0.0, 1.0) * 0.25;
    let mut extra = (moon * 0.25).clamp(0.0, time_part);
    if difficulty == 1 {
        extra *= 0.5;
    }
    f32::from(difficulty) * (0.75 + time_part + extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaceful_has_no_additional_difficulty() {
        assert_eq!(local_difficulty(0, 1_000_000), 0.0);
    }

    #[test]
    fn a_new_world_starts_at_three_quarters_per_level() {
        assert!((local_difficulty(1, 0) - 0.75).abs() < 1e-6);
        assert!((local_difficulty(3, 0) - 2.25).abs() < 1e-6);
    }

    #[test]
    fn a_long_played_world_on_hard_with_a_full_moon_reaches_the_cap() {
        // Day 200 is moon phase 0: the time part is capped at 0.25 and the
        // full moon adds the same again: 3 * (0.75 + 0.25 + 0.25).
        let time = 200 * 24_000;
        assert!((local_difficulty(3, time) - 3.75).abs() < 1e-3);
    }

    #[test]
    fn easy_halves_the_moon_part() {
        // Time part 0.25 (capped), moon part 0.25 halved.
        assert!((local_difficulty(1, 200 * 24_000) - (0.75 + 0.25 + 0.125)).abs() < 1e-3);
    }
}
