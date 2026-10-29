// SPDX-License-Identifier: MIT OR Apache-2.0

//! Horse entries (MCP `EntityHorse.entityInit`).

use umicraft_protocol::packets::play::Metadata;

use super::Variant;

/// Entry of the status flags.
const FLAGS: u8 = 16;
/// Entry of the type.
const TYPE: u8 = 19;
/// Entry of the coat variant.
const VARIANT: u8 = 20;
/// Entry of the armour index.
const ARMOR: u8 = 22;

/// Horse state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HorseMeta {
    /// `0` horse, `1` donkey, `2` mule, `3` zombie horse, `4` skeleton horse.
    pub kind: u8,
    /// Coat: base colour in the low byte (`0..=6`), marking in the second
    /// (`0..=4`).
    pub variant: i32,
    /// Armour: `0` none, `1` iron, `2` gold, `3` diamond.
    pub armor: u8,
    /// Status flags: `2` tame, `4` saddled, `8` chest, `16` bred, `32`
    /// eating, `64` rearing, `128` mouth open.
    pub flags: i32,
}

impl HorseMeta {
    /// True when the horse wears a saddle.
    #[must_use]
    pub const fn saddled(self) -> bool {
        self.flags & 4 != 0
    }

    /// True when the horse carries a chest (donkeys and mules).
    #[must_use]
    pub const fn chested(self) -> bool {
        self.flags & 8 != 0
    }

    /// True while the horse rears.
    #[must_use]
    pub const fn rearing(self) -> bool {
        self.flags & 64 != 0
    }

    /// True while the horse grazes.
    #[must_use]
    pub const fn eating(self) -> bool {
        self.flags & 32 != 0
    }

    /// True while the horse opens its mouth.
    #[must_use]
    pub const fn mouth_open(self) -> bool {
        self.flags & 128 != 0
    }
}

pub(super) fn apply(variant: &mut Variant, metadata: &Metadata) {
    let mut horse = match variant {
        Variant::Horse(horse) => *horse,
        _ => HorseMeta::default(),
    };
    if let Some(flags) = metadata.get_int(FLAGS) {
        horse.flags = flags;
    }
    if let Some(kind) = metadata.get_byte(TYPE) {
        horse.kind = kind;
    }
    if let Some(coat) = metadata.get_int(VARIANT) {
        horse.variant = coat;
    }
    if let Some(armor) = metadata.get_int(ARMOR) {
        horse.armor = u8::try_from(armor.clamp(0, 3)).unwrap_or(0);
    }
    *variant = Variant::Horse(horse);
}

#[cfg(test)]
mod tests {
    use super::super::tests::{byte, int, meta};
    use super::super::EntityMeta;
    use super::*;
    use crate::entity::kind::EntityKind;

    #[test]
    fn a_horse_has_a_type_a_coat_and_armour() {
        let mut state = EntityMeta::default();
        state.apply(
            EntityKind::Horse,
            &meta(vec![
                byte(19, 1),
                int(20, 0x0102),
                int(22, 3),
                int(16, 4 | 64),
            ]),
        );
        let Variant::Horse(horse) = state.variant else {
            panic!("a horse variant");
        };
        assert_eq!((horse.kind, horse.variant, horse.armor), (1, 0x0102, 3));
        assert!(horse.saddled() && horse.rearing());
        assert!(!horse.chested() && !horse.eating() && !horse.mouth_open());
    }

    #[test]
    fn an_unknown_armour_index_is_clamped() {
        let mut state = EntityMeta::default();
        state.apply(EntityKind::Horse, &meta(vec![int(22, 99)]));
        assert!(matches!(state.variant, Variant::Horse(horse) if horse.armor == 3));
    }
}
