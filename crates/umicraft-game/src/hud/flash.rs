// SPDX-License-Identifier: MIT OR Apache-2.0

//! The damage blink of the hearts and the hurt-resistance countdown it reads
//! (MCP `GuiIngame.renderPlayerStats`, `EntityPlayerSP.setPlayerSPHealth`,
//! `EntityLivingBase.handleStatusUpdate`).
//!
//! When the health changes while the player is still "hurt resistant", the
//! hearts flash between their normal and highlighted container for a short
//! while, and the old health is drawn as a ghost under the new one. The rule
//! uses the wall clock (a second without a change forgets the old health), so
//! the caller passes the time in milliseconds.

/// `EntityLivingBase.maxHurtResistantTime`.
const MAX_HURT_RESISTANT_TIME: i32 = 20;
/// How long the ghost of the old health stays, in milliseconds.
const FORGET_AFTER_MS: u64 = 1000;
/// Ticks the blink lasts after a loss (`updateCounter + 20`).
const LOSS_BLINK_TICKS: i64 = 20;
/// Ticks the blink lasts after a gain (`updateCounter + 10`).
const GAIN_BLINK_TICKS: i64 = 10;

/// `EntityLivingBase.hurtResistantTime` of the local player.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HurtResistance {
    time: i32,
    /// The health the last packet set; `None` before the first one
    /// (`EntityPlayerSP.hasValidHealth`).
    health: Option<f32>,
}

impl HurtResistance {
    /// A player that has not been told its health yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            time: 0,
            health: None,
        }
    }

    /// `EntityPlayerSP.setPlayerSPHealth`: a loss starts the full countdown, a
    /// gain half of it, the first packet nothing.
    pub fn on_health(&mut self, health: f32) {
        if let Some(current) = self.health {
            let lost = current - health;
            if lost > 0.0 {
                self.time = MAX_HURT_RESISTANT_TIME;
            } else if lost < 0.0 {
                self.time = MAX_HURT_RESISTANT_TIME / 2;
            }
        }
        self.health = Some(health);
    }

    /// `Entity Status` 2: the player was hurt.
    pub fn on_hurt(&mut self) {
        self.time = MAX_HURT_RESISTANT_TIME;
    }

    /// One game tick (`EntityLivingBase.onEntityUpdate`).
    pub fn tick(&mut self) {
        self.time = (self.time - 1).max(0);
    }

    /// Whether the countdown is running (`hurtResistantTime > 0`).
    #[must_use]
    pub fn active(&self) -> bool {
        self.time > 0
    }
}

/// What the hearts need for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashFrame {
    /// Whether the hearts show their highlighted container and the ghost.
    pub blink: bool,
    /// The health the ghost shows, in half hearts (`lastPlayerHealth`).
    pub last_health: i32,
}

/// The state `renderPlayerStats` keeps between frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HealthFlash {
    player_health: i32,
    last_player_health: i32,
    /// `healthUpdateCounter`: the tick the blink ends at.
    update_at: i64,
    last_time_ms: Option<u64>,
}

impl HealthFlash {
    /// A flash that has seen nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            player_health: 0,
            last_player_health: 0,
            update_at: 0,
            last_time_ms: None,
        }
    }

    /// Runs the per-frame rule with the ceiling of the health, whether the
    /// player is hurt resistant, the tick counter (`updateCounter`) and the
    /// clock in milliseconds.
    ///
    /// The blink is read before this frame's change is recorded, so it starts
    /// one frame after the damage, like the reference. Blink phases last
    /// three ticks: `(update_at - counter) / 3` odd.
    pub fn frame(
        &mut self,
        health: i32,
        hurt_resistant: bool,
        update_counter: u32,
        now_ms: u64,
    ) -> FlashFrame {
        let counter = i64::from(update_counter);
        let blink = self.update_at > counter && (self.update_at - counter) / 3 % 2 == 1;
        if hurt_resistant && health < self.player_health {
            self.last_time_ms = Some(now_ms);
            self.update_at = counter + LOSS_BLINK_TICKS;
        } else if hurt_resistant && health > self.player_health {
            self.last_time_ms = Some(now_ms);
            self.update_at = counter + GAIN_BLINK_TICKS;
        }
        let expired = self
            .last_time_ms
            .is_none_or(|at| now_ms.saturating_sub(at) > FORGET_AFTER_MS);
        if expired {
            self.last_player_health = health;
            self.last_time_ms = Some(now_ms);
        }
        self.player_health = health;
        FlashFrame {
            blink,
            last_health: self.last_player_health,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loss_starts_twenty_ticks_and_a_gain_ten() {
        let mut hurt = HurtResistance::new();
        hurt.on_health(20.0);
        assert!(!hurt.active(), "the first packet is not damage");
        hurt.on_health(14.0);
        assert_eq!(hurt.time, 20);
        hurt.time = 0;
        hurt.on_health(16.0);
        assert_eq!(hurt.time, 10);
    }

    #[test]
    fn the_countdown_stops_at_zero() {
        let mut hurt = HurtResistance::new();
        hurt.on_hurt();
        for _ in 0..25 {
            hurt.tick();
        }
        assert!(!hurt.active());
    }

    #[test]
    fn damage_blinks_from_the_next_frame_in_three_tick_phases() {
        let mut flash = HealthFlash::new();
        // Settle at 20 (the first frame adopts it: no clock yet).
        flash.frame(20, false, 100, 5_000);
        let hit = flash.frame(14, true, 101, 5_050);
        assert!(!hit.blink, "the blink starts after the frame that saw it");
        // update_at = 121: (121 - 102) / 3 = 6 (even), 118 -> 1 (odd).
        assert!(!flash.frame(14, true, 102, 5_100).blink);
        assert!(flash.frame(14, true, 118, 5_900).blink);
        assert!(!flash.frame(14, false, 121, 6_050).blink);
    }

    #[test]
    fn the_ghost_keeps_the_old_health_for_a_second() {
        let mut flash = HealthFlash::new();
        flash.frame(20, false, 100, 5_000);
        let hit = flash.frame(14, true, 101, 5_050);
        assert_eq!(hit.last_health, 20);
        assert_eq!(flash.frame(14, true, 110, 5_900).last_health, 20);
        assert_eq!(flash.frame(14, true, 121, 6_100).last_health, 14);
    }
}
