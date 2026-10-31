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
use crate::entity::meta::Variant;
use crate::entity::{EntityId, EntityKind, EntityRecord};
use crate::gamemode::GameMode;
use crate::interaction::Outgoing;
use umicraft_protocol::packets::play::ItemStack;

/// The item id of a spawn egg, which a villager does not trade through.
const SPAWN_EGG: i16 = 383;

/// The object type of the minecart that carries a player.
const MINECART: u8 = 10;

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
        local && consumes_interact(tick.entity, tick.held)
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

/// Whether `Entity.interactFirst` answers true on the client, so that the
/// click does not fall through to the held item.
///
/// Covers the entities whose answer does not depend on state the client does
/// not track: item frames and boats (MCP `interactFirst` ends in `true`),
/// minecarts, horses and adult villagers. Entities that react to a held item
/// (cows with a bucket, sheep with shears, wolves with bones) still fall
/// through; `docs/parity.md` records the gap.
fn consumes_interact(entity: &EntityRecord, held: Option<&ItemStack>) -> bool {
    match entity.kind {
        EntityKind::ItemFrame | EntityKind::Horse => true,
        EntityKind::Villager => {
            !entity.meta.baby && held.is_none_or(|item| item.item_id != SPAWN_EGG)
        }
        EntityKind::Other(id) => matches!(id, 1 | MINECART),
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

    #[test]
    fn a_spectator_sends_the_packets_and_consumes_nothing() {
        let result = click(&record(EntityKind::ItemFrame), GameMode::Spectator);
        assert_eq!(result.packets.len(), 2);
        assert!(!result.consumed);
    }
}
