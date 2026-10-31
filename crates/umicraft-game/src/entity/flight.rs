// SPDX-License-Identifier: MIT OR Apache-2.0

//! The flight history of the ender dragon (MCP `EntityDragon`).
//!
//! The dragon bends its neck, body and tail along the path it flew: the model
//! reads the yaw and height the entity had 0 to 23 ticks ago
//! (`getMovementOffsets`). The client keeps that history, and the phase of the
//! wing beat (`animTime`), itself.

use glam::Vec3;

use super::angle::wrap_degrees;

/// Entries of the history (`EntityDragon.ringBuffer`).
const RING: usize = 64;
/// How many past offsets a pose carries: the model reads 0 to 10 and 11 to 23.
pub const OFFSETS: usize = 24;

/// The yaw and height of the dragon over the last 64 ticks and its wing phase.
#[derive(Debug, Clone, PartialEq)]
pub struct DragonFlight {
    ring: [[f64; 2]; RING],
    /// Index of the newest entry; `-1` before the first tick.
    index: i32,
    /// Phase of the wing beat, in turns.
    pub anim_time: f32,
    /// The phase one tick ago.
    pub prev_anim_time: f32,
    /// Ticks since the dragon died (`EntityDragon.deathTicks`); the server
    /// removes it at 200.
    pub death_ticks: u32,
}

/// What the model reads from the flight at one sub-tick fraction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragonPose {
    /// Phase of the wing beat, in turns.
    pub anim_time: f32,
    /// Ticks since the dragon died plus the fraction, `0` while it lives.
    pub death_ticks: f32,
    /// `[yaw, height]` the dragon had `i` ticks ago, interpolated, for `i` in
    /// `0..OFFSETS` (`getMovementOffsets(i)`).
    pub offsets: [[f64; 2]; OFFSETS],
}

impl Default for DragonFlight {
    fn default() -> Self {
        Self {
            ring: [[0.0; 2]; RING],
            index: -1,
            anim_time: 0.0,
            prev_anim_time: 0.0,
            death_ticks: 0,
        }
    }
}

impl DragonFlight {
    /// Advances one tick (`EntityDragon.onLivingUpdate`, the part the client
    /// runs): the wing phase moves by `0.2 / (speed * 10 + 1) * 2^vy`, and the
    /// yaw and height are recorded. A dead dragon stops both.
    pub fn tick(&mut self, yaw: f32, y: f64, velocity: Vec3, alive: bool) {
        self.prev_anim_time = self.anim_time;
        if !alive {
            self.death_ticks = self.death_ticks.saturating_add(1);
            return;
        }
        let speed = velocity.x.hypot(velocity.z);
        self.anim_time += 0.2 / (speed * 10.0 + 1.0) * 2.0_f32.powf(velocity.y);
        let yaw = f64::from(wrap_degrees(yaw));
        if self.index < 0 {
            self.ring = [[yaw, y]; RING];
        }
        self.index = (self.index + 1) % RING as i32;
        self.ring[self.index as usize] = [yaw, y];
    }

    /// `getMovementOffsets(back, alpha)`: the yaw and height `back` ticks ago.
    #[must_use]
    pub fn offsets(&self, back: usize, alpha: f32, alive: bool) -> [f64; 2] {
        if self.index < 0 {
            return [0.0, 0.0];
        }
        let partial = 1.0 - f64::from(if alive { alpha } else { 0.0 });
        let at = |behind: usize| (self.index - behind as i32).rem_euclid(RING as i32) as usize;
        let (now, before) = (self.ring[at(back)], self.ring[at(back + 1)]);
        let turn = f64::from(wrap_degrees((before[0] - now[0]) as f32));
        [
            now[0] + turn * partial,
            now[1] + (before[1] - now[1]) * partial,
        ]
    }

    /// The pose at the sub-tick fraction `alpha`.
    #[must_use]
    pub fn pose(&self, alpha: f32, alive: bool) -> DragonPose {
        DragonPose {
            anim_time: self.prev_anim_time + (self.anim_time - self.prev_anim_time) * alpha,
            death_ticks: if self.death_ticks > 0 {
                self.death_ticks as f32 + alpha
            } else {
                0.0
            },
            offsets: std::array::from_fn(|back| self.offsets(back, alpha, alive)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flown(yaws: &[f32]) -> DragonFlight {
        let mut flight = DragonFlight::default();
        for (tick, yaw) in yaws.iter().enumerate() {
            flight.tick(*yaw, 70.0 + tick as f64, Vec3::ZERO, true);
        }
        flight
    }

    #[test]
    fn the_wings_beat_a_fifth_of_a_turn_per_tick_when_still() {
        let flight = flown(&[0.0; 5]);
        assert!((flight.anim_time - 1.0).abs() < 1e-5);
    }

    #[test]
    fn a_fast_dragon_beats_slower_and_a_climbing_one_faster() {
        let mut fast = DragonFlight::default();
        fast.tick(0.0, 0.0, Vec3::new(1.0, 0.0, 0.0), true);
        assert!((fast.anim_time - 0.2 / 11.0).abs() < 1e-6);
        let mut climbing = DragonFlight::default();
        climbing.tick(0.0, 0.0, Vec3::new(0.0, 1.0, 0.0), true);
        assert!((climbing.anim_time - 0.4).abs() < 1e-6);
    }

    #[test]
    fn the_history_gives_the_yaw_and_height_of_past_ticks() {
        let flight = flown(&[10.0, 20.0, 30.0, 40.0]);
        // The newest tick, then one and three ticks back.
        assert_eq!(flight.offsets(0, 1.0, true), [40.0, 73.0]);
        assert_eq!(flight.offsets(1, 1.0, true), [30.0, 72.0]);
        assert_eq!(flight.offsets(3, 1.0, true), [10.0, 70.0]);
        // Half way between this tick and the one before.
        let mid = flight.offsets(0, 0.5, true);
        assert!((mid[0] - 35.0).abs() < 1e-9 && (mid[1] - 72.5).abs() < 1e-9);
    }

    #[test]
    fn before_it_has_flown_the_history_is_the_first_position() {
        let flight = flown(&[90.0]);
        assert_eq!(flight.offsets(23, 1.0, true), [90.0, 70.0]);
    }

    #[test]
    fn the_yaw_interpolates_the_short_way_round() {
        let flight = flown(&[170.0, -170.0]);
        let mid = flight.offsets(0, 0.5, true);
        assert!((mid[0].abs() - 180.0).abs() < 1e-6, "{mid:?}");
    }

    #[test]
    fn a_dead_dragon_freezes_and_shows_one_tick_back() {
        let mut flight = flown(&[10.0, 20.0]);
        let beat = flight.anim_time;
        flight.tick(99.0, 0.0, Vec3::ZERO, false);
        assert_eq!(flight.anim_time, beat);
        assert_eq!(flight.death_ticks, 1);
        assert!((flight.pose(0.5, false).death_ticks - 1.5).abs() < 1e-6);
        // `getMovementOffsets` takes a dead dragon's fraction as 0: a whole
        // tick before the newest entry, whatever the fraction asked for.
        assert_eq!(flight.offsets(0, 0.3, false), [10.0, 70.0]);
    }
}
