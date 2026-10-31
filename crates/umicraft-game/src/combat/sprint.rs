// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sprint reset after a hit.
//!
//! MCP `EntityPlayer.attackTargetEntityWithCurrentItem` multiplies the
//! horizontal motion of the attacker by 0.6 and stops its sprint when the hit
//! lands with knockback (a Knockback enchantment on the held item, or a
//! sprint). "Lands" is `target.attackEntityFrom(..)`: on the client world a
//! mob answers false (`EntityLivingBase.attackEntityFrom` returns early when
//! `isRemote`) while `EntityOtherPlayerMP.attackEntityFrom` answers true. The
//! reset therefore happens on the client only against another player; against
//! mobs the server resets the sprint it tracks and the client keeps its own.

use crate::entity::EntityKind;

/// The factor on the horizontal motion (MCP `motionX *= 0.6D`).
pub const SPRINT_RESET_SCALE: f64 = 0.6;

/// The Knockback enchantment id (MCP `Enchantment.knockback`).
pub const KNOCKBACK: i16 = 19;

/// What a hit changes on the attacker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SprintReset {
    /// The factor to apply to the horizontal motion.
    pub scale: f64,
}

/// The change a hit on `target` makes to the attacker, if any.
///
/// `knockback` is the Knockback level of the held item.
#[must_use]
pub fn sprint_reset(target: EntityKind, sprinting: bool, knockback: u32) -> Option<SprintReset> {
    let lands_locally = target == EntityKind::Player;
    let pushes = sprinting || knockback > 0;
    (lands_locally && pushes).then_some(SprintReset {
        scale: SPRINT_RESET_SCALE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sprinting_hit_on_a_player_resets_the_sprint() {
        assert_eq!(
            sprint_reset(EntityKind::Player, true, 0),
            Some(SprintReset { scale: 0.6 })
        );
    }

    #[test]
    fn knockback_alone_resets_it_too() {
        assert!(sprint_reset(EntityKind::Player, false, 1).is_some());
    }

    #[test]
    fn a_walking_hit_without_knockback_changes_nothing() {
        assert_eq!(sprint_reset(EntityKind::Player, false, 0), None);
    }

    #[test]
    fn a_mob_does_not_reset_the_client_sprint() {
        // The client world answers false to `attackEntityFrom` for mobs.
        assert_eq!(sprint_reset(EntityKind::Zombie, true, 2), None);
    }
}
