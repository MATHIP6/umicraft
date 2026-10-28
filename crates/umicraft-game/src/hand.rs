// SPDX-License-Identifier: MIT OR Apache-2.0

//! The first-person hand: the arm swing, the equip animation and the pose of
//! the arm in front of the camera.
//!
//! [`HandState`] keeps the animation state of the local player (MCP
//! `EntityLivingBase` swing fields, `ItemRenderer` equip progress,
//! `EntityPlayerSP.renderArmPitch` / `renderArmYaw`); [`HandState::pose`]
//! turns it into the plain [`HandPose`] the renderer interpolates, and
//! [`HandPose::arm_matrix`] is the chain of `ItemRenderer.func_178095_a`.
//! Held items are drawn once the item registry exists (milestone P3); until
//! then only the bare arm is.

use glam::{Mat4, Vec3};
use umicraft_protocol::packets::play::ItemStack;

/// The ticks of an arm swing without Haste or Mining Fatigue
/// (MCP `EntityLivingBase.getArmSwingAnimationEnd`).
pub const SWING_TICKS: u32 = 6;

/// The most the equip progress moves per tick (MCP `ItemRenderer.java:554`).
const EQUIP_STEP: f32 = 0.4;

/// The part of the distance to the view the arm sway closes each tick
/// (MCP `EntityPlayerSP.java:701`).
const SWAY_SMOOTHING: f32 = 0.5;

/// The share of the sway applied to the arm (MCP `ItemRenderer.java:115`).
const SWAY_SCALE: f32 = 0.1;

/// The Haste and Mining Fatigue effects that change the swing speed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SwingEffects {
    /// The Haste amplifier, if active.
    pub haste: Option<i8>,
    /// The Mining Fatigue amplifier, if active.
    pub fatigue: Option<i8>,
}

impl SwingEffects {
    /// The length of a swing in ticks (MCP `getArmSwingAnimationEnd`).
    #[must_use]
    pub fn swing_ticks(self) -> u32 {
        let level = |amplifier: i8| u32::from(amplifier.max(-1).unsigned_abs()) + 1;
        match (self.haste, self.fatigue) {
            (Some(amplifier), _) => SWING_TICKS.saturating_sub(level(amplifier)).max(1),
            (None, Some(amplifier)) => SWING_TICKS + level(amplifier) * 2,
            (None, None) => SWING_TICKS,
        }
    }
}

/// The animation state of the hand of the local player.
#[derive(Debug, Clone, Default)]
pub struct HandState {
    /// The item the hand shows, which lags behind the held one while the
    /// equip animation lowers it (MCP `ItemRenderer.itemToRender`).
    item: Option<ItemStack>,
    equipped: f32,
    swinging: bool,
    swing_ticks_done: i32,
    swing_progress: f32,
    previous_swing_progress: f32,
    sway_pitch: f32,
    sway_yaw: f32,
    pitch_delta: f32,
    yaw_delta: f32,
}

impl HandState {
    /// A hand at rest with nothing in it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            equipped: 1.0,
            ..Self::default()
        }
    }

    /// Starts an arm swing (MCP `EntityLivingBase.swingItem`): it only
    /// restarts once half of the running swing is over.
    pub fn swing(&mut self, effects: SwingEffects) {
        let half = i32::try_from(effects.swing_ticks() / 2).unwrap_or(i32::MAX);
        if !self.swinging || self.swing_ticks_done >= half || self.swing_ticks_done < 0 {
            self.swing_ticks_done = -1;
            self.swinging = true;
        }
    }

    /// Advances the hand by one tick.
    ///
    /// `held` is the item in the selected slot, `yaw` and `pitch` the view
    /// in degrees.
    pub fn tick(&mut self, held: Option<&ItemStack>, yaw: f32, pitch: f32, effects: SwingEffects) {
        self.tick_swing(effects);
        self.tick_equip(held);
        // MCP `EntityPlayerSP.updateEntityActionState`.
        self.sway_pitch += (pitch - self.sway_pitch) * SWAY_SMOOTHING;
        self.sway_yaw += (yaw - self.sway_yaw) * SWAY_SMOOTHING;
        self.pitch_delta = (pitch - self.sway_pitch) * SWAY_SCALE;
        self.yaw_delta = (yaw - self.sway_yaw) * SWAY_SCALE;
    }

    /// MCP `EntityLivingBase.updateArmSwingProgress`.
    fn tick_swing(&mut self, effects: SwingEffects) {
        let end = effects.swing_ticks();
        self.previous_swing_progress = self.swing_progress;
        if self.swinging {
            self.swing_ticks_done += 1;
            if self.swing_ticks_done >= i32::try_from(end).unwrap_or(i32::MAX) {
                self.swing_ticks_done = 0;
                self.swinging = false;
            }
        } else {
            self.swing_ticks_done = 0;
        }
        self.swing_progress = self.swing_ticks_done as f32 / end as f32;
    }

    /// MCP `ItemRenderer.updateEquippedItem`.
    fn tick_equip(&mut self, held: Option<&ItemStack>) {
        let changed = match (self.item.as_ref(), held) {
            (None, None) => false,
            (Some(shown), Some(now)) => shown != now,
            _ => true,
        };
        let target = if changed { 0.0 } else { 1.0 };
        self.equipped += (target - self.equipped).clamp(-EQUIP_STEP, EQUIP_STEP);
        if self.equipped < 0.1 {
            self.item = held.cloned();
        }
    }

    /// The swing progress `partial` ticks after the last tick (MCP
    /// `getSwingProgress`: a swing that wrapped counts as more than 1).
    #[must_use]
    pub fn swing_progress(&self, partial: f32) -> f32 {
        let mut delta = self.swing_progress - self.previous_swing_progress;
        if delta < 0.0 {
            delta += 1.0;
        }
        self.previous_swing_progress + delta * partial
    }

    /// The pose of the hand at the end of the last tick.
    #[must_use]
    pub fn pose(&self) -> HandPose {
        HandPose {
            shows_arm: self.item.is_none(),
            equip: 1.0 - self.equipped,
            swing: self.swing_progress(1.0),
            sway_pitch: self.pitch_delta,
            sway_yaw: self.yaw_delta,
        }
    }
}

/// Where the arm is in front of the camera, as plain data.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HandPose {
    /// Whether the bare arm is drawn (the hand is empty).
    pub shows_arm: bool,
    /// How far the hand is lowered, `0` raised to `1` out of sight.
    pub equip: f32,
    /// The swing progress, `0` at rest to `1` at the end of a swing.
    pub swing: f32,
    /// The pitch sway of the arm in degrees.
    pub sway_pitch: f32,
    /// The yaw sway of the arm in degrees.
    pub sway_yaw: f32,
}

impl HandPose {
    /// The blend of the poses of two frames.
    ///
    /// A swing that wrapped from the end back to 0 counts as 1, like MCP
    /// `getSwingProgress`, so the arm does not swing backwards.
    #[must_use]
    pub fn lerp(self, other: Self, alpha: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * alpha;
        let swing_end = if other.swing < self.swing {
            other.swing + 1.0
        } else {
            other.swing
        };
        Self {
            shows_arm: other.shows_arm,
            equip: mix(self.equip, other.equip),
            swing: mix(self.swing, swing_end),
            sway_pitch: mix(self.sway_pitch, other.sway_pitch),
            sway_yaw: mix(self.sway_yaw, other.sway_yaw),
        }
    }

    /// The transform from a held item's frame to the camera space.
    ///
    /// MCP `ItemRenderer.renderItemInFirstPerson` for an idle or swinging
    /// hand: the sway, `func_178105_d` (the swing shift of an item),
    /// `transformFirstPersonItem` (placement, equip drop, swing rotations and
    /// the 0.4 scale), in the order of the `GlStateManager` calls. The item's
    /// own first-person pose follows (`umicraft_model::item::pose::held_pose`).
    #[must_use]
    pub fn item_matrix(&self) -> Mat4 {
        let swing = self.swing;
        let root = swing.sqrt();
        let pi = std::f32::consts::PI;
        let translate = |x: f32, y: f32, z: f32| Mat4::from_translation(Vec3::new(x, y, z));
        let rotate = |degrees: f32, axis: Vec3| Mat4::from_axis_angle(axis, degrees.to_radians());
        let yaw_swing = (swing * swing * pi).sin();
        let roll_swing = (root * pi).sin();

        rotate(self.sway_pitch, Vec3::X)
            * rotate(self.sway_yaw, Vec3::Y)
            * translate(
                -0.4 * (root * pi).sin(),
                0.2 * (root * pi * 2.0).sin(),
                -0.2 * (swing * pi).sin(),
            )
            * translate(0.56, -0.52, -0.719_999_97)
            * translate(0.0, self.equip * -0.6, 0.0)
            * rotate(45.0, Vec3::Y)
            * rotate(yaw_swing * -20.0, Vec3::Y)
            * rotate(roll_swing * -20.0, Vec3::Z)
            * rotate(roll_swing * -80.0, Vec3::X)
            * Mat4::from_scale(Vec3::splat(0.4))
    }

    /// The transform from the arm model (vanilla model space, in blocks) to
    /// the camera space.
    ///
    /// MCP `ItemRenderer.renderItemInFirstPerson` and `func_178095_a`: the
    /// sway, the swing, the equip drop and the fixed placement of the arm,
    /// in the order of the successive `GlStateManager` calls.
    #[must_use]
    pub fn arm_matrix(&self) -> Mat4 {
        let swing = self.swing;
        let root = swing.sqrt();
        let translate = |x: f32, y: f32, z: f32| Mat4::from_translation(Vec3::new(x, y, z));
        let rotate = |degrees: f32, axis: Vec3| Mat4::from_axis_angle(axis, degrees.to_radians());

        let swing_shift = translate(
            -0.3 * (root * std::f32::consts::PI).sin(),
            0.4 * (root * std::f32::consts::PI * 2.0).sin(),
            -0.4 * (swing * std::f32::consts::PI).sin(),
        );
        let roll = (swing * swing * std::f32::consts::PI).sin();
        let turn = (root * std::f32::consts::PI).sin();

        rotate(self.sway_pitch, Vec3::X)
            * rotate(self.sway_yaw, Vec3::Y)
            * swing_shift
            * translate(0.640_000_05, -0.6, -0.719_999_97)
            * translate(0.0, self.equip * -0.6, 0.0)
            * rotate(45.0, Vec3::Y)
            * rotate(turn * 70.0, Vec3::Y)
            * rotate(roll * -20.0, Vec3::Z)
            * translate(-1.0, 3.6, 3.5)
            * rotate(120.0, Vec3::Z)
            * rotate(200.0, Vec3::X)
            * rotate(-135.0, Vec3::Y)
            * translate(5.6, 0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item_id: i16, size: i8) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: size,
            damage: 0,
            nbt: None,
        }
    }

    fn idle() -> SwingEffects {
        SwingEffects::default()
    }

    #[test]
    fn a_resting_item_sits_in_the_lower_right_of_the_view() {
        let rest = HandPose::default();
        let centre = rest.item_matrix().transform_point3(Vec3::ZERO);
        assert!((centre - Vec3::new(0.56, -0.52, -0.719_999_97)).length() < 1e-5);
        let lowered = HandPose { equip: 1.0, ..rest };
        assert!((lowered.item_matrix().transform_point3(Vec3::ZERO).y + 1.12).abs() < 1e-5);
    }

    #[test]
    fn a_swinging_item_moves_away_from_its_resting_place() {
        let swinging = HandPose {
            swing: 0.5,
            ..HandPose::default()
        };
        let moved = swinging.item_matrix().transform_point3(Vec3::ZERO);
        let rest = HandPose::default()
            .item_matrix()
            .transform_point3(Vec3::ZERO);
        assert!((moved - rest).length() > 0.1);
    }

    #[test]
    fn a_swing_runs_six_ticks_and_ends() {
        let mut hand = HandState::new();
        hand.swing(idle());
        let mut seen = Vec::new();
        for _ in 0..8 {
            hand.tick(None, 0.0, 0.0, idle());
            seen.push(hand.pose().swing);
        }
        // Progress 0, 1/6 ... 5/6 (the first tick sets -1 -> 0), then rest.
        assert_eq!(seen[0], 0.0);
        assert!((seen[3] - 3.0 / 6.0).abs() < 1e-6);
        assert!((seen[5] - 5.0 / 6.0).abs() < 1e-6);
        assert_eq!(
            seen[6], 1.0,
            "the wrapped tick counts as the end of the swing"
        );
        assert_eq!(hand.swing_progress(0.5), 0.0);
        assert!(!hand.swinging);
    }

    #[test]
    fn a_swing_restarts_only_after_half_of_it() {
        let mut hand = HandState::new();
        hand.swing(idle());
        hand.tick(None, 0.0, 0.0, idle());
        hand.tick(None, 0.0, 0.0, idle());
        let early = hand.swing_ticks_done;
        hand.swing(idle());
        assert_eq!(hand.swing_ticks_done, early, "too early to restart");
        hand.tick(None, 0.0, 0.0, idle());
        hand.tick(None, 0.0, 0.0, idle());
        hand.swing(idle());
        assert_eq!(hand.swing_ticks_done, -1, "half way: it restarts");
    }

    #[test]
    fn haste_shortens_and_mining_fatigue_lengthens_the_swing() {
        let haste = SwingEffects {
            haste: Some(1),
            fatigue: None,
        };
        let fatigue = SwingEffects {
            haste: None,
            fatigue: Some(0),
        };
        assert_eq!(haste.swing_ticks(), 4);
        assert_eq!(fatigue.swing_ticks(), 8);
        assert_eq!(idle().swing_ticks(), 6);
    }

    #[test]
    fn the_hand_starts_raised_and_the_arm_is_shown_while_it_is_empty() {
        let mut hand = HandState::new();
        hand.tick(None, 0.0, 0.0, idle());
        let pose = hand.pose();
        assert!(pose.shows_arm);
        assert_eq!(pose.equip, 0.0);
    }

    #[test]
    fn taking_an_item_lowers_the_arm_then_hides_it() {
        let mut hand = HandState::new();
        let sword = stack(276, 1);
        let mut lowest: f32 = 0.0;
        let mut shown = Vec::new();
        for _ in 0..10 {
            hand.tick(Some(&sword), 0.0, 0.0, idle());
            lowest = lowest.max(hand.pose().equip);
            shown.push(hand.pose().shows_arm);
        }
        assert!(lowest > 0.7, "the hand dips out of sight: {lowest}");
        assert!(shown[0], "the arm is still drawn at the start of the dip");
        assert!(!shown[9], "the arm goes once the item is shown");
        assert!(hand.pose().equip < 0.2, "and the item rises");
    }

    #[test]
    fn a_changed_stack_size_dips_the_hand_like_vanilla() {
        let mut hand = HandState::new();
        for _ in 0..10 {
            hand.tick(Some(&stack(1, 10)), 0.0, 0.0, idle());
        }
        assert!(hand.pose().equip < 0.01);
        hand.tick(Some(&stack(1, 9)), 0.0, 0.0, idle());
        assert!(hand.pose().equip > 0.3);
    }

    #[test]
    fn the_arm_sways_against_a_fast_turn_and_settles() {
        let mut hand = HandState::new();
        hand.tick(None, 90.0, 0.0, idle());
        assert!(hand.pose().sway_yaw > 4.0, "{}", hand.pose().sway_yaw);
        for _ in 0..20 {
            hand.tick(None, 90.0, 0.0, idle());
        }
        assert!(hand.pose().sway_yaw.abs() < 0.01);
    }

    #[test]
    fn a_wrapped_swing_does_not_swing_back() {
        let end = HandPose {
            swing: 1.0,
            ..HandPose::default()
        };
        let rest = HandPose::default();
        assert_eq!(end.lerp(rest, 0.5).swing, 1.0);
        let rising = HandPose {
            swing: 0.5,
            ..HandPose::default()
        };
        assert!((rest.lerp(rising, 0.5).swing - 0.25).abs() < 1e-6);
    }

    #[test]
    fn the_resting_arm_is_in_the_lower_right_in_front_of_the_camera() {
        // The shoulder pivot of the arm model, in blocks (vanilla space).
        let shoulder = Vec3::new(-5.0, 2.0, 0.0) / 16.0;
        let at = HandPose::default().arm_matrix().transform_point3(shoulder);
        assert!(at.x > 0.2, "to the right: {at:?}");
        assert!(at.y < -0.2, "below the eyes: {at:?}");
        assert!(at.z < -0.2, "in front of the camera: {at:?}");
        assert!(at.length() < 1.5);
    }

    #[test]
    fn the_equip_drop_lowers_the_arm_and_the_swing_moves_it() {
        let shoulder = Vec3::new(-5.0, 2.0, 0.0) / 16.0;
        let rest = HandPose::default().arm_matrix().transform_point3(shoulder);
        let lowered = HandPose {
            equip: 1.0,
            ..HandPose::default()
        }
        .arm_matrix()
        .transform_point3(shoulder);
        assert!(lowered.y < rest.y - 0.4);
        let swung = HandPose {
            swing: 0.5,
            ..HandPose::default()
        }
        .arm_matrix()
        .transform_point3(shoulder);
        assert!((swung - rest).length() > 0.2);
    }
}
