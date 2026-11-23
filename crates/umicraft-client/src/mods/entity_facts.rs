// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the external mods are told about an entity (API 1.1, ADR-0094).
//!
//! One function, [`entity_view`], turns a record of the entity store into the
//! [`EntityView`] of the game-thread query. The render-thread snapshot is built from
//! the same function, one tick earlier: the game thread keeps an owned copy of those
//! views ([`ModEntity`]) in every frame it publishes, and the render thread adds the
//! one fact only it knows, where the frame draws the entity, and keeps the 256
//! nearest to the camera ([`SnapshotBuilder`]). Sharing the function is what makes
//! the snapshot and the query agree.

use std::sync::Arc;

use glam::DVec3;
use umicraft_game::combat::hitbox::size_of;
use umicraft_game::entity::{EntityKind, EntityMeta, EntityRecord, Variant};
use umicraft_game::inventory::armor::armor_points;
use umicraft_game::scoreboard::Scoreboard;
use umicraft_modapi_sys::{
    UMI_ENTITY_INVISIBLE, UMI_ENTITY_LIVING, UMI_ENTITY_ON_FIRE, UMI_ENTITY_SNEAKING,
    UMI_ENTITY_SPRINTING, UMI_ENTITY_USING_ITEM,
};
use umicraft_modhost::{EntitySnapshot, EntityView, MAX_SNAPSHOT_ENTITIES};

use crate::hud::player::HudTracker;

/// What the facts need besides the record itself.
#[derive(Clone, Copy)]
pub(crate) struct FactSources<'a> {
    /// The teams of the scoreboard, which hold players by name.
    pub(crate) scoreboard: &'a Scoreboard,
    /// The HUD state, which remembers `generic.maxHealth` per entity.
    pub(crate) hud: &'a HudTracker,
}

/// The name of the entity as the queries give it: a player's name or the name tag.
fn name_of(record: &EntityRecord) -> Option<&Arc<str>> {
    record
        .profile
        .as_ref()
        .map(|profile| &profile.name)
        .or(record.name_tag.as_ref())
}

/// The mod-facing facts of `record`, with `render_position` equal to the position
/// (the game thread draws nothing between ticks).
///
/// - health: the living-entity health entry of the metadata (MCP
///   `EntityLivingBase.getHealth`, data watcher entry 6), living entities only;
/// - max health: the `generic.maxHealth` attribute the server sent, else the vanilla
///   default of the kind ([`default_max_health`]);
/// - armour: the points of the worn armour the server described with `Entity
///   Equipment` (MCP `ItemArmor.ArmorMaterial`), so only what the client sees;
/// - team: for a player, the scoreboard team of their name (MCP
///   `ScorePlayerTeam`); other entities are on a team by UUID, which the client does
///   not track, so theirs is empty;
/// - size: the hitbox size of the ray test, else the width and height of the kind;
/// - velocity: the change of position over the last tick, which is what the client
///   knows (the server's motion packets are not kept past the tick they move);
/// - flags: entry 0 of the metadata (MCP `Entity.getFlag`), plus living.
pub(crate) fn entity_view<'a>(
    record: &'a EntityRecord,
    sources: &FactSources<'a>,
) -> EntityView<'a> {
    let living = record.kind.is_living();
    let (width, height) = size_of(record.kind, &record.meta).unwrap_or((
        f64::from(record.kind.width()),
        f64::from(record.kind.height()),
    ));
    let team = record
        .profile
        .as_ref()
        .and_then(|profile| sources.scoreboard.team_of(&profile.name))
        .map_or("", |team| team.name.as_str());
    EntityView {
        id: record.id.0,
        kind: record.kind.name(),
        position: record.position.to_array(),
        yaw: record.yaw,
        pitch: record.pitch,
        name: name_of(record).map_or("", |name| &**name),
        health: if living {
            record.meta.living.health
        } else {
            0.0
        },
        max_health: if living {
            sources
                .hud
                .max_health_of(record.id.0)
                .unwrap_or_else(|| default_max_health(record.kind, &record.meta))
        } else {
            0.0
        },
        armor: worn_armor(record),
        team,
        width: width as f32,
        height: height as f32,
        velocity: (record.position - record.previous_position).to_array(),
        flags: flags_of(record, living),
        is_player: record.kind == EntityKind::Player,
        render_position: record.position.to_array(),
    }
}

/// The armour points of what the entity wears.
fn worn_armor(record: &EntityRecord) -> f32 {
    let points: u32 = record
        .armor
        .iter()
        .flatten()
        .filter_map(|piece| armor_points(piece.item_id))
        .map(u32::from)
        .sum();
    points as f32
}

/// The `UMI_ENTITY_*` bits of the entity.
fn flags_of(record: &EntityRecord, living: bool) -> u32 {
    let flags = record.flags;
    [
        (flags.on_fire(), UMI_ENTITY_ON_FIRE),
        (flags.is_sneaking(), UMI_ENTITY_SNEAKING),
        (flags.is_sprinting(), UMI_ENTITY_SPRINTING),
        (flags.is_using_item(), UMI_ENTITY_USING_ITEM),
        (flags.is_invisible(), UMI_ENTITY_INVISIBLE),
        (living, UMI_ENTITY_LIVING),
    ]
    .into_iter()
    .filter(|(set, _)| *set)
    .fold(0, |bits, (_, bit)| bits | bit)
}

/// The vanilla maximum health of a kind, for an entity whose `generic.maxHealth`
/// the server did not send (MCP `applyEntityAttributes` of each class; `20` is the
/// base of `EntityLivingBase` and of players).
///
/// A tamed wolf has 20, a wild one 8 (`EntityWolf.setTamed`), a slime has its size
/// squared (`EntitySlime.setSlimeSize`), an elder guardian 80
/// (`EntityGuardian.applyEntityAttributes` and `setElder`). A horse's value is
/// random per animal (15 to 30) and always arrives as an attribute; `53` is the
/// class default.
pub(crate) fn default_max_health(kind: EntityKind, meta: &EntityMeta) -> f32 {
    match kind {
        EntityKind::Chicken => 4.0,
        EntityKind::Snowman => 4.0,
        EntityKind::Rabbit => 3.0,
        EntityKind::Bat => 6.0,
        EntityKind::Sheep => 8.0,
        EntityKind::Endermite | EntityKind::Silverfish => 8.0,
        EntityKind::Cow
        | EntityKind::Mooshroom
        | EntityKind::Pig
        | EntityKind::Squid
        | EntityKind::Ghast
        | EntityKind::Ocelot => 10.0,
        EntityKind::CaveSpider => 12.0,
        EntityKind::Spider => 16.0,
        EntityKind::Witch => 26.0,
        EntityKind::Enderman => 40.0,
        EntityKind::Horse => 53.0,
        EntityKind::Giant | EntityKind::IronGolem => 100.0,
        EntityKind::Dragon => 200.0,
        EntityKind::Wither => 300.0,
        EntityKind::Wolf => match meta.variant {
            Variant::Wolf(wolf) if wolf.tamed => 20.0,
            _ => 8.0,
        },
        EntityKind::Slime | EntityKind::MagmaCube => match meta.variant {
            Variant::Slime(size) => f32::from(size) * f32::from(size),
            _ => 1.0,
        },
        EntityKind::Guardian => match meta.variant {
            Variant::Guardian { elder: true, .. } => 80.0,
            _ => 30.0,
        },
        _ => 20.0,
    }
}

/// An entity as the game thread publishes it to the render thread: the facts of
/// [`entity_view`] owned, plus the position of the previous tick, which the render
/// thread interpolates with.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ModEntity {
    /// The facts; its strings are empty, the owned ones are below.
    facts: EntityView<'static>,
    name: Option<Arc<str>>,
    team: Option<Arc<str>>,
    previous_position: DVec3,
}

impl ModEntity {
    /// Copies the facts of `record`. The team name is the one allocation, for a
    /// player on a team.
    pub(crate) fn of(record: &EntityRecord, sources: &FactSources<'_>) -> Self {
        let view = entity_view(record, sources);
        Self {
            team: (!view.team.is_empty()).then(|| Arc::from(view.team)),
            facts: view.with_text(record.kind.name(), "", ""),
            name: name_of(record).cloned(),
            previous_position: record.previous_position,
        }
    }

    /// The facts with the entity drawn `alpha` of the way from the previous tick to
    /// this one (the same interpolation as `EntityFrameData::interpolate`).
    fn view(&self, alpha: f64) -> EntityView<'_> {
        let position = DVec3::from_array(self.facts.position);
        EntityView {
            render_position: self.previous_position.lerp(position, alpha).to_array(),
            ..self.facts.with_text(
                self.facts.kind,
                self.name.as_deref().unwrap_or_default(),
                self.team.as_deref().unwrap_or_default(),
            )
        }
    }
}

/// The reusable buffers that make the snapshot of a frame.
#[derive(Debug, Default)]
pub(crate) struct SnapshotBuilder {
    snapshot: EntitySnapshot,
    /// Squared distance to the eye and index of every candidate, reused.
    order: Vec<(f64, u32)>,
}

impl SnapshotBuilder {
    /// The snapshot as of the last [`refresh`](Self::refresh).
    pub(crate) fn snapshot(&self) -> &EntitySnapshot {
        &self.snapshot
    }

    /// Marks the snapshot as having no world.
    pub(crate) fn clear(&mut self) {
        self.snapshot.clear();
    }

    /// Rebuilds the snapshot: the [`MAX_SNAPSHOT_ENTITIES`] entities of `entities`
    /// nearest to `eye`, nearest first, each drawn `alpha` of the way through the
    /// tick. Nothing is allocated once the buffers have grown to the working size.
    pub(crate) fn refresh(&mut self, entities: &[ModEntity], eye: DVec3, alpha: f64) {
        self.order.clear();
        self.order
            .extend(entities.iter().enumerate().map(|(index, entity)| {
                let drawn = DVec3::from_array(entity.view(alpha).render_position);
                (drawn.distance_squared(eye), index as u32)
            }));
        let nearest_first =
            |a: &(f64, u32), b: &(f64, u32)| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1));
        if self.order.len() > MAX_SNAPSHOT_ENTITIES {
            self.order
                .select_nth_unstable_by(MAX_SNAPSHOT_ENTITIES - 1, nearest_first);
            self.order.truncate(MAX_SNAPSHOT_ENTITIES);
        }
        self.order.sort_unstable_by(nearest_first);
        self.snapshot.begin();
        for &(_, index) in &self.order {
            if let Some(entity) = entities.get(index as usize) {
                self.snapshot.push(&entity.view(alpha));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use umicraft_game::entity::meta::WolfMeta;
    use umicraft_game::entity::{EntityFlags, EntityId, EntityItem, PlayerProfile};
    use umicraft_net::play::PlayEvent;
    use umicraft_protocol::packets::play::clientbound::{
        EntityProperties, EntityProperty, TeamInfo, Teams,
    };

    use super::*;

    fn record(id: i32, kind: EntityKind, x: f64) -> EntityRecord {
        EntityRecord::new(EntityId(id), kind, DVec3::new(x, 64.0, 0.0), 0.0, 0.0)
    }

    fn item(item_id: i16) -> EntityItem {
        EntityItem {
            item_id,
            damage: 0,
            count: 1,
            nbt: None,
        }
    }

    fn named(id: i32, name: &str) -> EntityRecord {
        let mut player = record(id, EntityKind::Player, 0.0);
        player.profile = Some(Arc::new(PlayerProfile {
            name: Arc::from(name),
            skin_url: None,
            cape_url: None,
            slim: None,
        }));
        player
    }

    fn red_team(members: &[&str]) -> Scoreboard {
        let mut board = Scoreboard::new();
        board.apply_teams(&Teams::Create {
            name: "red".to_owned(),
            info: TeamInfo {
                display_name: "Red".to_owned(),
                prefix: String::new(),
                suffix: String::new(),
                friendly_flags: 0,
                name_tag_visibility: "always".to_owned(),
                color: -1,
            },
            players: members.iter().map(|member| (*member).to_owned()).collect(),
        });
        board
    }

    fn max_health_packet(entity_id: i32, value: f64) -> PlayEvent {
        PlayEvent::EntityProperties(EntityProperties {
            entity_id,
            properties: vec![EntityProperty {
                key: "generic.maxHealth".to_owned(),
                value,
                modifiers: Vec::new(),
            }],
        })
    }

    /// The facts of `record` with no teams and no attributes, as a snapshot row (an
    /// owned copy, so the throwaway sources need not outlive it).
    fn plain(record: &EntityRecord) -> EntitySnapshot {
        let (board, hud) = (Scoreboard::new(), HudTracker::default());
        let sources = FactSources {
            scoreboard: &board,
            hud: &hud,
        };
        let mut one = EntitySnapshot::new();
        one.begin();
        one.push(&entity_view(record, &sources));
        one
    }

    fn facts(record: &EntityRecord) -> EntityView<'static> {
        // The snapshot is leaked: a test-only way to hand out its borrowed text.
        let leaked: &'static EntitySnapshot = Box::leak(Box::new(plain(record)));
        leaked.get(0).unwrap()
    }

    #[test]
    fn a_living_entity_reports_its_health_and_the_vanilla_maximum() {
        let mut zombie = record(1, EntityKind::Zombie, 3.0);
        zombie.meta.living.health = 12.5;
        let view = facts(&zombie);
        assert_eq!((view.health, view.max_health), (12.5, 20.0));
        assert_ne!(view.flags & UMI_ENTITY_LIVING, 0);
        assert_eq!((view.width, view.height), (0.6, 1.95));
        assert_eq!(view.kind, "zombie");
        assert!(!view.is_player);
        assert_eq!(view.render_position, view.position);
    }

    #[test]
    fn the_server_attribute_beats_the_default_maximum() {
        let mut hud = HudTracker::default();
        hud.observe(&max_health_packet(1, 40.0), 0);
        let board = Scoreboard::new();
        let sources = FactSources {
            scoreboard: &board,
            hud: &hud,
        };
        let pig = record(1, EntityKind::Pig, 0.0);
        assert_eq!(entity_view(&pig, &sources).max_health, 40.0);
        let other_pig = record(2, EntityKind::Pig, 0.0);
        assert_eq!(entity_view(&other_pig, &sources).max_health, 10.0);
    }

    #[test]
    fn an_object_is_not_living_and_has_no_health() {
        let mut dropped = record(1, EntityKind::Item, 0.0);
        dropped.meta.living.health = 7.0;
        let view = facts(&dropped);
        assert_eq!((view.health, view.max_health), (0.0, 0.0));
        assert_eq!(view.flags & UMI_ENTITY_LIVING, 0);
        assert_eq!((view.width, view.height), (0.25, 0.25));
    }

    #[test]
    fn the_flags_come_from_the_metadata_byte() {
        let mut zombie = record(1, EntityKind::Zombie, 0.0);
        assert_eq!(facts(&zombie).flags, UMI_ENTITY_LIVING);
        zombie.flags = EntityFlags::new(0x01 | 0x02 | 0x08 | 0x10 | 0x20, 0);
        let all = UMI_ENTITY_ON_FIRE
            | UMI_ENTITY_SNEAKING
            | UMI_ENTITY_SPRINTING
            | UMI_ENTITY_USING_ITEM
            | UMI_ENTITY_INVISIBLE
            | UMI_ENTITY_LIVING;
        assert_eq!(facts(&zombie).flags, all);
        zombie.flags = EntityFlags::new(0x04, 0);
        assert_eq!(
            facts(&zombie).flags,
            UMI_ENTITY_LIVING,
            "bit 2 (riding) is not a flag"
        );
    }

    #[test]
    fn the_velocity_is_the_change_over_the_last_tick() {
        let mut zombie = record(1, EntityKind::Zombie, 5.0);
        zombie.previous_position = DVec3::new(4.5, 64.5, 0.0);
        assert_eq!(facts(&zombie).velocity, [0.5, -0.5, 0.0]);
    }

    #[test]
    fn the_armour_adds_the_points_of_the_worn_pieces() {
        let mut player = record(1, EntityKind::Player, 0.0);
        assert_eq!(facts(&player).armor, 0.0);
        // Boots to helmet: diamond boots 3, a stick adds nothing, leather helmet 1.
        player.armor = [Some(item(313)), Some(item(280)), None, Some(item(298))];
        assert_eq!(facts(&player).armor, 4.0);
    }

    #[test]
    fn a_player_is_on_the_team_of_their_name() {
        let board = red_team(&["Ann"]);
        let hud = HudTracker::default();
        let sources = FactSources {
            scoreboard: &board,
            hud: &hud,
        };
        let ann = named(1, "Ann");
        let view = entity_view(&ann, &sources);
        assert_eq!((view.team, view.name, view.is_player), ("red", "Ann", true));
        assert_eq!(entity_view(&named(2, "Bob"), &sources).team, "");
        let zombie = record(3, EntityKind::Zombie, 0.0);
        assert_eq!(entity_view(&zombie, &sources).team, "");
    }

    #[test]
    fn the_name_tag_names_a_mob() {
        let mut cow = record(1, EntityKind::Cow, 0.0);
        cow.name_tag = Some(Arc::from("Bessie"));
        assert_eq!(facts(&cow).name, "Bessie");
    }

    #[test]
    fn a_child_is_smaller_and_a_slime_has_its_size() {
        let mut pig = record(1, EntityKind::Pig, 0.0);
        pig.meta.baby = true;
        assert_eq!((facts(&pig).width, facts(&pig).height), (0.45, 0.45));
        let mut slime = record(2, EntityKind::Slime, 0.0);
        slime.meta.variant = Variant::Slime(4);
        assert_eq!(facts(&slime).width, 2.4_f32);
        assert_eq!(facts(&slime).max_health, 16.0);
    }

    #[test]
    fn the_default_maximum_health_follows_the_vanilla_classes() {
        let meta = EntityMeta::default();
        let table = [
            (EntityKind::Player, 20.0),
            (EntityKind::Zombie, 20.0),
            (EntityKind::Chicken, 4.0),
            (EntityKind::Sheep, 8.0),
            (EntityKind::Cow, 10.0),
            (EntityKind::Spider, 16.0),
            (EntityKind::Enderman, 40.0),
            (EntityKind::IronGolem, 100.0),
            (EntityKind::Dragon, 200.0),
            (EntityKind::Wither, 300.0),
            (EntityKind::Guardian, 30.0),
            (EntityKind::Other(255), 20.0),
        ];
        for (kind, health) in table {
            assert_eq!(default_max_health(kind, &meta), health, "{kind:?}");
        }
        let tamed = EntityMeta {
            variant: Variant::Wolf(WolfMeta {
                tamed: true,
                ..WolfMeta::default()
            }),
            ..EntityMeta::default()
        };
        assert_eq!(default_max_health(EntityKind::Wolf, &meta), 8.0);
        assert_eq!(default_max_health(EntityKind::Wolf, &tamed), 20.0);
    }

    fn mods_of(records: &[EntityRecord]) -> Vec<ModEntity> {
        let (board, hud) = (Scoreboard::new(), HudTracker::default());
        let sources = FactSources {
            scoreboard: &board,
            hud: &hud,
        };
        records
            .iter()
            .map(|record| ModEntity::of(record, &sources))
            .collect()
    }

    fn ids(builder: &SnapshotBuilder) -> Vec<i32> {
        builder.snapshot().iter().map(|view| view.id).collect()
    }

    #[test]
    fn the_snapshot_keeps_the_nearest_entities_nearest_first() {
        let records: Vec<EntityRecord> = [30.0, 5.0, 12.0, 1.0]
            .into_iter()
            .enumerate()
            .map(|(id, x)| record(id as i32, EntityKind::Pig, x))
            .collect();
        let mut builder = SnapshotBuilder::default();
        builder.refresh(&mods_of(&records), DVec3::new(0.0, 64.0, 0.0), 1.0);
        assert_eq!(ids(&builder), [3, 1, 2, 0]);
        assert!(builder.snapshot().is_available());
    }

    #[test]
    fn the_snapshot_is_bounded_and_keeps_the_nearest_of_a_crowd() {
        // Far to near, so the nearest ones are the last of the list.
        let records: Vec<EntityRecord> = (0..400)
            .map(|id| record(id, EntityKind::Item, f64::from(400 - id)))
            .collect();
        let mut builder = SnapshotBuilder::default();
        builder.refresh(&mods_of(&records), DVec3::new(0.0, 64.0, 0.0), 1.0);
        assert_eq!(builder.snapshot().len(), MAX_SNAPSHOT_ENTITIES);
        let ids = ids(&builder);
        assert_eq!(ids.first(), Some(&399));
        assert_eq!(ids.last(), Some(&144));
        assert!(ids.windows(2).all(|pair| pair[0] > pair[1]));
    }

    #[test]
    fn the_render_position_is_interpolated_and_orders_the_snapshot() {
        let mut runner = record(1, EntityKind::Zombie, 20.0);
        runner.previous_position = DVec3::new(10.0, 64.0, 0.0);
        let still = record(2, EntityKind::Zombie, 16.0);
        let mut builder = SnapshotBuilder::default();
        let eye = DVec3::new(0.0, 64.0, 0.0);
        let entities = mods_of(&[runner, still]);
        builder.refresh(&entities, eye, 0.5);
        // Half a tick in, the runner is at 15: nearer than the one at 16.
        assert_eq!(ids(&builder), [1, 2]);
        let first = builder.snapshot().get(0).unwrap();
        assert_eq!(
            first.position,
            [20.0, 64.0, 0.0],
            "tick facts are the tick's"
        );
        assert_eq!(first.render_position, [15.0, 64.0, 0.0]);
        builder.refresh(&entities, eye, 1.0);
        assert_eq!(ids(&builder), [2, 1]);
    }

    #[test]
    fn refreshing_again_reuses_the_buffers() {
        let records: Vec<EntityRecord> = (0..300)
            .map(|id| record(id, EntityKind::Item, f64::from(id)))
            .collect();
        let entities = mods_of(&records);
        let mut builder = SnapshotBuilder::default();
        builder.refresh(&entities, DVec3::ZERO, 0.0);
        let order = builder.order.capacity();
        builder.refresh(&entities, DVec3::ZERO, 0.5);
        assert_eq!(builder.order.capacity(), order);
    }

    #[test]
    fn clearing_makes_the_snapshot_unavailable() {
        let mut builder = SnapshotBuilder::default();
        builder.refresh(&[], DVec3::ZERO, 0.0);
        assert!(builder.snapshot().is_available() && builder.snapshot().is_empty());
        builder.clear();
        assert!(!builder.snapshot().is_available());
    }
}
