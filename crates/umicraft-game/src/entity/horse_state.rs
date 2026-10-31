// SPDX-License-Identifier: MIT OR Apache-2.0

//! The animation state a horse keeps by itself (MCP `EntityHorse.onLivingUpdate`).
//!
//! The server sends only the flags (eating, rearing, mouth open); how far the
//! head is lowered, the body is raised and the mouth is open follows from them
//! over a few ticks, and the tail wags now and then on its own.

/// How far each pose has progressed, and the wag of the tail.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HorseState {
    head_lean: f32,
    prev_head_lean: f32,
    rearing: f32,
    prev_rearing: f32,
    mouth: f32,
    prev_mouth: f32,
    /// Ticks of the wag of the tail, `0` when it hangs still
    /// (`field_110278_bp`).
    tail_swing: u8,
    /// Where the next random number comes from (the reference rolls
    /// `rand.nextInt(200)` each tick to start a wag).
    seed: u64,
}

impl Default for HorseState {
    fn default() -> Self {
        Self::new(0)
    }
}

/// The pose of a horse at one sub-tick fraction.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HorsePose {
    /// How far the head is down grazing, `0..=1`.
    pub grazing: f32,
    /// How far the horse stands on its hind legs, `0..=1`.
    pub rearing: f32,
    /// How far the mouth is open, `0..=1`.
    pub mouth: f32,
    /// Whether the tail wags.
    pub tail_swinging: bool,
}

impl HorseState {
    /// A horse at rest; `seed` makes the wags of two horses differ.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            head_lean: 0.0,
            prev_head_lean: 0.0,
            rearing: 0.0,
            prev_rearing: 0.0,
            mouth: 0.0,
            prev_mouth: 0.0,
            tail_swing: 0,
            seed: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        }
    }

    /// `rand.nextInt(bound)` of a small xorshift generator.
    fn next_below(&mut self, bound: u64) -> u64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 11) % bound
    }

    /// One tick, with the flags the server last sent.
    pub fn tick(&mut self, eating: bool, rearing: bool, mouth_open: bool) {
        if self.next_below(200) == 0 {
            self.tail_swing = 1;
        }
        if self.tail_swing > 0 {
            self.tail_swing += 1;
            if self.tail_swing > 8 {
                self.tail_swing = 0;
            }
        }

        self.prev_head_lean = self.head_lean;
        if eating {
            self.head_lean = (self.head_lean + (1.0 - self.head_lean) * 0.4 + 0.05).min(1.0);
        } else {
            self.head_lean = (self.head_lean + (0.0 - self.head_lean) * 0.4 - 0.05).max(0.0);
        }

        self.prev_rearing = self.rearing;
        if rearing {
            self.prev_head_lean = 0.0;
            self.head_lean = 0.0;
            self.rearing = (self.rearing + (1.0 - self.rearing) * 0.4 + 0.05).min(1.0);
        } else {
            let cube = self.rearing * self.rearing * self.rearing;
            self.rearing = (self.rearing + (0.8 * cube - self.rearing) * 0.6 - 0.05).max(0.0);
        }

        self.prev_mouth = self.mouth;
        if mouth_open {
            self.mouth = (self.mouth + (1.0 - self.mouth) * 0.7 + 0.05).min(1.0);
        } else {
            self.mouth = (self.mouth + (0.0 - self.mouth) * 0.7 - 0.05).max(0.0);
        }
    }

    /// How far the horse rears at the previous tick, which seats the rider
    /// (`EntityHorse.updateRiderPosition`).
    #[must_use]
    pub const fn previous_rearing(&self) -> f32 {
        self.prev_rearing
    }

    /// The pose at the sub-tick fraction `alpha`.
    #[must_use]
    pub fn pose(&self, alpha: f32) -> HorsePose {
        let lerp = |prev: f32, now: f32| prev + (now - prev) * alpha;
        HorsePose {
            grazing: lerp(self.prev_head_lean, self.head_lean),
            rearing: lerp(self.prev_rearing, self.rearing),
            mouth: lerp(self.prev_mouth, self.mouth),
            tail_swinging: self.tail_swing != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_horse_that_rears_rises_to_full_height_in_a_few_ticks_and_comes_down() {
        let mut state = HorseState::default();
        for _ in 0..8 {
            state.tick(false, true, false);
        }
        assert_eq!(state.pose(1.0).rearing, 1.0);
        for _ in 0..12 {
            state.tick(false, false, false);
        }
        assert_eq!(state.pose(1.0).rearing, 0.0);
    }

    #[test]
    fn rearing_lifts_the_head_from_the_grass() {
        let mut state = HorseState::default();
        for _ in 0..10 {
            state.tick(true, false, false);
        }
        assert_eq!(state.pose(1.0).grazing, 1.0);
        state.tick(true, true, false);
        assert_eq!(state.pose(1.0).grazing, 0.0);
    }

    #[test]
    fn the_mouth_opens_and_shuts_quickly() {
        let mut state = HorseState::default();
        state.tick(false, false, true);
        state.tick(false, false, true);
        assert!(state.pose(1.0).mouth > 0.9);
        state.tick(false, false, false);
        state.tick(false, false, false);
        state.tick(false, false, false);
        assert_eq!(state.pose(1.0).mouth, 0.0);
    }

    #[test]
    fn the_tail_wags_for_eight_ticks_now_and_then() {
        let mut state = HorseState::new(7);
        let mut wagging = 0;
        let mut longest = 0;
        let mut run = 0;
        for _ in 0..4000 {
            state.tick(false, false, false);
            if state.pose(1.0).tail_swinging {
                wagging += 1;
                run += 1;
                longest = longest.max(run);
            } else {
                run = 0;
            }
        }
        // One start in 200 ticks on average, each lasting 7 ticks.
        assert!((50..400).contains(&wagging), "{wagging}");
        assert_eq!(longest, 7);
    }
}
