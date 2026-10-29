// SPDX-License-Identifier: MIT OR Apache-2.0

//! Timed entity animations: the hurt flash, the death fall, the arm swing and
//! sleeping in a bed.

use glam::Vec3;

/// Ticks the hurt flash lasts (MCP `EntityLivingBase.handleStatusUpdate`,
/// `hurtTime = maxHurtTime = 10`).
pub const HURT_TICKS: u8 = 10;
/// Ticks of the death fall before the entity disappears
/// (MCP `EntityLivingBase.onDeathUpdate`).
pub const DEATH_TICKS: u8 = 20;
/// Ticks of an arm swing (MCP `getArmSwingAnimationEnd`, without potions).
const SWING_TICKS: i8 = 6;

/// The animation state of one entity.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Animation {
    hurt_time: u8,
    dying: bool,
    death_time: u8,
    swing: ArmSwing,
    fuse: Fuse,
    /// The bed the entity sleeps in, if any.
    pub bed: Option<BedPose>,
}

/// Ticks a creeper fuse takes (MCP `EntityCreeper.fuseTime`).
const FUSE_TICKS: i8 = 30;

/// The swelling of a creeper (MCP `timeSinceIgnited`, `lastActiveTime`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Fuse {
    ticks: i8,
    previous_ticks: i8,
}

/// The progress of an arm swing (MCP `swingProgressInt`, `swingProgress`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct ArmSwing {
    in_progress: bool,
    ticks: i8,
    progress: f32,
    previous_progress: f32,
}

/// How a sleeping player lies (MCP `EntityPlayer.trySleep`,
/// `getBedOrientationInDegrees`, `func_175139_a`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BedPose {
    /// Turn of the body about the vertical axis, in degrees.
    pub orientation: f32,
    /// Offset that moves the body to the foot end of the bed
    /// (MCP `renderOffsetX/Z`).
    pub render_offset: Vec3,
    /// Where the head rests, relative to the bed block.
    pub head_offset: Vec3,
}

impl BedPose {
    /// The pose for a bed block whose head points towards `facing`
    /// (`0` south, `1` west, `2` north, `3` east, the bed block's metadata).
    #[must_use]
    pub fn from_facing(facing: u8) -> Self {
        let (orientation, render_offset, head_x, head_z) = match facing & 3 {
            0 => (90.0, Vec3::new(0.0, 0.0, -1.8), 0.5, 0.9),
            1 => (0.0, Vec3::new(1.8, 0.0, 0.0), 0.1, 0.5),
            2 => (270.0, Vec3::new(0.0, 0.0, 1.8), 0.5, 0.1),
            _ => (180.0, Vec3::new(-1.8, 0.0, 0.0), 0.9, 0.5),
        };
        Self {
            orientation,
            render_offset,
            head_offset: Vec3::new(head_x, 0.6875, head_z),
        }
    }
}

impl Animation {
    /// Starts the hurt flash (`Entity Status` 2, `Animation` 1).
    pub fn hurt(&mut self) {
        self.hurt_time = HURT_TICKS;
    }

    /// Starts the death fall (`Entity Status` 3).
    pub fn die(&mut self) {
        self.dying = true;
    }

    /// Starts an arm swing (`Animation` 0). A swing in its second half
    /// restarts; one in its first half is kept
    /// (MCP `EntityLivingBase.swingItem`).
    pub fn swing_item(&mut self) {
        let swing = &mut self.swing;
        if !swing.in_progress || swing.ticks >= SWING_TICKS / 2 || swing.ticks < 0 {
            swing.ticks = -1;
            swing.in_progress = true;
        }
    }

    /// Advances one tick.
    pub fn tick(&mut self) {
        self.hurt_time = self.hurt_time.saturating_sub(1);
        if self.dying {
            self.death_time = self.death_time.saturating_add(1);
        }
        let swing = &mut self.swing;
        swing.previous_progress = swing.progress;
        if swing.in_progress {
            swing.ticks += 1;
            if swing.ticks >= SWING_TICKS {
                swing.ticks = 0;
                swing.in_progress = false;
            }
        } else {
            swing.ticks = 0;
        }
        swing.progress = f32::from(swing.ticks) / f32::from(SWING_TICKS);
    }

    /// Advances a creeper's fuse one tick: it grows while the state is `1`
    /// and shrinks while it is `-1` (MCP `EntityCreeper.onUpdate`).
    pub fn tick_fuse(&mut self, state: i8) {
        let fuse = &mut self.fuse;
        fuse.previous_ticks = fuse.ticks;
        fuse.ticks = fuse.ticks.saturating_add(state).clamp(0, FUSE_TICKS);
    }

    /// The creeper flash `alpha` of the way into the tick, `0..=1`
    /// (MCP `EntityCreeper.getCreeperFlashIntensity`).
    #[must_use]
    pub fn fuse_flash_at(&self, alpha: f32) -> f32 {
        let from = f32::from(self.fuse.previous_ticks);
        let to = f32::from(self.fuse.ticks);
        (from + (to - from) * alpha) / f32::from(FUSE_TICKS - 2)
    }

    /// True once the death fall is over and the entity is removed.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.death_time >= DEATH_TICKS
    }

    /// True while the red flash shows (MCP `RendererLivingEntity.setBrightness`:
    /// `hurtTime > 0 || deathTime > 0`).
    #[must_use]
    pub fn is_flashing(&self) -> bool {
        self.hurt_time > 0 || self.death_time > 0
    }

    /// Ticks into the death fall plus `alpha`, or `0` while alive.
    #[must_use]
    pub fn death_time_at(&self, alpha: f32) -> f32 {
        if self.death_time == 0 {
            0.0
        } else {
            f32::from(self.death_time) + alpha
        }
    }

    /// The arm swing `alpha` of the way into the tick
    /// (MCP `EntityLivingBase.getSwingProgress`).
    #[must_use]
    pub fn swing_progress_at(&self, alpha: f32) -> f32 {
        let mut delta = self.swing.progress - self.swing.previous_progress;
        if delta < 0.0 {
            delta += 1.0;
        }
        self.swing.previous_progress + delta * alpha
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(animation: &mut Animation, ticks: usize) {
        for _ in 0..ticks {
            animation.tick();
        }
    }

    #[test]
    fn the_hurt_flash_lasts_ten_ticks() {
        let mut animation = Animation::default();
        animation.hurt();
        assert!(animation.is_flashing());
        run(&mut animation, 9);
        assert!(animation.is_flashing());
        run(&mut animation, 1);
        assert!(!animation.is_flashing());
    }

    #[test]
    fn a_dying_entity_falls_for_twenty_ticks_then_is_finished() {
        let mut animation = Animation::default();
        animation.die();
        assert_eq!(animation.death_time_at(0.5), 0.0, "not yet ticked");
        run(&mut animation, 1);
        assert_eq!(animation.death_time_at(0.5), 1.5);
        assert!(animation.is_flashing(), "a corpse stays red");
        run(&mut animation, 18);
        assert!(!animation.is_finished());
        run(&mut animation, 1);
        assert!(animation.is_finished());
    }

    #[test]
    fn a_living_entity_has_no_death_time() {
        let mut animation = Animation::default();
        run(&mut animation, 30);
        assert_eq!(animation.death_time_at(1.0), 0.0);
        assert!(!animation.is_finished());
    }

    #[test]
    fn a_swing_runs_from_zero_to_one_in_six_ticks() {
        let mut animation = Animation::default();
        animation.swing_item();
        let mut progress = Vec::new();
        for _ in 0..7 {
            animation.tick();
            progress.push(animation.swing_progress_at(1.0));
        }
        let expected = [
            0.0,
            1.0 / 6.0,
            2.0 / 6.0,
            3.0 / 6.0,
            4.0 / 6.0,
            5.0 / 6.0,
            // The wrap of 5/6 to 0 interpolates upwards, to 1.
            1.0,
        ];
        for (got, want) in progress.iter().zip(expected) {
            assert!((got - want).abs() < 1e-6, "{progress:?}");
        }
    }

    #[test]
    fn the_swing_wraps_smoothly_into_the_next_tick() {
        let mut animation = Animation::default();
        animation.swing_item();
        run(&mut animation, 7);
        // progress went 5/6 -> 0: the delta wraps by one instead of going back.
        let halfway = animation.swing_progress_at(0.5);
        assert!(
            (halfway - (5.0 / 6.0 + 0.5 / 6.0)).abs() < 1e-6,
            "{halfway}"
        );
    }

    #[test]
    fn a_swing_in_its_first_half_is_not_restarted() {
        let mut animation = Animation::default();
        animation.swing_item();
        run(&mut animation, 2);
        let before = animation.swing_progress_at(1.0);
        animation.swing_item();
        animation.tick();
        assert!(animation.swing_progress_at(1.0) > before);
    }

    #[test]
    fn a_swing_in_its_second_half_restarts() {
        let mut animation = Animation::default();
        animation.swing_item();
        run(&mut animation, 5);
        animation.swing_item();
        run(&mut animation, 2);
        assert!((animation.swing_progress_at(1.0) - 1.0 / 6.0).abs() < 1e-6);
    }

    #[test]
    fn a_fusing_creeper_swells_to_the_full_flash_and_a_calm_one_deflates() {
        let mut animation = Animation::default();
        for _ in 0..40 {
            animation.tick_fuse(1);
        }
        assert!(
            animation.fuse_flash_at(1.0) > 1.0,
            "30 / 28 at the end of the fuse"
        );
        animation.tick_fuse(-1);
        let calmer = animation.fuse_flash_at(1.0);
        for _ in 0..40 {
            animation.tick_fuse(-1);
        }
        assert_eq!(animation.fuse_flash_at(1.0), 0.0);
        assert!(calmer < 30.0 / 28.0);
    }

    #[test]
    fn the_flash_interpolates_between_ticks() {
        let mut animation = Animation::default();
        animation.tick_fuse(1);
        assert!((animation.fuse_flash_at(0.5) - 0.5 / 28.0).abs() < 1e-6);
    }

    #[test]
    fn beds_turn_the_sleeper_by_their_facing() {
        let south = BedPose::from_facing(0);
        assert_eq!(south.orientation, 90.0);
        assert_eq!(south.render_offset, Vec3::new(0.0, 0.0, -1.8));
        assert_eq!(south.head_offset, Vec3::new(0.5, 0.6875, 0.9));
        assert_eq!(BedPose::from_facing(3).orientation, 180.0);
        assert_eq!(BedPose::from_facing(2).orientation, 270.0);
        assert_eq!(BedPose::from_facing(1).orientation, 0.0);
    }
}
