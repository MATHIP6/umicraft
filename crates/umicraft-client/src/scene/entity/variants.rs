// SPDX-License-Identifier: MIT OR Apache-2.0

//! The texture of a mob variant: which coat a wolf, ocelot, rabbit,
//! villager or horse wears (MCP `RenderWolf`, `RenderOcelot`, `RenderRabbit`,
//! `RenderVillager`, `RenderHorse` `getEntityTexture`).

use umicraft_game::entity::meta::{HorseMeta, OcelotMeta, RabbitMeta, WolfMeta};
use umicraft_model::entity::TextureSlot;

/// A tamed wolf wears the tame coat, an angry one the angry coat.
pub(super) fn wolf(wolf: WolfMeta) -> TextureSlot {
    if wolf.tamed {
        TextureSlot::WolfTame
    } else if wolf.angry {
        TextureSlot::WolfAngry
    } else {
        TextureSlot::Wolf
    }
}

/// The ocelot coat: `0` wild, `1` black, `2` red, `3` siamese.
pub(super) fn ocelot(ocelot: OcelotMeta) -> TextureSlot {
    match ocelot.skin {
        1 => TextureSlot::OcelotBlack,
        2 => TextureSlot::OcelotRed,
        3 => TextureSlot::OcelotSiamese,
        _ => TextureSlot::Ocelot,
    }
}

/// The rabbit coat by type; the rabbit named Toast has its own.
pub(super) fn rabbit(rabbit: RabbitMeta, custom_name: Option<&str>) -> TextureSlot {
    if custom_name == Some("Toast") {
        return TextureSlot::RabbitToast;
    }
    match rabbit.kind {
        1 => TextureSlot::RabbitWhite,
        2 => TextureSlot::RabbitBlack,
        3 => TextureSlot::RabbitWhiteSplotched,
        4 => TextureSlot::RabbitGold,
        5 => TextureSlot::RabbitSalt,
        99 => TextureSlot::RabbitCaerbannog,
        _ => TextureSlot::Rabbit,
    }
}

/// The villager profession texture: `0` farmer, `1` librarian, `2` priest,
/// `3` smith, `4` butcher; any other value wears the plain one.
pub(super) fn villager(profession: i32) -> TextureSlot {
    match profession {
        0 => TextureSlot::Villager,
        1 => TextureSlot::VillagerLibrarian,
        2 => TextureSlot::VillagerPriest,
        3 => TextureSlot::VillagerSmith,
        4 => TextureSlot::VillagerButcher,
        _ => TextureSlot::VillagerPlain,
    }
}

/// The horse texture by type; horses (type `0`) get a composed coat.
pub(super) fn horse(horse: HorseMeta) -> TextureSlot {
    match horse.kind {
        1 => TextureSlot::HorseDonkey,
        2 => TextureSlot::HorseMule,
        3 => TextureSlot::HorseZombie,
        4 => TextureSlot::HorseSkeleton,
        // The coat in the low 12 bits, the armour index above it.
        _ => TextureSlot::HorseCoat(
            (horse.variant & 0x0FFF) as u16 | (u16::from(horse.armor & 3) << 12),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tame_wolf_beats_an_angry_one() {
        let wolf_meta = |tamed, angry| WolfMeta {
            tamed,
            angry,
            ..WolfMeta::default()
        };
        assert_eq!(wolf(wolf_meta(false, false)), TextureSlot::Wolf);
        assert_eq!(wolf(wolf_meta(false, true)), TextureSlot::WolfAngry);
        assert_eq!(wolf(wolf_meta(true, true)), TextureSlot::WolfTame);
    }

    #[test]
    fn rabbits_and_ocelots_follow_their_coat_numbers() {
        assert_eq!(
            rabbit(RabbitMeta { kind: 99 }, None),
            TextureSlot::RabbitCaerbannog
        );
        assert_eq!(
            rabbit(RabbitMeta { kind: 0 }, Some("Toast")),
            TextureSlot::RabbitToast
        );
        assert_eq!(rabbit(RabbitMeta { kind: 7 }, None), TextureSlot::Rabbit);
        let cat = |skin| OcelotMeta {
            skin,
            ..OcelotMeta::default()
        };
        assert_eq!(ocelot(cat(3)), TextureSlot::OcelotSiamese);
        assert_eq!(ocelot(cat(0)), TextureSlot::Ocelot);
    }

    #[test]
    fn professions_map_to_their_textures() {
        assert_eq!(villager(0), TextureSlot::Villager);
        assert_eq!(villager(3), TextureSlot::VillagerSmith);
        assert_eq!(villager(9), TextureSlot::VillagerPlain);
    }

    #[test]
    fn horse_types_pick_a_fixed_texture_or_a_coat() {
        let kind = |kind, variant| HorseMeta {
            kind,
            variant,
            ..HorseMeta::default()
        };
        assert_eq!(horse(kind(1, 0)), TextureSlot::HorseDonkey);
        assert_eq!(horse(kind(4, 0)), TextureSlot::HorseSkeleton);
        assert_eq!(horse(kind(0, 0x0203)), TextureSlot::HorseCoat(0x0203));
    }

    #[test]
    fn the_horse_armour_goes_above_the_coat_bits() {
        let horse = |armor| HorseMeta {
            kind: 0,
            variant: 0x0102,
            armor,
            flags: 0,
        };
        assert_eq!(super::horse(horse(0)), TextureSlot::HorseCoat(0x0102));
        assert_eq!(super::horse(horse(3)), TextureSlot::HorseCoat(0x3102));
        // A donkey ignores the coat and the armour.
        let donkey = HorseMeta {
            kind: 1,
            ..horse(3)
        };
        assert_eq!(super::horse(donkey), TextureSlot::HorseDonkey);
    }
}
