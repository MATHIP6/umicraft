// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the player sees of damage: the roll of the camera when hurt and the
//! flash of the hearts (MCP `EntityRenderer.hurtCameraEffect`,
//! `GuiIngame.renderPlayerStats`).

use std::f32::consts::PI;

/// Ticks of the hurt flash (`maxHurtTime`).
const MAX_HURT_TIME: f32 = 10.0;
/// Degrees of roll at the peak of the hurt shake.
const HURT_ROLL: f32 = 14.0;
/// Ticks the hearts keep flashing after a loss.
const LOSS_FLASH_TICKS: i64 = 20;
/// Ticks the hearts keep flashing after a gain.
const GAIN_FLASH_TICKS: i64 = 10;
/// Milliseconds after which the remembered health catches up
/// (`Minecraft.getSystemTime() - lastSystemTime > 1000`).
const CATCH_UP_MS: u64 = 1000;

/// The roll of the camera, in degrees, `alpha` of the way into the tick when
/// `hurt_time` ticks of the hurt flash are left.
///
/// `hurtCameraEffect` turns the view about `z` by `-sin((t / 10)^4 * pi) * 14`
/// with `t = hurtTime - alpha`; the two turns about `y` by `attackedAtYaw` cancel
/// because a hurt sent by the server leaves `attackedAtYaw` at 0.
#[must_use]
pub fn hurt_roll(hurt_time: u8, alpha: f32) -> f32 {
    let left = f32::from(hurt_time) - alpha;
    if left < 0.0 {
        return 0.0;
    }
    let t = left / MAX_HURT_TIME;
    -(t * t * t * t * PI).sin() * HURT_ROLL
}

/// How the hearts flash (the counters of `GuiIngame`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeartsFlash {
    /// `updateCounter`: ticks of the HUD.
    update_counter: i64,
    /// `healthUpdateCounter`: the tick at which the flash ends.
    flash_until: i64,
    /// `playerHealth`: the health at the last look, in half hearts.
    shown: i32,
    /// `lastPlayerHealth`: the health the white hearts show.
    previous: i32,
    /// `lastSystemTime`, in milliseconds.
    last_change_ms: u64,
}

/// What the hearts show at one look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartsLook {
    /// Whether the white flash is on this tick.
    pub flash: bool,
    /// The health the flash outlines, in half hearts.
    pub previous: i32,
    /// The tick of the HUD, which seeds the shake and moves the regeneration
    /// heart.
    pub tick: u32,
}

impl HeartsFlash {
    /// A HUD that has not looked at the health yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// One tick of the HUD (`GuiIngame.updateTick`).
    pub fn tick(&mut self) {
        self.update_counter += 1;
    }

    /// Looks at the health once, as `renderPlayerStats` does each frame.
    ///
    /// `resistant` is `hurtResistantTime > 0`; `now_ms` is a clock in
    /// milliseconds.
    pub fn look(&mut self, health: f32, resistant: bool, now_ms: u64) -> HeartsLook {
        let ceil = health.ceil() as i32;
        let left = self.flash_until - self.update_counter;
        let flash = left > 0 && left / 3 % 2 == 1;
        if resistant && ceil < self.shown {
            self.last_change_ms = now_ms;
            self.flash_until = self.update_counter + LOSS_FLASH_TICKS;
        } else if resistant && ceil > self.shown {
            self.last_change_ms = now_ms;
            self.flash_until = self.update_counter + GAIN_FLASH_TICKS;
        }
        if now_ms.saturating_sub(self.last_change_ms) > CATCH_UP_MS {
            self.previous = ceil;
            self.last_change_ms = now_ms;
        }
        self.shown = ceil;
        HeartsLook {
            flash,
            previous: self.previous,
            tick: self.update_counter as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_camera_rolls_hardest_at_the_start_of_the_hurt_and_settles() {
        // 10 ticks left, 0 elapsed: sin(pi) is 0, the shake builds up first.
        assert!(hurt_roll(10, 0.0).abs() < 1e-3);
        let early = hurt_roll(9, 0.0);
        let late = hurt_roll(3, 0.0);
        assert!(early < -1.0, "{early}");
        assert!(late.abs() < early.abs());
        // A healed player does not roll.
        assert_eq!(hurt_roll(0, 0.5), 0.0);
        // Always the same way, within 14 degrees.
        for time in 0..=10u8 {
            let roll = hurt_roll(time, 0.3);
            assert!((-14.0..=0.0).contains(&roll), "{time}: {roll}");
        }
    }

    #[test]
    fn a_loss_makes_the_hearts_blink_every_three_ticks_for_twenty() {
        let mut hearts = HeartsFlash::new();
        hearts.tick();
        assert!(!hearts.look(20.0, false, 0).flash);
        hearts.tick();
        // The player is hit: ten half hearts are lost with the resistance on.
        assert!(!hearts.look(10.0, true, 100).flash);
        let mut blinks = Vec::new();
        for _ in 0..22 {
            hearts.tick();
            blinks.push(hearts.look(10.0, true, 100).flash);
        }
        assert!(blinks.iter().any(|on| *on));
        let on = blinks.iter().filter(|on| **on).count();
        assert!((7..=10).contains(&on), "{on}");
        assert!(!blinks[20] && !blinks[21]);
    }

    #[test]
    fn the_outline_keeps_the_old_health_for_a_second() {
        let mut hearts = HeartsFlash::new();
        hearts.look(20.0, false, 5_000);
        hearts.tick();
        hearts.look(14.0, true, 5_100);
        let look = hearts.look(14.0, true, 5_200);
        assert_eq!(look.previous, 20);
        let look = hearts.look(14.0, false, 6_200);
        assert_eq!(look.previous, 14);
    }

    #[test]
    fn a_gain_flashes_for_half_as_long() {
        let mut hearts = HeartsFlash::new();
        hearts.look(10.0, false, 0);
        hearts.tick();
        hearts.look(12.0, true, 10);
        assert_eq!(hearts.flash_until - hearts.update_counter, 10);
    }
}
