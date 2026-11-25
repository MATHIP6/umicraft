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
use umicraft_game::chat::style::SECTION;
use umicraft_game::chat::Color;
use umicraft_game::combat::hitbox::size_of;
use umicraft_game::entity::{EntityKind, EntityMeta, EntityRecord, Variant};
use umicraft_game::inventory::armor::armor_points;
use umicraft_game::player_list::PlayerList;
use umicraft_game::raycast::raycast;
use umicraft_game::scoreboard::{Scoreboard, Team};
use umicraft_game::server::effects::Effects;
use umicraft_modapi_sys::{
    UMI_ENTITY_INVISIBLE, UMI_ENTITY_IN_TAB_LIST, UMI_ENTITY_LIVING, UMI_ENTITY_ON_FIRE,
    UMI_ENTITY_SNEAKING, UMI_ENTITY_SPECTATOR, UMI_ENTITY_SPRINTING, UMI_ENTITY_TEAMMATE,
    UMI_ENTITY_USING_ITEM, UMI_ENTITY_VISIBLE,
};
use umicraft_modhost::{
    Effect, EntityEffects, EntitySnapshot, EntityTexts, EntityView, HeldItem, MAX_SNAPSHOT_ENTITIES,
};
use umicraft_world::World;

use crate::hud::player::HudTracker;

/// The longest line of the visibility test, in blocks (ADR-0097).
const SIGHT_RANGE: f64 = 128.0;

/// What the visibility test looks from and through.
#[derive(Clone, Copy)]
pub(crate) struct Sight<'a> {
    /// The local player's eye.
    pub(crate) eye: DVec3,
    /// The blocks a line of sight stops at.
    pub(crate) world: &'a World,
}

/// What the facts need besides the record itself.
#[derive(Clone, Copy)]
pub(crate) struct FactSources<'a> {
    /// The teams of the scoreboard, which hold players by name.
    pub(crate) scoreboard: &'a Scoreboard,
    /// The HUD state, which remembers `generic.maxHealth` per entity.
    pub(crate) hud: &'a HudTracker,
    /// The tab list: ping and presence (API 1.4).
    pub(crate) players: &'a PlayerList,
    /// The local player's name, whose team makes team mates (API 1.4).
    pub(crate) own_name: &'a str,
    /// The player's eye and the blocks, `None` before there is a world: then no
    /// entity is visible (API 1.4).
    pub(crate) sight: Option<Sight<'a>>,
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
/// (the game thread draws nothing between ticks). `display` holds the formatted
/// name the view borrows.
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
/// - flags: entry 0 of the metadata (MCP `Entity.getFlag`), plus living, and the
///   API 1.4 bits: spectator and listed from the tab list, visible from
///   [`sees`], team mate from the scoreboard;
/// - hurt and death time: the client's own animation counters (MCP `hurtTime`,
///   `deathTime`);
/// - display name: `ScorePlayerTeam.formatPlayerName` for a player, the name tag
///   otherwise; team colour: [`team_color`];
/// - ping and UUID version: the tab list entry and the UUID of the spawn packet.
pub(crate) fn entity_view<'a>(
    record: &'a EntityRecord,
    sources: &FactSources<'a>,
    display: &'a mut String,
) -> EntityView<'a> {
    let living = record.kind.is_living();
    let (width, height) = size_of(record.kind, &record.meta).unwrap_or((
        f64::from(record.kind.width()),
        f64::from(record.kind.height()),
    ));
    let team = record
        .profile
        .as_ref()
        .and_then(|profile| sources.scoreboard.team_of(&profile.name));
    let name = name_of(record).map_or("", |name| &**name);
    format_display_name(display, team, name);
    let listed = record
        .uuid
        .and_then(|uuid| sources.players.get(&u128::from_be_bytes(uuid)));
    let visible = sources
        .sight
        .is_some_and(|sight| sees(&sight, record.position, height));
    let teammate = team.is_some_and(|team| {
        sources
            .scoreboard
            .team_of(sources.own_name)
            .is_some_and(|own| own.name == team.name)
    });
    let flags = flags_of(record, living)
        | bit(record.spectator, UMI_ENTITY_SPECTATOR)
        | bit(listed.is_some(), UMI_ENTITY_IN_TAB_LIST)
        | bit(visible, UMI_ENTITY_VISIBLE)
        | bit(teammate, UMI_ENTITY_TEAMMATE);
    EntityView {
        id: record.id.0,
        kind: record.kind.name(),
        position: record.position.to_array(),
        yaw: record.yaw,
        pitch: record.pitch,
        name,
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
        team: team.map_or("", |team| team.name.as_str()),
        width: width as f32,
        height: height as f32,
        velocity: (record.position - record.previous_position).to_array(),
        flags,
        is_player: record.kind == EntityKind::Player,
        render_position: record.position.to_array(),
        hurt_time: u32::from(record.animation.hurt_time()),
        death_time: u32::from(record.animation.death_time()),
        held_item: record.held.as_ref().map(|item| HeldItem {
            id: i32::from(item.item_id),
            damage: i32::from(item.damage),
        }),
        display_name: display.as_str(),
        team_color: team.and_then(team_color),
        ping: listed.map(|entry| entry.ping),
        uuid_version: record.uuid.map_or(0, |uuid| uuid[6] >> 4),
        effects: effects_of(&record.effects),
    }
}

/// `bit` when `set`, else nothing.
fn bit(set: bool, bit: u32) -> u32 {
    if set {
        bit
    } else {
        0
    }
}

/// Writes the name as the player sees it into `out`: the team's prefix, the name
/// and the suffix (`ScorePlayerTeam.formatPlayerName`), or the bare name.
fn format_display_name(out: &mut String, team: Option<&Team>, name: &str) {
    out.clear();
    match team {
        Some(team) => {
            out.push_str(&team.prefix);
            out.push_str(name);
            out.push_str(&team.suffix);
        }
        None => out.push_str(name),
    }
}

/// The colour of a team: the colour index the server gave it, else the last
/// colour code of its prefix, which is what 1.8 draws member names with.
fn team_color(team: &Team) -> Option<u32> {
    let own = u8::try_from(team.color)
        .ok()
        .and_then(|index| char::from_digit(u32::from(index), 16))
        .and_then(Color::from_code);
    let color = own.or_else(|| last_color_code(&team.prefix))?;
    let [r, g, b] = color.rgb();
    Some(u32::from_be_bytes([0, r, g, b]))
}

/// The last colour of `text`'s `§` codes; a formatting code (`§l`) is no colour.
fn last_color_code(text: &str) -> Option<Color> {
    let mut chars = text.chars();
    let mut last = None;
    while let Some(c) = chars.next() {
        if c == SECTION {
            if let Some(color) = chars.next().and_then(Color::from_code) {
                last = Some(color);
            }
        }
    }
    last
}

/// Whether a line from the eye reaches the entity standing at `feet`, `height`
/// tall, without a block in the way: its eye height (MCP `getEyeHeight`, 0.85 of
/// the height), its middle or just above its feet. Farther than [`SIGHT_RANGE`]
/// it is not visible.
fn sees(sight: &Sight<'_>, feet: DVec3, height: f64) -> bool {
    [height * 0.85, height * 0.5, 0.1].into_iter().any(|up| {
        let to = feet + DVec3::Y * up - sight.eye;
        let distance = to.length();
        distance <= SIGHT_RANGE
            && (distance < 1e-6 || raycast(sight.eye, to, distance, sight.world).is_none())
    })
}

/// The effects as the API carries them, at most 8, in potion id order.
fn effects_of(effects: &Effects) -> EntityEffects {
    effects
        .iter()
        .map(|effect| Effect {
            id: effect.effect_id,
            amplifier: effect.amplifier as u8,
            hide_particles: !effect.show_particles,
            duration: effect.duration,
        })
        .collect()
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
    /// The display name when it is not the bare name (a team prefix or suffix).
    display_name: Option<Arc<str>>,
    previous_position: DVec3,
}

/// What the client knows of the local player besides its body (API 1.4).
pub(crate) struct LocalPlayer<'a> {
    /// The body the third person draws, for the position, size and flags.
    pub(crate) body: &'a EntityRecord,
    /// The player's entity id.
    pub(crate) id: i32,
    /// The player's name.
    pub(crate) name: &'a str,
    /// The health of the last `Update Health`.
    pub(crate) health: f32,
    /// The player's potion effects.
    pub(crate) effects: &'a Effects,
    /// The stack of the selected hotbar slot.
    pub(crate) held: Option<HeldItem>,
}

impl ModEntity {
    /// Copies the facts of `record`; `scratch` is reused for the display name. The
    /// strings that differ from the record's own (team, a decorated display name)
    /// are the allocations, for players on a team.
    pub(crate) fn of(
        record: &EntityRecord,
        sources: &FactSources<'_>,
        scratch: &mut String,
    ) -> Self {
        let view = entity_view(record, sources, scratch);
        Self::from_view(
            &view,
            record.kind.name(),
            name_of(record).cloned(),
            record.previous_position,
        )
    }

    /// The local player as the mods see it: its body's facts with the name, id,
    /// health, effects, held item and tab-list facts of the player. It is not
    /// "visible" (the test is from its own eye) and is its own team mate when on
    /// a team.
    pub(crate) fn local(
        player: &LocalPlayer<'_>,
        sources: &FactSources<'_>,
        scratch: &mut String,
    ) -> Self {
        let body = entity_view(player.body, sources, scratch);
        let team = sources.scoreboard.team_of(player.name);
        let mut display = String::new();
        format_display_name(&mut display, team, player.name);
        let listed = sources.players.by_name(player.name);
        let flags = (body.flags
            & !(UMI_ENTITY_VISIBLE | UMI_ENTITY_IN_TAB_LIST | UMI_ENTITY_TEAMMATE))
            | bit(listed.is_some(), UMI_ENTITY_IN_TAB_LIST)
            | bit(team.is_some(), UMI_ENTITY_TEAMMATE);
        let view = EntityView {
            id: player.id,
            kind: EntityKind::Player.name(),
            name: player.name,
            health: player.health,
            max_health: sources
                .hud
                .max_health_of(player.id)
                .unwrap_or_else(|| default_max_health(EntityKind::Player, &player.body.meta)),
            team: team.map_or("", |team| team.name.as_str()),
            flags: flags | UMI_ENTITY_LIVING,
            is_player: true,
            held_item: player.held,
            display_name: &display,
            team_color: team.and_then(team_color),
            ping: listed.map(|entry| entry.ping),
            uuid_version: listed.map_or(0, |entry| (entry.uuid >> 76) as u8 & 0xF),
            effects: effects_of(player.effects),
            ..body
        };
        Self::from_view(
            &view,
            EntityKind::Player.name(),
            Some(Arc::from(player.name)),
            player.body.previous_position,
        )
    }

    /// Keeps `view` with owned strings; `kind` is its kind's static name and `name`
    /// the shared copy of its name.
    fn from_view(
        view: &EntityView<'_>,
        kind: &'static str,
        name: Option<Arc<str>>,
        previous_position: DVec3,
    ) -> Self {
        let owned = |text: &str| (!text.is_empty()).then(|| Arc::from(text));
        Self {
            team: owned(view.team),
            display_name: (view.display_name != view.name)
                .then(|| owned(view.display_name))
                .flatten(),
            facts: view.with_text(EntityTexts {
                kind,
                ..EntityTexts::default()
            }),
            name,
            previous_position,
        }
    }

    /// The facts with the entity drawn `alpha` of the way from the previous tick to
    /// this one (the same interpolation as `EntityFrameData::interpolate`).
    fn view(&self, alpha: f64) -> EntityView<'_> {
        let position = DVec3::from_array(self.facts.position);
        let name = self.name.as_deref().unwrap_or_default();
        EntityView {
            render_position: self.previous_position.lerp(position, alpha).to_array(),
            ..self.facts.with_text(EntityTexts {
                kind: self.facts.kind,
                name,
                team: self.team.as_deref().unwrap_or_default(),
                display_name: self.display_name.as_deref().unwrap_or(name),
            })
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
    /// tick, and the `local` player apart. Nothing is allocated once the buffers
    /// have grown to the working size.
    pub(crate) fn refresh(
        &mut self,
        entities: &[ModEntity],
        local: Option<&ModEntity>,
        eye: DVec3,
        alpha: f64,
    ) {
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
        if let Some(local) = local {
            self.snapshot.set_local(&local.view(alpha));
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

    /// Sources with no tab list, no world and a local player named `Me`.
    fn bare_sources<'a>(board: &'a Scoreboard, hud: &'a HudTracker) -> FactSources<'a> {
        static NOBODY: std::sync::OnceLock<PlayerList> = std::sync::OnceLock::new();
        FactSources {
            scoreboard: board,
            hud,
            players: NOBODY.get_or_init(PlayerList::new),
            own_name: "Me",
            sight: None,
        }
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
        let sources = bare_sources(&board, &hud);
        let mut one = EntitySnapshot::new();
        one.begin();
        one.push(&entity_view(record, &sources, &mut String::new()));
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
        let sources = bare_sources(&board, &hud);
        let pig = record(1, EntityKind::Pig, 0.0);
        assert_eq!(
            entity_view(&pig, &sources, &mut String::new()).max_health,
            40.0
        );
        let other_pig = record(2, EntityKind::Pig, 0.0);
        assert_eq!(
            entity_view(&other_pig, &sources, &mut String::new()).max_health,
            10.0
        );
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
        let sources = bare_sources(&board, &hud);
        let ann = named(1, "Ann");
        let mut display = String::new();
        let view = entity_view(&ann, &sources, &mut display);
        assert_eq!((view.team, view.name, view.is_player), ("red", "Ann", true));
        assert_eq!(
            entity_view(&named(2, "Bob"), &sources, &mut String::new()).team,
            ""
        );
        let zombie = record(3, EntityKind::Zombie, 0.0);
        assert_eq!(entity_view(&zombie, &sources, &mut String::new()).team, "");
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

    /// A team `name` with `prefix`, colour index `color` and `members`.
    fn team(board: &mut Scoreboard, name: &str, prefix: &str, color: i8, members: &[&str]) {
        board.apply_teams(&Teams::Create {
            name: name.to_owned(),
            info: TeamInfo {
                display_name: name.to_owned(),
                prefix: prefix.to_owned(),
                suffix: "\u{a7}r".to_owned(),
                friendly_flags: 0,
                name_tag_visibility: "always".to_owned(),
                color,
            },
            players: members.iter().map(|member| (*member).to_owned()).collect(),
        });
    }

    #[test]
    fn combat_state_comes_from_the_record() {
        let mut zombie = record(1, EntityKind::Zombie, 0.0);
        assert_eq!(facts(&zombie).held_item, None);
        zombie.animation.hurt();
        zombie.held = Some(EntityItem {
            damage: 12,
            ..item(267)
        });
        zombie.uuid = Some([0x12, 0, 0, 0, 0, 0, 0x2a, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let view = facts(&zombie);
        assert_eq!((view.hurt_time, view.death_time), (10, 0));
        assert_eq!(
            view.held_item,
            Some(HeldItem {
                id: 267,
                damage: 12
            })
        );
        assert_eq!(view.uuid_version, 2);
        zombie.animation.die();
        zombie.animation.tick();
        assert!(facts(&zombie).death_time > 0);
    }

    #[test]
    fn the_effects_of_the_record_are_listed_in_id_order() {
        use umicraft_protocol::packets::play::clientbound::EntityEffect;
        let mut zombie = record(1, EntityKind::Zombie, 0.0);
        for (id, amplifier) in [(14_i8, 0_i8), (1, 1)] {
            zombie.effects.apply(&EntityEffect {
                entity_id: 1,
                effect_id: id,
                amplifier,
                duration: 300,
                // The wire byte is really "show particles" (see `Effects::apply`).
                hide_particles: i8::from(id != 14),
            });
        }
        let view = facts(&zombie);
        let effects = view.effects.as_slice();
        assert_eq!(effects.len(), 2);
        assert_eq!((effects[0].id, effects[0].amplifier), (1, 1));
        assert_eq!(
            (
                effects[1].id,
                effects[1].hide_particles,
                effects[1].duration
            ),
            (14, true, 300)
        );
    }

    #[test]
    fn the_display_name_and_colour_come_from_the_team() {
        let mut board = Scoreboard::new();
        team(&mut board, "red", "\u{a7}l\u{a7}c[R] ", -1, &["Ann"]);
        team(&mut board, "blue", "", 9, &["Bob", "Me"]);
        let hud = HudTracker::default();
        let sources = bare_sources(&board, &hud);
        let mut display = String::new();
        let ann = named(1, "Ann");
        let view = entity_view(&ann, &sources, &mut display);
        assert_eq!(view.display_name, "\u{a7}l\u{a7}c[R] Ann\u{a7}r");
        assert_eq!(
            view.team_color,
            Some(0xff5555),
            "the prefix colour; §l is no colour"
        );
        assert_eq!(view.flags & UMI_ENTITY_TEAMMATE, 0);
        let bob = named(2, "Bob");
        let view = entity_view(&bob, &sources, &mut display);
        assert_eq!(
            view.team_color,
            Some(0x5555ff),
            "the team's own colour wins"
        );
        assert_ne!(view.flags & UMI_ENTITY_TEAMMATE, 0, "Bob is on Me's team");
        let mut cow = record(3, EntityKind::Cow, 0.0);
        cow.name_tag = Some(Arc::from("\u{a7}6Bessie"));
        let view = entity_view(&cow, &sources, &mut display);
        assert_eq!(
            (view.display_name, view.team_color),
            ("\u{a7}6Bessie", None)
        );
    }

    #[test]
    fn the_tab_list_gives_the_ping_and_the_spectators() {
        use umicraft_protocol::packets::play::clientbound::{AddPlayerEntry, PlayerListItem};
        let mut players = PlayerList::new();
        players.apply(&PlayerListItem::AddPlayer(vec![AddPlayerEntry {
            uuid: u128::from_be_bytes([7; 16]),
            name: "Ann".to_owned(),
            properties: Vec::new(),
            game_mode: 3,
            ping: 87,
            display_name: None,
        }]));
        let (board, hud) = (Scoreboard::new(), HudTracker::default());
        let sources = FactSources {
            players: &players,
            ..bare_sources(&board, &hud)
        };
        let mut listed = named(1, "Ann");
        listed.uuid = Some([7; 16]);
        listed.spectator = true;
        let mut display = String::new();
        let view = entity_view(&listed, &sources, &mut display);
        assert_eq!(view.ping, Some(87));
        let both = UMI_ENTITY_IN_TAB_LIST | UMI_ENTITY_SPECTATOR;
        assert_eq!(view.flags & both, both);
        let mut npc = named(2, "Npc");
        npc.uuid = Some([9; 16]);
        let view = entity_view(&npc, &sources, &mut display);
        assert_eq!((view.ping, view.flags & UMI_ENTITY_IN_TAB_LIST), (None, 0));
    }

    /// A world with one loaded section at y 64..80 and a stone wall at x = 2.
    fn walled() -> World {
        use umicraft_world::{BlockPos, BlockState, ChunkPayload, ChunkPos};
        let mut world = World::new(0);
        let payload = vec![0u8; umicraft_world::chunk::payload_len(0b10000, true, true)];
        world
            .apply_chunk(
                ChunkPos::new(0, 0),
                ChunkPayload {
                    primary_bit_mask: 0b10000,
                    ground_up: true,
                    data: &payload,
                },
            )
            .expect("an empty column");
        for y in 64..72 {
            for z in 0..16 {
                world.set_block(BlockPos::new(2, y, z), BlockState::new(1, 0));
            }
        }
        world
    }

    #[test]
    fn an_entity_is_visible_only_without_a_block_in_the_way() {
        let world = walled();
        let (board, hud) = (Scoreboard::new(), HudTracker::default());
        let sight = |eye| FactSources {
            sight: Some(Sight { eye, world: &world }),
            ..bare_sources(&board, &hud)
        };
        let zombie = |x| {
            let mut zombie = record(1, EntityKind::Zombie, x);
            zombie.position.z = 8.5;
            zombie
        };
        let eye = DVec3::new(0.5, 65.62, 8.5);
        let visible = |record: &EntityRecord, sources: &FactSources<'_>| {
            entity_view(record, sources, &mut String::new()).flags & UMI_ENTITY_VISIBLE != 0
        };
        assert!(visible(&zombie(1.5), &sight(eye)), "in front of the wall");
        assert!(!visible(&zombie(5.5), &sight(eye)), "behind the wall");
        let above = DVec3::new(0.5, 90.0, 8.5);
        assert!(visible(&zombie(5.5), &sight(above)), "seen over the wall");
        assert!(
            !visible(&zombie(1.5), &bare_sources(&board, &hud)),
            "no world, nothing visible"
        );
        let far = DVec3::new(0.5, 65.62, 8.5 + SIGHT_RANGE + 10.0);
        assert!(!visible(&zombie(1.5), &sight(far)), "past the range");
    }

    #[test]
    fn the_local_player_has_its_own_name_team_and_effects() {
        use umicraft_protocol::packets::play::clientbound::EntityEffect;
        let mut board = Scoreboard::new();
        team(&mut board, "blue", "\u{a7}9", -1, &["Me"]);
        let hud = HudTracker::default();
        let sources = bare_sources(&board, &hud);
        let mut effects = Effects::default();
        effects.apply(&EntityEffect {
            entity_id: 5,
            effect_id: 5,
            amplifier: 1,
            duration: 100,
            hide_particles: 0,
        });
        let body = record(0, EntityKind::Player, 4.0);
        let local = ModEntity::local(
            &LocalPlayer {
                body: &body,
                id: 5,
                name: "Me",
                health: 13.0,
                effects: &effects,
                held: Some(HeldItem { id: 276, damage: 0 }),
            },
            &sources,
            &mut String::new(),
        );
        let mut builder = SnapshotBuilder::default();
        builder.refresh(&[], Some(&local), DVec3::ZERO, 1.0);
        let me = builder.snapshot().local().expect("the local player");
        assert_eq!((me.id, me.name, me.team), (5, "Me", "blue"));
        assert_eq!(me.display_name, "\u{a7}9Me\u{a7}r");
        assert_eq!((me.health, me.team_color), (13.0, Some(0x5555ff)));
        assert_eq!(me.effects.as_slice()[0].id, 5);
        assert_eq!(me.held_item.map(|held| held.id), Some(276));
        assert_eq!(me.flags & UMI_ENTITY_VISIBLE, 0);
        assert_ne!(me.flags & UMI_ENTITY_TEAMMATE, 0);
        assert_eq!(me.position, [4.0, 64.0, 0.0]);
        assert!(builder.snapshot().is_empty(), "never in the list");
    }

    fn mods_of(records: &[EntityRecord]) -> Vec<ModEntity> {
        let (board, hud) = (Scoreboard::new(), HudTracker::default());
        let sources = bare_sources(&board, &hud);
        records
            .iter()
            .map(|record| ModEntity::of(record, &sources, &mut String::new()))
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
        builder.refresh(&mods_of(&records), None, DVec3::new(0.0, 64.0, 0.0), 1.0);
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
        builder.refresh(&mods_of(&records), None, DVec3::new(0.0, 64.0, 0.0), 1.0);
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
        builder.refresh(&entities, None, eye, 0.5);
        // Half a tick in, the runner is at 15: nearer than the one at 16.
        assert_eq!(ids(&builder), [1, 2]);
        let first = builder.snapshot().get(0).unwrap();
        assert_eq!(
            first.position,
            [20.0, 64.0, 0.0],
            "tick facts are the tick's"
        );
        assert_eq!(first.render_position, [15.0, 64.0, 0.0]);
        builder.refresh(&entities, None, eye, 1.0);
        assert_eq!(ids(&builder), [2, 1]);
    }

    #[test]
    fn refreshing_again_reuses_the_buffers() {
        let records: Vec<EntityRecord> = (0..300)
            .map(|id| record(id, EntityKind::Item, f64::from(id)))
            .collect();
        let entities = mods_of(&records);
        let mut builder = SnapshotBuilder::default();
        builder.refresh(&entities, None, DVec3::ZERO, 0.0);
        let order = builder.order.capacity();
        builder.refresh(&entities, None, DVec3::ZERO, 0.5);
        assert_eq!(builder.order.capacity(), order);
    }

    #[test]
    fn clearing_makes_the_snapshot_unavailable() {
        let mut builder = SnapshotBuilder::default();
        builder.refresh(&[], None, DVec3::ZERO, 0.0);
        assert!(builder.snapshot().is_available() && builder.snapshot().is_empty());
        builder.clear();
        assert!(!builder.snapshot().is_available());
    }
}
