// SPDX-License-Identifier: MIT OR Apache-2.0

//! What replaces the food row and the experience bar on a mount (MCP
//! `GuiIngame.renderPlayerStats` "mountHealth", `renderHorseJumpBar`,
//! `EntityPlayerSP.onLivingUpdate`).

use super::vitals::Fill;

/// Hearts shown for the mount at most (`k8 > 30`).
const MAX_MOUNT_HEARTS: i32 = 30;
/// Hearts per row of the mount's health.
const HEARTS_PER_ROW: i32 = 10;
/// Ticks of charging after which the power stops rising linearly.
const LINEAR_TICKS: i32 = 10;
/// Ticks the bar stays empty after a jump was sent.
const COOLDOWN_TICKS: i32 = -10;

/// What the player sits on, as far as the status rows care.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Riding {
    /// On foot: the food row is shown.
    OnFoot,
    /// In a boat, a minecart or another entity without health: no food row
    /// and no mount hearts (`entity instanceof EntityLivingBase` fails).
    Vehicle,
    /// On a living entity: its health replaces the food row.
    Mob {
        /// `getHealth`.
        health: f32,
        /// `getMaxHealth`.
        max_health: f32,
    },
}

/// One heart of the mount's health row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountHeartView {
    /// Position from the right edge, `0..10`.
    pub col: u8,
    /// Pixels above the food row's baseline, a multiple of 10.
    pub dy: i32,
    /// How much of the heart is filled.
    pub fill: Fill,
}

/// The hearts of a mount with `health` of `max_health`.
///
/// `(int)(max + 0.5) / 2` hearts, capped at 30, ten per row, the rows rising
/// by ten pixels. The mount hearts have no blink (`flag1` is never set in 1.8).
#[must_use]
pub fn mount_hearts(health: f32, max_health: f32) -> Vec<MountHeartView> {
    let health = health.ceil() as i32;
    let mut remaining = (((max_health + 0.5) as i32) / 2).clamp(0, MAX_MOUNT_HEARTS);
    let mut hearts = Vec::with_capacity(remaining as usize);
    let mut offset = 0;
    let mut dy = 0;
    while remaining > 0 {
        let in_row = remaining.min(HEARTS_PER_ROW);
        remaining -= in_row;
        for col in 0..in_row {
            let point = col * 2 + 1 + offset;
            hearts.push(MountHeartView {
                col: col as u8,
                dy,
                fill: fill_of(point, health),
            });
        }
        offset += 2 * HEARTS_PER_ROW;
        dy -= 10;
    }
    hearts
}

fn fill_of(point: i32, health: i32) -> Fill {
    match point.cmp(&health) {
        std::cmp::Ordering::Less => Fill::Full,
        std::cmp::Ordering::Equal => Fill::Half,
        std::cmp::Ordering::Greater => Fill::Empty,
    }
}

/// The charge of the jump bar of a ridden horse
/// (`EntityPlayerSP.horseJumpPower`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HorseJump {
    counter: i32,
    power: f32,
}

impl HorseJump {
    /// An empty bar.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            counter: 0,
            power: 0.0,
        }
    }

    /// The charge, `0.0..=1.0` and a little above 0.8 at most.
    #[must_use]
    pub fn power(&self) -> f32 {
        self.power
    }

    /// One tick: `was_jumping` is the jump key of the previous tick,
    /// `jumping` the key now. Returns the strength to send with the
    /// `Riding Jump` action (`(int)(power * 100)`) on the tick the key is
    /// released.
    ///
    /// The power rises by 0.1 per tick for ten ticks, then creeps up from 0.8
    /// as `0.8 + 0.2 / (counter - 9)`.
    pub fn tick(&mut self, riding_horse: bool, was_jumping: bool, jumping: bool) -> Option<u8> {
        if !riding_horse {
            self.power = 0.0;
            return None;
        }
        if self.counter < 0 {
            self.counter += 1;
            if self.counter == 0 {
                self.power = 0.0;
            }
        }
        if was_jumping && !jumping {
            self.counter = COOLDOWN_TICKS;
            return Some((self.power * 100.0) as u8);
        }
        if !was_jumping && jumping {
            self.counter = 0;
            self.power = 0.0;
        } else if was_jumping {
            self.counter += 1;
            self.power = if self.counter < LINEAR_TICKS {
                self.counter as f32 * 0.1
            } else {
                0.8 + 2.0 / (self.counter - 9) as f32 * 0.1
            };
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_horse_with_thirty_health_has_fifteen_hearts_in_two_rows() {
        let hearts = mount_hearts(30.0, 30.0);
        assert_eq!(hearts.len(), 15);
        assert_eq!(hearts[0].dy, 0);
        assert_eq!(hearts[10].dy, -10);
        assert_eq!(hearts[10].col, 0);
        assert!(hearts.iter().all(|h| h.fill == Fill::Full));
    }

    #[test]
    fn mount_hearts_are_capped_at_thirty() {
        assert_eq!(mount_hearts(100.0, 200.0).len(), 30);
    }

    #[test]
    fn the_second_row_continues_the_health_count() {
        // 25 health: row one is full (20), the second row's first two hearts
        // are full and the third half (point 25 == health).
        let hearts = mount_hearts(25.0, 30.0);
        assert_eq!(hearts[11].fill, Fill::Full);
        assert_eq!(hearts[12].fill, Fill::Half);
        assert_eq!(hearts[13].fill, Fill::Empty);
    }

    #[test]
    fn the_jump_power_rises_a_tenth_per_tick() {
        let mut jump = HorseJump::new();
        assert_eq!(jump.tick(true, false, true), None);
        for tick in 1..10 {
            jump.tick(true, true, true);
            assert!((jump.power() - tick as f32 * 0.1).abs() < 1e-6);
        }
        jump.tick(true, true, true);
        assert!((jump.power() - 0.8 - 0.2).abs() < 1e-6, "counter 10 -> 1.0");
        jump.tick(true, true, true);
        assert!((jump.power() - 0.9).abs() < 1e-6, "counter 11 -> 0.9");
    }

    #[test]
    fn releasing_the_key_sends_the_strength_and_empties_the_bar_later() {
        let mut jump = HorseJump::new();
        jump.tick(true, false, true);
        for _ in 0..5 {
            jump.tick(true, true, true);
        }
        assert_eq!(jump.tick(true, true, false), Some(50));
        // The bar stays until the cooldown of ten ticks has run out.
        for _ in 0..9 {
            jump.tick(true, false, false);
            assert!(jump.power() > 0.0);
        }
        jump.tick(true, false, false);
        assert_eq!(jump.power(), 0.0);
    }

    #[test]
    fn dismounting_clears_the_bar() {
        let mut jump = HorseJump::new();
        jump.tick(true, false, true);
        jump.tick(true, true, true);
        jump.tick(false, true, true);
        assert_eq!(jump.power(), 0.0);
    }
}
