// SPDX-License-Identifier: MIT OR Apache-2.0

//! The 1.7 animations the "1.7 Animations" built-in mod brings back
//! (ADR-0079).
//!
//! Each one is a difference between the 1.7.10 and the 1.8.9 client found by
//! reading both (`docs/sources.md`, "1.7 animations"). They only change what
//! the local player sees: no packet differs, so the server and the
//! anti-cheat see a 1.8 client (ADR-0076).
//!
//! - [`LegacyAnimations::block_hit`]: an item in use (a sword raised, food,
//!   a drawn bow) still turns with the arm swing, and holding attack at a
//!   block while using an item swings the arm (`HandPose::item_matrix`,
//!   the attack tick of the client);
//! - [`LegacyAnimations::equip`]: the hand only dips again when the item or
//!   its damage changes, not its count or its tags ([`EquipRule`]);
//! - [`LegacyAnimations::sneak_camera`]: the view glides down and up when
//!   sneaking instead of snapping ([`SneakCamera`]);
//! - [`LegacyAnimations::item_position`]: a flat item in the first-person
//!   hand sits where 1.7 drew it, smaller and further left, which is most
//!   visible on a blocking sword (`HandPose::legacy_item_position`).

use umicraft_protocol::packets::play::ItemStack;

use crate::physics::SNEAKING_EYE_HEIGHT_OFFSET;

/// Which 1.7 animations are on. All off is the 1.8 client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LegacyAnimations {
    /// An item in use turns with the swing, and attack at a block swings the
    /// arm while an item is in use.
    pub block_hit: bool,
    /// The 1.7 rule of the equip animation ([`EquipRule::V1_7`]).
    pub equip: bool,
    /// The 1.7 glide of the view when sneaking ([`SneakCamera`]).
    pub sneak_camera: bool,
    /// The 1.7 place of a flat item in the first-person hand.
    pub item_position: bool,
}

impl LegacyAnimations {
    /// The equip rule these animations select.
    #[must_use]
    pub const fn equip_rule(self) -> EquipRule {
        if self.equip {
            EquipRule::V1_7
        } else {
            EquipRule::V1_8
        }
    }
}

/// When a new stack in the hand makes it dip and rise again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EquipRule {
    /// Any difference, the count and the tags included
    /// (MCP 1.8.9 `ItemRenderer.updateEquippedItem`, `getIsItemStackEqual`):
    /// eating or placing blocks can dip the hand.
    #[default]
    V1_8,
    /// Only a different item or damage (MCP 1.7.10
    /// `ItemRenderer.updateEquippedItem`, lines 695-699): the shown stack is
    /// swapped for the new one without a dip otherwise.
    V1_7,
}

impl EquipRule {
    /// Whether `held` is the same item as `shown` for this rule, so that the
    /// hand stays raised.
    #[must_use]
    pub fn same_item(self, shown: Option<&ItemStack>, held: Option<&ItemStack>) -> bool {
        match (shown, held) {
            (None, None) => true,
            (Some(shown), Some(held)) => match self {
                Self::V1_8 => shown == held,
                Self::V1_7 => shown.item_id == held.item_id && shown.damage == held.damage,
            },
            _ => false,
        }
    }
}

/// The value `ySize` is raised to on each tick of sneaking
/// (MCP 1.7.10 `EntityPlayerSP.onLivingUpdate`, line 196).
const SNEAK_Y_SIZE: f32 = 0.2;

/// The share of `ySize` that is left after each move
/// (MCP 1.7.10 `Entity.moveEntity`, line 623).
const Y_SIZE_DECAY: f32 = 0.4;

/// The drop of the view of a sneaking player as 1.7 animated it.
///
/// 1.7.10 lowers the player by `ySize` (`posY = minY + yOffset - ySize`):
/// every tick of sneaking raises it to 0.2, then the move multiplies it by
/// 0.4 before placing the player, so a sneaking view sits 0.08 lower, the
/// 1.8 drop. The difference is in the motion: `posY` is interpolated between
/// ticks, so the view glides down over a tick, and once the key is let go
/// `ySize` keeps shrinking by 0.4 a tick, so the view rises back smoothly.
/// 1.8 moves the eye by the full 0.08 at once (`getEyeHeight`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SneakCamera {
    previous: f32,
    current: f32,
}

impl SneakCamera {
    /// Advances the drop by one tick, `sneaking` being the state of the tick.
    pub fn tick(&mut self, sneaking: bool) {
        self.previous = self.current;
        let raised = if sneaking {
            self.current.max(SNEAK_Y_SIZE)
        } else {
            self.current
        };
        self.current = raised * Y_SIZE_DECAY;
    }

    /// How far below the standing eye the view is, in blocks: `at_end` is the
    /// end of the last tick, otherwise its start.
    #[must_use]
    pub fn drop(&self, at_end: bool) -> f32 {
        if at_end {
            self.current
        } else {
            self.previous
        }
    }

    /// What to add to the height of a 1.8 eye (`sneaking` gives its drop) to
    /// put it where 1.7 had it.
    #[must_use]
    pub fn eye_correction(&self, sneaking: bool, at_end: bool) -> f64 {
        let vanilla = if sneaking {
            SNEAKING_EYE_HEIGHT_OFFSET
        } else {
            0.0
        };
        f64::from(vanilla - self.drop(at_end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item_id: i16, stack_size: i8, damage: i16) -> ItemStack {
        ItemStack {
            item_id,
            stack_size,
            damage,
            nbt: None,
        }
    }

    #[test]
    fn all_off_is_the_1_8_client() {
        let off = LegacyAnimations::default();
        assert!(!off.block_hit && !off.equip && !off.sneak_camera && !off.item_position);
        assert_eq!(off.equip_rule(), EquipRule::V1_8);
    }

    #[test]
    fn the_1_8_rule_dips_on_any_change_the_1_7_one_on_item_or_damage() {
        let ten = stack(1, 10, 0);
        let nine = stack(1, 9, 0);
        assert!(!EquipRule::V1_8.same_item(Some(&ten), Some(&nine)));
        assert!(EquipRule::V1_7.same_item(Some(&ten), Some(&nine)));
        let worn = stack(276, 1, 3);
        let new = stack(276, 1, 0);
        assert!(!EquipRule::V1_7.same_item(Some(&worn), Some(&new)));
        assert!(!EquipRule::V1_7.same_item(Some(&ten), Some(&new)));
        for rule in [EquipRule::V1_7, EquipRule::V1_8] {
            assert!(rule.same_item(None, None));
            assert!(!rule.same_item(None, Some(&ten)));
            assert!(!rule.same_item(Some(&ten), None));
        }
    }

    #[test]
    fn a_sneaking_view_settles_at_the_1_8_drop_within_one_tick() {
        let mut camera = SneakCamera::default();
        camera.tick(true);
        assert_eq!(camera.drop(false), 0.0, "it glides from the standing eye");
        assert!((camera.drop(true) - 0.08).abs() < 1e-6);
        camera.tick(true);
        assert!((camera.drop(true) - 0.08).abs() < 1e-6, "and stays there");
        assert!(camera.eye_correction(true, true).abs() < 1e-6);
    }

    #[test]
    fn the_view_rises_back_by_forty_percent_a_tick() {
        let mut camera = SneakCamera::default();
        camera.tick(true);
        camera.tick(false);
        assert!((camera.drop(true) - 0.032).abs() < 1e-6);
        camera.tick(false);
        assert!((camera.drop(true) - 0.0128).abs() < 1e-6);
        // 1.8 is already standing: the correction lowers its eye.
        assert!((camera.eye_correction(false, true) + 0.0128).abs() < 1e-6);
        for _ in 0..20 {
            camera.tick(false);
        }
        assert!(camera.drop(true) < 1e-6);
    }
}
