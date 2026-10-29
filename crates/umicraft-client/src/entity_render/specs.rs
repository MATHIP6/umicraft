// SPDX-License-Identifier: MIT OR Apache-2.0

//! The table of entity textures: the jar path of each [`TextureSlot`] and
//! the procedural fallback used when the official jar is absent.
//!
//! Adding a new mob texture = one new entry in [`texture_specs`].

use umicraft_assets::skin::{
    procedural_bat_texture, procedural_blaze_texture, procedural_cave_spider_texture,
    procedural_chicken_texture, procedural_cow_texture, procedural_creeper_texture,
    procedural_dragon_texture, procedural_enderman_texture, procedural_endermite_texture,
    procedural_fallback_skin, procedural_ghast_texture, procedural_guardian_texture,
    procedural_horse_texture, procedural_iron_golem_texture, procedural_magma_cube_texture,
    procedural_mooshroom_texture, procedural_ocelot_texture, procedural_pig_texture,
    procedural_rabbit_texture, procedural_sheep_fur_texture, procedural_sheep_texture,
    procedural_silverfish_texture, procedural_skeleton_texture, procedural_slime_texture,
    procedural_snowman_texture, procedural_spider_texture, procedural_squid_texture,
    procedural_villager_texture, procedural_witch_texture, procedural_wither_texture,
    procedural_wolf_texture, procedural_zombie_pigman_texture, procedural_zombie_skin,
    procedural_zombie_villager_texture, SkinModel,
};
use umicraft_model::entity::TextureSlot;

/// Specification for one entity texture: jar path + procedural fallback.
pub(super) struct TextureSpec {
    pub(super) slot: TextureSlot,
    pub(super) jar_path: &'static str,
    pub(super) fallback: fn() -> (Vec<u8>, u32, u32),
}

/// Central texture registry. Adding a new mob texture = one new entry here.
pub(super) fn texture_specs() -> Vec<TextureSpec> {
    vec![
        TextureSpec {
            slot: TextureSlot::Steve,
            jar_path: "assets/minecraft/textures/entity/steve.png",
            fallback: || (procedural_fallback_skin(SkinModel::Classic), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Alex,
            jar_path: "assets/minecraft/textures/entity/alex.png",
            fallback: || (procedural_fallback_skin(SkinModel::Slim), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Zombie,
            jar_path: "assets/minecraft/textures/entity/zombie/zombie.png",
            fallback: || (procedural_zombie_skin(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Skeleton,
            jar_path: "assets/minecraft/textures/entity/skeleton/skeleton.png",
            fallback: || (procedural_skeleton_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Creeper,
            jar_path: "assets/minecraft/textures/entity/creeper/creeper.png",
            fallback: || (procedural_creeper_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Cow,
            jar_path: "assets/minecraft/textures/entity/cow/cow.png",
            fallback: || (procedural_cow_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Pig,
            jar_path: "assets/minecraft/textures/entity/pig/pig.png",
            fallback: || (procedural_pig_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Sheep,
            jar_path: "assets/minecraft/textures/entity/sheep/sheep.png",
            fallback: || (procedural_sheep_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::SheepWool,
            jar_path: "assets/minecraft/textures/entity/sheep/sheep_fur.png",
            fallback: || (procedural_sheep_fur_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Chicken,
            jar_path: "assets/minecraft/textures/entity/chicken.png",
            fallback: || (procedural_chicken_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Spider,
            jar_path: "assets/minecraft/textures/entity/spider/spider.png",
            fallback: || (procedural_spider_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::CaveSpider,
            jar_path: "assets/minecraft/textures/entity/spider/cave_spider.png",
            fallback: || (procedural_cave_spider_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Enderman,
            jar_path: "assets/minecraft/textures/entity/enderman/enderman.png",
            fallback: || (procedural_enderman_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Slime,
            jar_path: "assets/minecraft/textures/entity/slime/slime.png",
            fallback: || (procedural_slime_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::MagmaCube,
            jar_path: "assets/minecraft/textures/entity/slime/magmacube.png",
            fallback: || (procedural_magma_cube_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::ZombiePigman,
            jar_path: "assets/minecraft/textures/entity/zombie_pigman.png",
            fallback: || (procedural_zombie_pigman_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Villager,
            jar_path: "assets/minecraft/textures/entity/villager/farmer.png",
            fallback: || (procedural_villager_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::ZombieVillager,
            jar_path: "assets/minecraft/textures/entity/zombie/zombie_villager.png",
            fallback: || (procedural_zombie_villager_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Witch,
            jar_path: "assets/minecraft/textures/entity/witch.png",
            fallback: || (procedural_witch_texture(), 64, 128),
        },
        TextureSpec {
            slot: TextureSlot::IronGolem,
            jar_path: "assets/minecraft/textures/entity/iron_golem.png",
            fallback: || (procedural_iron_golem_texture(), 128, 128),
        },
        TextureSpec {
            slot: TextureSlot::Snowman,
            jar_path: "assets/minecraft/textures/entity/snowman.png",
            fallback: || (procedural_snowman_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Wolf,
            jar_path: "assets/minecraft/textures/entity/wolf/wolf.png",
            fallback: || (procedural_wolf_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Mooshroom,
            jar_path: "assets/minecraft/textures/entity/cow/mooshroom.png",
            fallback: || (procedural_mooshroom_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Squid,
            jar_path: "assets/minecraft/textures/entity/squid.png",
            fallback: || (procedural_squid_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Bat,
            jar_path: "assets/minecraft/textures/entity/bat.png",
            fallback: || (procedural_bat_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Blaze,
            jar_path: "assets/minecraft/textures/entity/blaze.png",
            fallback: || (procedural_blaze_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Dragon,
            jar_path: "assets/minecraft/textures/entity/enderdragon/dragon.png",
            fallback: || (procedural_dragon_texture(), 256, 256),
        },
        TextureSpec {
            slot: TextureSlot::Endermite,
            jar_path: "assets/minecraft/textures/entity/endermite.png",
            fallback: || (procedural_endermite_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Ghast,
            jar_path: "assets/minecraft/textures/entity/ghast/ghast.png",
            fallback: || (procedural_ghast_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Guardian,
            jar_path: "assets/minecraft/textures/entity/guardian.png",
            fallback: || (procedural_guardian_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Horse,
            jar_path: "assets/minecraft/textures/entity/horse/horse_white.png",
            fallback: || (procedural_horse_texture(), 128, 128),
        },
        TextureSpec {
            slot: TextureSlot::Ocelot,
            jar_path: "assets/minecraft/textures/entity/cat/ocelot.png",
            fallback: || (procedural_ocelot_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Rabbit,
            jar_path: "assets/minecraft/textures/entity/rabbit/brown.png",
            fallback: || (procedural_rabbit_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Silverfish,
            jar_path: "assets/minecraft/textures/entity/silverfish.png",
            fallback: || (procedural_silverfish_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Wither,
            jar_path: "assets/minecraft/textures/entity/wither/wither.png",
            fallback: || (procedural_wither_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::WolfTame,
            jar_path: "assets/minecraft/textures/entity/wolf/wolf_tame.png",
            fallback: || (procedural_wolf_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::WolfAngry,
            jar_path: "assets/minecraft/textures/entity/wolf/wolf_angry.png",
            fallback: || (procedural_wolf_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::WolfCollar,
            jar_path: "assets/minecraft/textures/entity/wolf/wolf_collar.png",
            fallback: || (procedural_wolf_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::OcelotBlack,
            jar_path: "assets/minecraft/textures/entity/cat/black.png",
            fallback: || (procedural_ocelot_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::OcelotRed,
            jar_path: "assets/minecraft/textures/entity/cat/red.png",
            fallback: || (procedural_ocelot_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::OcelotSiamese,
            jar_path: "assets/minecraft/textures/entity/cat/siamese.png",
            fallback: || (procedural_ocelot_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::RabbitWhite,
            jar_path: "assets/minecraft/textures/entity/rabbit/white.png",
            fallback: || (procedural_rabbit_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::RabbitBlack,
            jar_path: "assets/minecraft/textures/entity/rabbit/black.png",
            fallback: || (procedural_rabbit_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::RabbitGold,
            jar_path: "assets/minecraft/textures/entity/rabbit/gold.png",
            fallback: || (procedural_rabbit_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::RabbitSalt,
            jar_path: "assets/minecraft/textures/entity/rabbit/salt.png",
            fallback: || (procedural_rabbit_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::RabbitWhiteSplotched,
            jar_path: "assets/minecraft/textures/entity/rabbit/white_splotched.png",
            fallback: || (procedural_rabbit_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::RabbitToast,
            jar_path: "assets/minecraft/textures/entity/rabbit/toast.png",
            fallback: || (procedural_rabbit_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::RabbitCaerbannog,
            jar_path: "assets/minecraft/textures/entity/rabbit/caerbannog.png",
            fallback: || (procedural_rabbit_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::VillagerPlain,
            jar_path: "assets/minecraft/textures/entity/villager/villager.png",
            fallback: || (procedural_villager_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::VillagerLibrarian,
            jar_path: "assets/minecraft/textures/entity/villager/librarian.png",
            fallback: || (procedural_villager_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::VillagerPriest,
            jar_path: "assets/minecraft/textures/entity/villager/priest.png",
            fallback: || (procedural_villager_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::VillagerSmith,
            jar_path: "assets/minecraft/textures/entity/villager/smith.png",
            fallback: || (procedural_villager_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::VillagerButcher,
            jar_path: "assets/minecraft/textures/entity/villager/butcher.png",
            fallback: || (procedural_villager_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::SkeletonWither,
            jar_path: "assets/minecraft/textures/entity/skeleton/wither_skeleton.png",
            fallback: || (procedural_skeleton_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::HorseDonkey,
            jar_path: "assets/minecraft/textures/entity/horse/donkey.png",
            fallback: || (procedural_horse_texture(), 128, 128),
        },
        TextureSpec {
            slot: TextureSlot::HorseMule,
            jar_path: "assets/minecraft/textures/entity/horse/mule.png",
            fallback: || (procedural_horse_texture(), 128, 128),
        },
        TextureSpec {
            slot: TextureSlot::HorseZombie,
            jar_path: "assets/minecraft/textures/entity/horse/horse_zombie.png",
            fallback: || (procedural_horse_texture(), 128, 128),
        },
        TextureSpec {
            slot: TextureSlot::HorseSkeleton,
            jar_path: "assets/minecraft/textures/entity/horse/horse_skeleton.png",
            fallback: || (procedural_horse_texture(), 128, 128),
        },
    ]
}
