// SPDX-License-Identifier: MIT OR Apache-2.0

//! The packets of a click on an entity.
//!
//! A left click is `Minecraft.clickMouse`: the arm swings, then
//! `PlayerControllerMP.attackEntity` sends `Use Entity` (attack). A right
//! click is `Minecraft.rightClickMouse`: `func_178894_a` sends the
//! interact-at packet with the click point relative to the entity, and when
//! the client does not consume it `interactWithEntitySendPacket` sends the
//! plain interact packet. A click nobody consumed falls through to the use of
//! the held item, which the caller handles.

use super::target::EntityHit;
use crate::entity::meta::{Variant, WolfMeta};
use crate::entity::{EntityId, EntityKind, EntityRecord};
use crate::gamemode::GameMode;
use crate::interaction::Outgoing;
use crate::item_nbt::ItemDisplay;
use umicraft_protocol::packets::play::ItemStack;

/// The item id of a spawn egg, which a villager does not trade through.
const SPAWN_EGG: i16 = 383;

/// The packets of a left click on an entity (MCP `Minecraft.clickMouse` and
/// `PlayerControllerMP.attackEntity`): the swing first, then the attack.
#[must_use]
pub fn attack_packets(entity: EntityId) -> [Outgoing; 2] {
    [Outgoing::Swing, Outgoing::Attack { entity: entity.0 }]
}

/// What the right click on an entity reads.
pub struct InteractTick<'a> {
    /// The game mode of the player.
    pub mode: GameMode,
    /// The entity under the crosshair.
    pub entity: &'a EntityRecord,
    /// Where the ray entered it.
    pub hit: EntityHit,
    /// The held item.
    pub held: Option<&'a ItemStack>,
    /// Who clicks.
    pub viewer: ClickViewer,
}

/// The player who clicks, for what depends on who owns or holds what.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickViewer {
    /// The entity id of the player.
    pub id: EntityId,
    /// The UUID of the player, when known.
    pub uuid: Option<u128>,
}

impl Default for ClickViewer {
    fn default() -> Self {
        Self {
            id: EntityId(0),
            uuid: None,
        }
    }
}

/// The result of a right click on an entity.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityClick {
    /// The packets to send, in order.
    pub packets: Vec<Outgoing>,
    /// True when the client consumed the click: the held item is not used.
    pub consumed: bool,
}

/// The packets of a right click on an entity and whether it was consumed.
///
/// The interact-at packet is always sent. A spectator consumes nothing
/// locally (`currentGameType != SPECTATOR`).
#[must_use]
pub fn interact_packets(tick: &InteractTick<'_>) -> EntityClick {
    let id = tick.entity.id.0;
    let offset = tick.hit.offset();
    let mut packets = vec![Outgoing::InteractAt {
        entity: id,
        offset: [offset.x as f32, offset.y as f32, offset.z as f32],
    }];
    let local = tick.mode != GameMode::Spectator;
    let consumed = if local && consumes_interact_at(tick.entity) {
        true
    } else {
        packets.push(Outgoing::Interact { entity: id });
        local && consumes_interact(tick)
    };
    EntityClick { packets, consumed }
}

/// Whether `Entity.interactAt` answers true on the client
/// (MCP `EntityArmorStand.interactAt`: everything but a marker).
fn consumes_interact_at(entity: &EntityRecord) -> bool {
    match (entity.kind, &entity.meta.variant) {
        (EntityKind::ArmorStand, Variant::ArmorStand(stand)) => stand.flags & 16 == 0,
        (EntityKind::ArmorStand, _) => true,
        _ => false,
    }
}

/// The held items that matter to a click (MCP `Items` ids).
const BOWL: i16 = 281;
const WHEAT_SEEDS: i16 = 295;
const WHEAT: i16 = 296;
const BUCKET: i16 = 325;
const SADDLE: i16 = 329;
const DYE: i16 = 351;
const BONE: i16 = 352;
const SUGAR: i16 = 353;
const FISH: i16 = 349;
const SHEARS: i16 = 359;
const CARROT: i16 = 391;
const GOLDEN_CARROT: i16 = 396;
const LEAD: i16 = 420;
const NAME_TAG: i16 = 421;
const APPLE: i16 = 260;
const GOLDEN_APPLE: i16 = 322;
/// The dandelion block (`yellow_flower`), a rabbit's food.
const DANDELION: i16 = 37;
/// The hay block, food of a horse.
const HAY_BLOCK: i16 = 170;
/// The meats a wolf likes (`ItemFood.isWolfsFavoriteMeat`): pork, beef,
/// chicken, rotten flesh, rabbit and mutton, raw and cooked.
const WOLF_MEATS: [i16; 11] = [319, 320, 363, 364, 365, 366, 367, 411, 412, 423, 424];

/// Minecart types of the data of `Spawn Object` (MCP
/// `EntityMinecart.EnumMinecartType.getNetworkID`).
const CART_RIDEABLE: i32 = 0;
const CART_CHEST: i32 = 1;
const CART_FURNACE: i32 = 2;
const CART_HOPPER: i32 = 5;

/// Whether the click is consumed by the client, so that the held item is not
/// used: `Entity.interactFirst`, then `Item.itemInteractionForEntity`
/// (MCP `EntityPlayer.interactWith`).
///
/// Everything here answers from state the client has. What it does not see is
/// guessed the way that is right most of the time: an adult animal is ready
/// to breed (the cooldown is not sent), a villager is not trading, a cart is
/// free. A zombie villager cured with a golden apple needs its weakness
/// effect, which is not tracked: it is not consumed.
fn consumes_interact(tick: &InteractTick<'_>) -> bool {
    let entity = tick.entity;
    let item = tick.held.map(|stack| stack.item_id);
    if is_entity_living(entity.kind) && consumes_living(tick, item) {
        return true;
    }
    if consumes_kind(entity, tick.held, tick.mode) {
        return true;
    }
    item_interaction(entity, tick.held)
}

/// `EntityLiving` (a mob): not a player, an armour stand or an object.
fn is_entity_living(kind: EntityKind) -> bool {
    kind.is_living() && !matches!(kind, EntityKind::Player | EntityKind::ArmorStand)
}

/// The leash part of `EntityLiving.interactFirst`.
fn consumes_living(tick: &InteractTick<'_>, item: Option<i16>) -> bool {
    let entity = tick.entity;
    // Taking the leash off by clicking its holder.
    if entity.leash == Some(tick.viewer.id) {
        return true;
    }
    if item != Some(LEAD) || !allows_leashing(entity) {
        return false;
    }
    match owner_of(entity) {
        Some((true, owner)) => owner.is_some() && owner == tick.viewer.uuid,
        _ => true,
    }
}

/// `(tamed, owner)` of a wolf or an ocelot, `None` for the other mobs.
fn owner_of(entity: &EntityRecord) -> Option<(bool, Option<u128>)> {
    match entity.meta.variant {
        Variant::Wolf(wolf) => Some((wolf.tamed, wolf.owner)),
        Variant::Ocelot(ocelot) => Some((ocelot.tamed, ocelot.owner)),
        _ => None,
    }
}

/// `EntityLiving.allowLeashing` and its overrides.
fn allows_leashing(entity: &EntityRecord) -> bool {
    if entity.leash.is_some() || is_monster(entity.kind) {
        return false;
    }
    match (entity.kind, &entity.meta.variant) {
        (EntityKind::Bat | EntityKind::Villager, _) => false,
        (EntityKind::Horse, Variant::Horse(horse)) => !matches!(horse.kind, 3 | 4),
        (EntityKind::Wolf, Variant::Wolf(wolf)) => !wolf.angry,
        _ => true,
    }
}

/// The mobs that implement `IMob`.
fn is_monster(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Zombie
            | EntityKind::Skeleton
            | EntityKind::Creeper
            | EntityKind::Spider
            | EntityKind::CaveSpider
            | EntityKind::Enderman
            | EntityKind::Slime
            | EntityKind::MagmaCube
            | EntityKind::ZombiePigman
            | EntityKind::Giant
            | EntityKind::ZombieVillager
            | EntityKind::Witch
            | EntityKind::Blaze
            | EntityKind::Dragon
            | EntityKind::Endermite
            | EntityKind::Ghast
            | EntityKind::Guardian
            | EntityKind::Silverfish
            | EntityKind::Wither
    )
}

/// The mobs that extend `EntityAgeable`, whose `interact` takes a spawn egg.
fn is_ageable(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Cow
            | EntityKind::Pig
            | EntityKind::Sheep
            | EntityKind::Chicken
            | EntityKind::Mooshroom
            | EntityKind::Wolf
            | EntityKind::Ocelot
            | EntityKind::Rabbit
            | EntityKind::Horse
            | EntityKind::Villager
    )
}

/// `Entity.interactFirst` of the entity itself (its `interact` for a mob).
fn consumes_kind(entity: &EntityRecord, held: Option<&ItemStack>, mode: GameMode) -> bool {
    let item = held.map(|stack| stack.item_id);
    let adult = !entity.meta.baby;
    // `EntityAgeable.interact`: a spawn egg is always taken.
    if item == Some(SPAWN_EGG) && is_ageable(entity.kind) {
        return true;
    }
    match entity.kind {
        // Item frames, boats and leash knots answer true.
        EntityKind::ItemFrame | EntityKind::Boat | EntityKind::LeashKnot => true,
        EntityKind::Minecart => matches!(
            entity.object_data,
            CART_RIDEABLE | CART_CHEST | CART_FURNACE | CART_HOPPER
        ),
        EntityKind::Horse => horse_answer(entity, item),
        EntityKind::Villager => adult,
        EntityKind::Cow => cow_answer(item, adult, mode, false),
        EntityKind::Mooshroom => cow_answer(item, adult, mode, true),
        EntityKind::Sheep => item == Some(WHEAT),
        EntityKind::Pig => item == Some(CARROT),
        EntityKind::Chicken => item == Some(WHEAT_SEEDS),
        EntityKind::Rabbit => matches!(item, Some(CARROT | GOLDEN_CARROT | DANDELION)),
        EntityKind::Wolf => wolf_answer(entity, item, held),
        EntityKind::Ocelot => item == Some(FISH),
        _ => false,
    }
}

/// `EntityCow.interact` and, with `mushroom`, `EntityMooshroom.interact`.
fn cow_answer(item: Option<i16>, adult: bool, mode: GameMode, mushroom: bool) -> bool {
    match item {
        // Survival fills the bucket; a calf gives nothing.
        Some(BUCKET) => adult && mode != GameMode::Creative,
        Some(BOWL | SHEARS) if mushroom => adult,
        Some(WHEAT) => true,
        _ => false,
    }
}

/// `EntityWolf.interact`: a tamed wolf takes meat (to eat or to breed) and a
/// new collar colour, a wild one takes a bone (unless angry) or meat.
fn wolf_answer(entity: &EntityRecord, item: Option<i16>, held: Option<&ItemStack>) -> bool {
    // A wolf whose metadata has not arrived yet is a wild one.
    let wolf = match entity.meta.variant {
        Variant::Wolf(wolf) => wolf,
        _ => WolfMeta::default(),
    };
    let Some(item) = item else { return false };
    if WOLF_MEATS.contains(&item) {
        return true;
    }
    if wolf.tamed {
        // The collar byte is the damage of the dye (`setCollarColor` stores
        // `getDyeDamage`), so a dye of the same damage changes nothing.
        item == DYE && held.is_some_and(|dye| (dye.damage & 15) as u8 != wolf.collar)
    } else {
        item == BONE && !wolf.angry
    }
}

/// `EntityHorse.interact`, as far as the client can follow it: a spawn egg
/// is taken above; a wild undead horse refuses; a wild horse rears for any
/// item; a tamed foal takes only its food; every adult is mounted.
fn horse_answer(entity: &EntityRecord, item: Option<i16>) -> bool {
    let Variant::Horse(horse) = entity.meta.variant else {
        return true;
    };
    let tame = horse.flags & 2 != 0;
    if !tame && matches!(horse.kind, 3 | 4) {
        return false;
    }
    if !entity.meta.baby || !tame {
        return true;
    }
    matches!(
        item,
        Some(WHEAT | SUGAR | HAY_BLOCK | APPLE | GOLDEN_CARROT | GOLDEN_APPLE)
    )
}

/// `Item.itemInteractionForEntity` of the held item, reached when the entity
/// itself did not take the click and is living: a name tag with a name on a
/// mob, a saddle on a pig, a dye on a sheep.
fn item_interaction(entity: &EntityRecord, held: Option<&ItemStack>) -> bool {
    let Some(stack) = held else { return false };
    match stack.item_id {
        NAME_TAG => {
            is_entity_living(entity.kind)
                && stack
                    .nbt
                    .as_deref()
                    .is_some_and(|nbt| ItemDisplay::from_nbt(nbt).custom_name.is_some())
        }
        SADDLE => entity.kind == EntityKind::Pig,
        DYE => entity.kind == EntityKind::Sheep,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::meta::ArmorStandMeta;
    use glam::DVec3;

    fn record(kind: EntityKind) -> EntityRecord {
        EntityRecord::new(EntityId(5), kind, DVec3::new(1.0, 64.0, 1.0), 0.0, 0.0)
    }

    fn click(entity: &EntityRecord, mode: GameMode) -> EntityClick {
        interact_packets(&InteractTick {
            mode,
            entity,
            hit: EntityHit {
                id: entity.id,
                point: DVec3::new(1.25, 64.5, 0.75),
                origin: entity.position,
            },
            held: None,
            viewer: ClickViewer::default(),
        })
    }

    #[test]
    fn an_attack_swings_before_it_hits() {
        assert_eq!(
            attack_packets(EntityId(9)),
            [Outgoing::Swing, Outgoing::Attack { entity: 9 }]
        );
    }

    #[test]
    fn interact_at_carries_the_point_relative_to_the_entity() {
        let result = click(&record(EntityKind::Zombie), GameMode::Survival);
        assert_eq!(
            result.packets[0],
            Outgoing::InteractAt {
                entity: 5,
                offset: [0.25, 0.5, -0.25]
            }
        );
    }

    #[test]
    fn a_mob_the_client_does_not_consume_gets_interact_and_the_item_use() {
        let result = click(&record(EntityKind::Zombie), GameMode::Survival);
        assert_eq!(result.packets.len(), 2);
        assert_eq!(result.packets[1], Outgoing::Interact { entity: 5 });
        assert!(!result.consumed);
    }

    #[test]
    fn an_armour_stand_is_consumed_by_interact_at_alone() {
        let result = click(&record(EntityKind::ArmorStand), GameMode::Survival);
        assert_eq!(result.packets.len(), 1);
        assert!(result.consumed);
    }

    #[test]
    fn a_marker_armour_stand_is_not_consumed() {
        let mut stand = record(EntityKind::ArmorStand);
        stand.meta.variant = Variant::ArmorStand(ArmorStandMeta {
            flags: 16,
            ..Default::default()
        });
        let result = click(&stand, GameMode::Survival);
        assert_eq!(result.packets.len(), 2);
        assert!(!result.consumed);
    }

    #[test]
    fn an_item_frame_and_an_adult_villager_are_consumed_after_interact() {
        for kind in [EntityKind::ItemFrame, EntityKind::Villager] {
            let result = click(&record(kind), GameMode::Survival);
            assert_eq!(result.packets.len(), 2);
            assert!(result.consumed, "{kind:?}");
        }
    }

    #[test]
    fn a_baby_villager_is_not_consumed() {
        let mut villager = record(EntityKind::Villager);
        villager.meta.baby = true;
        assert!(!click(&villager, GameMode::Survival).consumed);
    }

    fn holding(kind: EntityKind, item_id: i16, mode: GameMode, baby: bool) -> bool {
        let mut entity = record(kind);
        entity.meta.baby = baby;
        let stack = ItemStack {
            item_id,
            stack_size: 1,
            damage: 0,
            nbt: None,
        };
        interact_packets(&InteractTick {
            mode,
            entity: &entity,
            hit: EntityHit {
                id: entity.id,
                point: DVec3::new(1.25, 64.5, 0.75),
                origin: entity.position,
            },
            held: Some(&stack),
            viewer: ClickViewer::default(),
        })
        .consumed
    }

    #[test]
    fn a_bucket_fills_on_an_adult_cow_in_survival_only() {
        assert!(holding(EntityKind::Cow, BUCKET, GameMode::Survival, false));
        assert!(!holding(EntityKind::Cow, BUCKET, GameMode::Creative, false));
        assert!(!holding(EntityKind::Cow, BUCKET, GameMode::Survival, true));
        assert!(holding(
            EntityKind::Mooshroom,
            BUCKET,
            GameMode::Survival,
            false
        ));
    }

    #[test]
    fn a_mushroom_cow_takes_a_bowl_and_shears_but_a_cow_does_not() {
        for item in [BOWL, SHEARS] {
            assert!(holding(
                EntityKind::Mooshroom,
                item,
                GameMode::Creative,
                false
            ));
            assert!(!holding(
                EntityKind::Mooshroom,
                item,
                GameMode::Creative,
                true
            ));
            assert!(!holding(EntityKind::Cow, item, GameMode::Survival, false));
        }
    }

    #[test]
    fn each_animal_takes_its_own_food_and_calves_too() {
        let food = [
            (EntityKind::Cow, WHEAT),
            (EntityKind::Mooshroom, WHEAT),
            (EntityKind::Sheep, WHEAT),
            (EntityKind::Pig, CARROT),
            (EntityKind::Chicken, WHEAT_SEEDS),
            (EntityKind::Rabbit, GOLDEN_CARROT),
            (EntityKind::Rabbit, DANDELION),
        ];
        for (kind, item) in food {
            assert!(holding(kind, item, GameMode::Survival, false), "{kind:?}");
            assert!(holding(kind, item, GameMode::Survival, true), "{kind:?}");
        }
        // Not the food of a pig, and nothing for a zombie.
        assert!(!holding(EntityKind::Pig, WHEAT, GameMode::Survival, false));
        assert!(!holding(
            EntityKind::Zombie,
            WHEAT,
            GameMode::Survival,
            false
        ));
    }

    #[test]
    fn a_saddle_on_a_pig_and_a_dye_on_a_sheep_are_consumed() {
        assert!(holding(EntityKind::Pig, SADDLE, GameMode::Survival, true));
        assert!(holding(EntityKind::Sheep, DYE, GameMode::Creative, false));
        // Shears on a sheep fall through: the reference answers false.
        assert!(!holding(
            EntityKind::Sheep,
            SHEARS,
            GameMode::Survival,
            false
        ));
    }

    fn stack(item_id: i16, damage: i16, nbt: Option<Vec<u8>>) -> ItemStack {
        ItemStack {
            item_id,
            stack_size: 1,
            damage,
            nbt,
        }
    }

    fn click_as(entity: &EntityRecord, held: Option<&ItemStack>, viewer: ClickViewer) -> bool {
        interact_packets(&InteractTick {
            mode: GameMode::Survival,
            entity,
            hit: EntityHit {
                id: entity.id,
                point: DVec3::new(1.25, 64.5, 0.75),
                origin: entity.position,
            },
            held,
            viewer,
        })
        .consumed
    }

    fn named_tag() -> ItemStack {
        // {display: {Name: "Bob"}} as the bytes of an NBT compound.
        let mut nbt = vec![10, 0, 0];
        nbt.extend([10, 0, 7]);
        nbt.extend(b"display");
        nbt.extend([8, 0, 4]);
        nbt.extend(b"Name");
        nbt.extend([0, 3]);
        nbt.extend(b"Bob");
        nbt.extend([0, 0]);
        stack(NAME_TAG, 0, Some(nbt))
    }

    #[test]
    fn a_lead_ties_any_tame_or_wild_animal_but_not_a_monster_or_a_bat() {
        let lead = stack(LEAD, 0, None);
        let viewer = ClickViewer::default();
        for kind in [EntityKind::Cow, EntityKind::Pig, EntityKind::Squid] {
            assert!(click_as(&record(kind), Some(&lead), viewer), "{kind:?}");
        }
        for kind in [EntityKind::Zombie, EntityKind::Bat] {
            assert!(!click_as(&record(kind), Some(&lead), viewer), "{kind:?}");
        }
        // Already tied: no second leash.
        let mut tied = record(EntityKind::Cow);
        tied.leash = Some(EntityId(77));
        assert!(!click_as(&tied, Some(&lead), viewer));
    }

    #[test]
    fn clicking_the_animal_you_lead_unties_it() {
        let mut cow = record(EntityKind::Cow);
        cow.leash = Some(EntityId(3));
        let me = ClickViewer {
            id: EntityId(3),
            uuid: None,
        };
        assert!(click_as(&cow, None, me));
        assert!(!click_as(&cow, None, ClickViewer::default()));
    }

    #[test]
    fn a_tamed_pet_takes_the_lead_only_from_its_owner() {
        let owner = 0x1234_u128;
        let mut wolf = record(EntityKind::Wolf);
        wolf.meta.variant = Variant::Wolf(crate::entity::meta::WolfMeta {
            tamed: true,
            owner: Some(owner),
            ..Default::default()
        });
        let lead = stack(LEAD, 0, None);
        let mine = ClickViewer {
            id: EntityId(1),
            uuid: Some(owner),
        };
        let other = ClickViewer {
            id: EntityId(1),
            uuid: Some(owner + 1),
        };
        assert!(click_as(&wolf, Some(&lead), mine));
        assert!(!click_as(&wolf, Some(&lead), other));
        // An angry wild wolf cannot be tied.
        wolf.meta.variant = Variant::Wolf(crate::entity::meta::WolfMeta {
            angry: true,
            ..Default::default()
        });
        assert!(!click_as(&wolf, Some(&lead), mine));
    }

    #[test]
    fn a_name_tag_needs_a_name_and_a_mob() {
        let viewer = ClickViewer::default();
        let plain = stack(NAME_TAG, 0, None);
        assert!(!click_as(&record(EntityKind::Zombie), Some(&plain), viewer));
        let named = named_tag();
        assert!(click_as(&record(EntityKind::Zombie), Some(&named), viewer));
        assert!(click_as(&record(EntityKind::Cow), Some(&named), viewer));
        assert!(!click_as(&record(EntityKind::Player), Some(&named), viewer));
    }

    #[test]
    fn a_spawn_egg_is_taken_by_every_ageable_mob_baby_villager_included() {
        let egg = stack(SPAWN_EGG, 90, None);
        let viewer = ClickViewer::default();
        for kind in [
            EntityKind::Cow,
            EntityKind::Wolf,
            EntityKind::Horse,
            EntityKind::Villager,
        ] {
            assert!(click_as(&record(kind), Some(&egg), viewer), "{kind:?}");
        }
        let mut child = record(EntityKind::Villager);
        child.meta.baby = true;
        assert!(click_as(&child, Some(&egg), viewer));
        assert!(!click_as(&record(EntityKind::Zombie), Some(&egg), viewer));
    }

    #[test]
    fn a_wolf_takes_meat_and_bones_and_a_new_collar() {
        let viewer = ClickViewer::default();
        let mut wild = record(EntityKind::Wolf);
        assert!(click_as(&wild, Some(&stack(BONE, 0, None)), viewer));
        assert!(click_as(&wild, Some(&stack(319, 0, None)), viewer));
        wild.meta.variant = Variant::Wolf(crate::entity::meta::WolfMeta {
            angry: true,
            ..Default::default()
        });
        assert!(!click_as(&wild, Some(&stack(BONE, 0, None)), viewer));
        // A tamed wolf with the red collar (dye damage 14).
        let mut tame = record(EntityKind::Wolf);
        tame.meta.variant = Variant::Wolf(crate::entity::meta::WolfMeta {
            tamed: true,
            ..Default::default()
        });
        assert!(click_as(&tame, Some(&stack(DYE, 4, None)), viewer));
        assert!(!click_as(&tame, Some(&stack(DYE, 14, None)), viewer));
        assert!(!click_as(&tame, Some(&stack(BONE, 0, None)), viewer));
    }

    #[test]
    fn an_ocelot_takes_fish() {
        let cat = record(EntityKind::Ocelot);
        assert!(click_as(
            &cat,
            Some(&stack(FISH, 0, None)),
            ClickViewer::default()
        ));
        assert!(!click_as(
            &cat,
            Some(&stack(WHEAT, 0, None)),
            ClickViewer::default()
        ));
    }

    #[test]
    fn only_some_minecarts_answer_true() {
        let viewer = ClickViewer::default();
        for (data, taken) in [
            (0, true),
            (1, true),
            (2, true),
            (3, false),
            (4, false),
            (5, true),
            (6, false),
        ] {
            let mut cart = record(EntityKind::Minecart);
            cart.object_data = data;
            assert_eq!(click_as(&cart, None, viewer), taken, "type {data}");
        }
        assert!(click_as(&record(EntityKind::Boat), None, viewer));
        assert!(click_as(&record(EntityKind::LeashKnot), None, viewer));
    }

    #[test]
    fn a_wild_undead_horse_refuses_and_a_tamed_foal_takes_only_food() {
        let viewer = ClickViewer::default();
        let mut horse = record(EntityKind::Horse);
        horse.meta.variant = Variant::Horse(crate::entity::meta::HorseMeta {
            kind: 4,
            ..Default::default()
        });
        assert!(!click_as(&horse, None, viewer));
        horse.meta.variant = Variant::Horse(crate::entity::meta::HorseMeta {
            flags: 2,
            ..Default::default()
        });
        horse.meta.baby = true;
        assert!(!click_as(&horse, Some(&stack(BUCKET, 0, None)), viewer));
        assert!(click_as(&horse, Some(&stack(WHEAT, 0, None)), viewer));
        horse.meta.baby = false;
        assert!(click_as(&horse, None, viewer));
    }

    #[test]
    fn a_spectator_sends_the_packets_and_consumes_nothing() {
        let result = click(&record(EntityKind::ItemFrame), GameMode::Spectator);
        assert_eq!(result.packets.len(), 2);
        assert!(!result.consumed);
    }
}
