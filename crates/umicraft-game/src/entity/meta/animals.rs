// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sheep, pig, wolf, ocelot and rabbit entries.

use umicraft_protocol::packets::play::Metadata;

use super::Variant;

/// Sheep entry: colour in the low nibble, shorn in bit 4
/// (MCP `EntitySheep.getFleeceColor` / `getSheared`).
const SHEEP_WOOL: u8 = 16;
/// Pig entry: saddle (MCP `EntityPig.getSaddled`).
const PIG_SADDLE: u8 = 16;
/// Tameable entry: bit 0 sitting, bit 1 angry (wolf), bit 2 tamed
/// (MCP `EntityTameable`, `EntityWolf`).
const TAMEABLE_FLAGS: u8 = 16;
/// Wolf entry: collar dye colour.
const WOLF_COLLAR: u8 = 20;
/// Wolf entry: begging (head tilt).
const WOLF_BEGGING: u8 = 19;
/// Ocelot entry: coat (`0` wild, `1` tuxedo, `2` tabby, `3` siamese).
const OCELOT_SKIN: u8 = 18;
/// Rabbit entry: coat (MCP `EntityRabbit.getRabbitType`).
const RABBIT_TYPE: u8 = 18;

/// Sheep wool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SheepMeta {
    /// Dye colour `0..=15` (MCP `EnumDyeColor` metadata, `0` white).
    pub color: u8,
    /// True when the sheep has been sheared.
    pub sheared: bool,
}

/// Wolf state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WolfMeta {
    /// Sitting.
    pub sitting: bool,
    /// Angry (red eyes).
    pub angry: bool,
    /// Tamed (collar).
    pub tamed: bool,
    /// Collar dye colour `0..=15`; vanilla starts with red (`14`).
    pub collar: u8,
    /// Head tilted as when begging for food.
    pub begging: bool,
}

impl Default for WolfMeta {
    fn default() -> Self {
        Self {
            sitting: false,
            angry: false,
            tamed: false,
            collar: 14,
            begging: false,
        }
    }
}

/// Ocelot state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OcelotMeta {
    /// Sitting.
    pub sitting: bool,
    /// Tamed (a house cat).
    pub tamed: bool,
    /// Coat `0..=3`.
    pub skin: u8,
}

/// Rabbit coat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RabbitMeta {
    /// MCP rabbit type: `0` brown, `1` white, `2` black, `3` black and
    /// white, `4` gold, `5` salt and pepper, `99` the killer bunny.
    pub kind: u8,
}

pub(super) fn apply_sheep(variant: &mut Variant, metadata: &Metadata) {
    let Some(wool) = metadata.get_byte(SHEEP_WOOL) else {
        return;
    };
    *variant = Variant::Sheep(super::SheepMeta {
        color: wool & 15,
        sheared: wool & 16 != 0,
    });
}

pub(super) fn apply_pig(variant: &mut Variant, metadata: &Metadata) {
    if let Some(saddle) = metadata.get_byte(PIG_SADDLE) {
        *variant = Variant::Pig {
            saddled: saddle != 0,
        };
    }
}

pub(super) fn apply_wolf(variant: &mut Variant, metadata: &Metadata) {
    let mut wolf = match variant {
        Variant::Wolf(wolf) => *wolf,
        _ => WolfMeta::default(),
    };
    if let Some(flags) = metadata.get_byte(TAMEABLE_FLAGS) {
        wolf.sitting = flags & 1 != 0;
        wolf.angry = flags & 2 != 0;
        wolf.tamed = flags & 4 != 0;
    }
    if let Some(collar) = metadata.get_byte(WOLF_COLLAR) {
        wolf.collar = collar & 15;
    }
    if let Some(begging) = metadata.get_byte(WOLF_BEGGING) {
        wolf.begging = begging != 0;
    }
    *variant = Variant::Wolf(wolf);
}

pub(super) fn apply_ocelot(variant: &mut Variant, metadata: &Metadata) {
    let mut ocelot = match variant {
        Variant::Ocelot(ocelot) => *ocelot,
        _ => OcelotMeta::default(),
    };
    if let Some(flags) = metadata.get_byte(TAMEABLE_FLAGS) {
        ocelot.sitting = flags & 1 != 0;
        ocelot.tamed = flags & 4 != 0;
    }
    if let Some(skin) = metadata.get_byte(OCELOT_SKIN) {
        ocelot.skin = skin;
    }
    *variant = Variant::Ocelot(ocelot);
}

pub(super) fn apply_rabbit(variant: &mut Variant, metadata: &Metadata) {
    if let Some(kind) = metadata.get_byte(RABBIT_TYPE) {
        *variant = Variant::Rabbit(RabbitMeta { kind });
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{byte, meta};
    use super::super::{EntityMeta, Variant};
    use super::*;
    use crate::entity::kind::EntityKind;

    #[test]
    fn a_sheep_has_a_colour_and_can_be_shorn() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Sheep, &meta(vec![byte(16, 6)]));
        assert_eq!(
            state.variant,
            Variant::Sheep(SheepMeta {
                color: 6,
                sheared: false
            })
        );
        state.apply(EntityKind::Sheep, &meta(vec![byte(16, 16 | 6)]));
        assert!(matches!(state.variant, Variant::Sheep(sheep) if sheep.sheared));
    }

    #[test]
    fn a_wolf_is_tamed_angry_or_sitting_by_flag_bits() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Wolf, &meta(vec![byte(16, 0b111), byte(20, 3)]));
        let Variant::Wolf(wolf) = state.variant else {
            panic!("a wolf variant");
        };
        assert!(wolf.sitting && wolf.angry && wolf.tamed);
        assert_eq!(wolf.collar, 3);
    }

    #[test]
    fn a_wolf_update_keeps_its_collar() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Wolf, &meta(vec![byte(20, 5)]));
        state.apply(EntityKind::Wolf, &meta(vec![byte(16, 4)]));
        let Variant::Wolf(wolf) = state.variant else {
            panic!("a wolf variant");
        };
        assert!(wolf.tamed);
        assert_eq!(wolf.collar, 5);
    }

    #[test]
    fn an_untouched_wolf_has_the_red_collar() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Wolf, &meta(vec![byte(19, 1)]));
        assert!(matches!(state.variant, Variant::Wolf(wolf) if wolf.collar == 14 && wolf.begging));
    }

    #[test]
    fn an_ocelot_has_a_coat_and_a_tame_flag() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Ocelot, &meta(vec![byte(16, 4), byte(18, 2)]));
        assert_eq!(
            state.variant,
            Variant::Ocelot(OcelotMeta {
                sitting: false,
                tamed: true,
                skin: 2
            })
        );
    }

    #[test]
    fn a_rabbit_has_a_type_and_a_pig_a_saddle() {
        let mut rabbit = EntityMeta::default();
        rabbit.apply(EntityKind::Rabbit, &meta(vec![byte(18, 99)]));
        assert_eq!(rabbit.variant, Variant::Rabbit(RabbitMeta { kind: 99 }));
        let mut pig = EntityMeta::default();
        pig.apply(EntityKind::Pig, &meta(vec![byte(16, 1)]));
        assert_eq!(pig.variant, Variant::Pig { saddled: true });
    }
}
