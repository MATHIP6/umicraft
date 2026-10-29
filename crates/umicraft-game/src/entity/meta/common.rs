// SPDX-License-Identifier: MIT OR Apache-2.0

//! The entries every entity or every living entity has.

use std::sync::Arc;

use umicraft_protocol::packets::play::Metadata;

use super::EntityMeta;
use crate::entity::kind::EntityKind;

/// Entry of the custom name (MCP `Entity.entityInit`).
const CUSTOM_NAME: u8 = 2;
/// Entry of the custom name visibility.
const CUSTOM_NAME_VISIBLE: u8 = 3;
/// Entry of the health (MCP `EntityLivingBase.entityInit`).
const HEALTH: u8 = 6;
/// Entry of the potion effect colour.
const POTION_COLOR: u8 = 7;
/// Entry of the "potion effects are ambient" flag.
const POTION_AMBIENT: u8 = 8;
/// Entry of the number of arrows stuck in the body.
const ARROWS: u8 = 9;
/// Entry of the age of an ageable mob (a byte in 1.8, negative for a child).
const AGE: u8 = 12;
/// Entry of the absorption hearts of a player (MCP `EntityPlayer.entityInit`).
const ABSORPTION: u8 = 17;

/// The living-entity entries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LivingMeta {
    /// Health in half hearts; the entity dies at zero.
    pub health: f32,
    /// Mixed colour of the active potion effects, `0` for none.
    pub potion_color: i32,
    /// True when the effects come from a beacon (translucent particles).
    pub potion_ambient: bool,
    /// Arrows stuck in the body.
    pub arrows: u8,
    /// Absorption hearts (players).
    pub absorption: f32,
}

impl Default for LivingMeta {
    fn default() -> Self {
        Self {
            health: 1.0,
            potion_color: 0,
            potion_ambient: false,
            arrows: 0,
            absorption: 0.0,
        }
    }
}

/// True for the classes whose entry 12 is the age byte of `EntityAgeable`.
const fn is_ageable(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Cow
            | EntityKind::Pig
            | EntityKind::Sheep
            | EntityKind::Chicken
            | EntityKind::Mooshroom
            | EntityKind::Villager
            | EntityKind::Wolf
            | EntityKind::Ocelot
            | EntityKind::Horse
            | EntityKind::Rabbit
    )
}

/// Decodes the common entries into `meta`.
pub(super) fn apply(meta: &mut EntityMeta, kind: EntityKind, metadata: &Metadata) {
    if let Some(name) = metadata.get_string(CUSTOM_NAME) {
        meta.custom_name = (!name.is_empty()).then(|| Arc::from(name));
    }
    if let Some(visible) = metadata.get_byte(CUSTOM_NAME_VISIBLE) {
        meta.custom_name_visible = visible != 0;
    }
    if !kind.is_living() {
        return;
    }
    let living = &mut meta.living;
    if let Some(health) = metadata.get_float(HEALTH) {
        living.health = health;
    }
    if let Some(color) = metadata.get_int(POTION_COLOR) {
        living.potion_color = color;
    }
    if let Some(ambient) = metadata.get_byte(POTION_AMBIENT) {
        living.potion_ambient = ambient != 0;
    }
    if let Some(arrows) = metadata.get_byte(ARROWS) {
        living.arrows = arrows;
    }
    if kind == EntityKind::Player {
        if let Some(absorption) = metadata.get_float(ABSORPTION) {
            living.absorption = absorption;
        }
    }
    if is_ageable(kind) {
        if let Some(age) = metadata.get_byte(AGE) {
            // MCP `EntityAgeable.getGrowingAge` reads the byte on a client
            // and `isChild` tests `< 0`.
            meta.baby = (age as i8) < 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{byte, float, int, meta, string};
    use super::*;

    #[test]
    fn the_living_entries_are_decoded() {
        let mut state = EntityMeta::default();
        state.apply(
            EntityKind::Zombie,
            &meta(vec![
                float(6, 12.5),
                int(7, 0x00FF_00AA),
                byte(8, 1),
                byte(9, 3),
            ]),
        );
        assert_eq!(state.living.health, 12.5);
        assert_eq!(state.living.potion_color, 0x00FF_00AA);
        assert!(state.living.potion_ambient);
        assert_eq!(state.living.arrows, 3);
    }

    #[test]
    fn a_player_has_absorption() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Player, &meta(vec![float(17, 4.0)]));
        assert_eq!(state.living.absorption, 4.0);
        let mut zombie = EntityMeta::default();
        zombie.apply(EntityKind::Zombie, &meta(vec![float(17, 4.0)]));
        assert_eq!(zombie.living.absorption, 0.0);
    }

    #[test]
    fn the_custom_name_appears_and_clears() {
        let mut state = EntityMeta::default();
        state.apply(
            EntityKind::Cow,
            &meta(vec![string(2, "Bessie"), byte(3, 1)]),
        );
        assert_eq!(state.custom_name.as_deref(), Some("Bessie"));
        assert!(state.custom_name_visible);
        state.apply(EntityKind::Cow, &meta(vec![string(2, "")]));
        assert_eq!(state.custom_name, None);
    }

    #[test]
    fn a_negative_age_is_a_child_for_ageable_mobs_only() {
        let mut calf = EntityMeta::default();
        calf.apply(EntityKind::Cow, &meta(vec![byte(12, 0xFF)]));
        assert!(calf.baby);
        calf.apply(EntityKind::Cow, &meta(vec![byte(12, 0)]));
        assert!(!calf.baby, "grown up");
        let mut creeper = EntityMeta::default();
        creeper.apply(EntityKind::Creeper, &meta(vec![byte(12, 0xFF)]));
        assert!(!creeper.baby);
    }

    #[test]
    fn objects_have_no_living_entries() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Arrow, &meta(vec![float(6, 3.0)]));
        assert_eq!(state.living, LivingMeta::default());
    }
}
