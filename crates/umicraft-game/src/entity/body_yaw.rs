// SPDX-License-Identifier: MIT OR Apache-2.0

//! The body yaw (MCP `renderYawOffset`): where the torso points, which lags
//! behind the head and the facing direction.
//!
//! Two vanilla algorithms exist. Players and other `EntityLivingBase`
//! without AI turn the body towards the movement direction
//! ([`BodyYaw::update_player`], `EntityLivingBase.func_110146_f`); mobs
//! follow the head within a bound ([`BodyYaw::update_mob`],
//! `EntityBodyHelper.updateRenderAngles`).

use super::angle::wrap_degrees;

/// Squared horizontal distance above which a player counts as walking
/// (MCP `EntityLivingBase.onUpdate`).
const PLAYER_MOVING_SQUARED: f32 = 0.002_500_000_2;
/// Squared horizontal distance above which a mob counts as walking
/// (MCP `EntityBodyHelper`).
const MOB_MOVING_SQUARED: f64 = 2.500_000_277_905_201e-7;
/// Largest angle between head and body, in degrees.
const MAX_HEAD_TWIST: f32 = 75.0;
/// Ticks the head may stay still before the body starts to catch up.
const IDLE_TICKS_BEFORE_REALIGN: u32 = 10;

/// The torso orientation of a living entity, in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BodyYaw {
    /// Current body yaw (MCP `renderYawOffset`).
    pub offset: f32,
    /// Body yaw one tick ago (MCP `prevRenderYawOffset`).
    pub previous: f32,
    /// Ticks the mob's head has stayed within 15° of its reference.
    idle_ticks: u32,
    /// Head yaw the idle counter is measured against.
    head_reference: f32,
}

impl BodyYaw {
    /// A body facing `yaw`, as at spawn.
    #[must_use]
    pub fn new(yaw: f32) -> Self {
        Self {
            offset: yaw,
            previous: yaw,
            idle_ticks: 0,
            head_reference: yaw,
        }
    }

    /// Remembers the current yaw as the previous one; call at the start of a tick.
    pub fn begin_tick(&mut self) {
        self.previous = self.offset;
    }

    /// Player algorithm. `moved` is the horizontal displacement this tick
    /// `(dx, dz)`, `yaw` the facing direction; `swinging` snaps the body to
    /// the facing direction while an arm swings.
    pub fn update_player(&mut self, moved: (f64, f64), yaw: f32, swinging: bool) {
        let (dx, dz) = moved;
        let distance_squared = (dx * dx + dz * dz) as f32;
        let mut target = self.offset;
        if distance_squared > PLAYER_MOVING_SQUARED {
            target = (dz.atan2(dx) as f32).to_degrees() - 90.0;
        }
        if swinging {
            target = yaw;
        }
        self.offset += wrap_degrees(target - self.offset) * 0.3;
        let twist = wrap_degrees(yaw - self.offset).clamp(-MAX_HEAD_TWIST, MAX_HEAD_TWIST);
        self.offset = yaw - twist;
        if twist * twist > 2500.0 {
            self.offset += twist * 0.2;
        }
    }

    /// Mob algorithm. A walking mob faces `yaw` and drags its head within
    /// 75° of the body; an idle one turns the body towards the head, more
    /// and more tightly once the head has rested for 10 ticks.
    pub fn update_mob(&mut self, moved: (f64, f64), yaw: f32, head_yaw: &mut f32) {
        let (dx, dz) = moved;
        if dx * dx + dz * dz > MOB_MOVING_SQUARED {
            self.offset = yaw;
            *head_yaw = bounded_towards(self.offset, *head_yaw, MAX_HEAD_TWIST);
            self.head_reference = *head_yaw;
            self.idle_ticks = 0;
            return;
        }
        let mut bound = MAX_HEAD_TWIST;
        if (*head_yaw - self.head_reference).abs() > 15.0 {
            self.idle_ticks = 0;
            self.head_reference = *head_yaw;
        } else {
            self.idle_ticks += 1;
            if self.idle_ticks > IDLE_TICKS_BEFORE_REALIGN {
                let rested = (self.idle_ticks - IDLE_TICKS_BEFORE_REALIGN) as f32;
                bound = (1.0 - rested / 10.0).max(0.0) * MAX_HEAD_TWIST;
            }
        }
        self.offset = bounded_towards(*head_yaw, self.offset, bound);
    }
}

/// `angle2` moved so it is at most `bound` degrees from `angle1`
/// (MCP `EntityBodyHelper.computeAngleWithBound`).
fn bounded_towards(angle1: f32, angle2: f32, bound: f32) -> f32 {
    let mut difference = wrap_degrees(angle1 - angle2);
    if difference < -bound {
        difference = -bound;
    }
    if difference >= bound {
        difference = bound;
    }
    angle1 - difference
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_walking_player_turns_towards_the_movement_direction() {
        let mut body = BodyYaw::new(0.0);
        // Moving along +x is yaw -90 in Minecraft's convention.
        body.update_player((0.2, 0.0), 0.0, false);
        // 30 % of the way towards -90, within the 75 degree twist.
        assert!((body.offset + 27.0).abs() < 1e-3, "{}", body.offset);
    }

    #[test]
    fn a_standing_player_keeps_the_body_still() {
        let mut body = BodyYaw::new(40.0);
        body.update_player((0.0, 0.0), 40.0, false);
        assert!((body.offset - 40.0).abs() < 1e-4);
    }

    #[test]
    fn the_body_never_trails_the_facing_by_more_than_75_degrees() {
        let mut body = BodyYaw::new(0.0);
        for _ in 0..50 {
            body.update_player((0.0, 0.0), 170.0, false);
        }
        let twist = wrap_degrees(170.0 - body.offset);
        assert!(twist.abs() <= MAX_HEAD_TWIST + 1e-3, "{twist}");
    }

    #[test]
    fn a_swinging_player_faces_forwards() {
        let mut body = BodyYaw::new(0.0);
        body.update_player((0.2, 0.0), 30.0, true);
        // 30 % of the way to the facing direction, as for any other target.
        assert!((body.offset - 9.0).abs() < 1e-3, "{}", body.offset);
    }

    #[test]
    fn a_walking_mob_faces_its_heading_and_drags_the_head() {
        let mut body = BodyYaw::new(0.0);
        let mut head = 170.0;
        body.update_mob((0.1, 0.0), 20.0, &mut head);
        assert_eq!(body.offset, 20.0);
        assert!((head - 95.0).abs() < 1e-3, "head clamped to 75 degrees");
    }

    #[test]
    fn an_idle_mob_follows_a_head_beyond_75_degrees() {
        let mut body = BodyYaw::new(0.0);
        let mut head = 120.0;
        body.update_mob((0.0, 0.0), 0.0, &mut head);
        assert!((body.offset - 45.0).abs() < 1e-3, "{}", body.offset);
    }

    #[test]
    fn a_resting_head_pulls_the_body_in_line_after_20_ticks() {
        let mut body = BodyYaw::new(0.0);
        let mut head = 60.0;
        body.head_reference = 60.0;
        for _ in 0..21 {
            body.update_mob((0.0, 0.0), 0.0, &mut head);
        }
        assert!((body.offset - 60.0).abs() < 1e-3, "{}", body.offset);
    }
}
