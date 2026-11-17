// SPDX-License-Identifier: MIT OR Apache-2.0

//! The state the game thread owns, and the questions asked of it.

use std::collections::BTreeSet;

use glam::DVec3;
use umicraft_assets::profile_textures::{AccountSkinFetcher, ProfileTextures};
use umicraft_game::entity::{EntityId, EntityKind, EntityRecord, EntityStore};
use umicraft_game::gamemode::GameMode;
use umicraft_game::hand::HandState;
use umicraft_game::interaction::Interaction;
use umicraft_game::inventory::creative::CreativeScreen;
use umicraft_game::inventory::WindowManager;
use umicraft_game::item_use::ItemUse;
use umicraft_game::raycast::{look_direction, raycast, RayHit};
use umicraft_game::server::ServerState;
use umicraft_game::shapes::selection_shape;
use umicraft_game::traffic::ServerView;
use umicraft_game::view::Perspective;
use umicraft_game::GameState;
use umicraft_protocol::packets::play::ItemStack;
use umicraft_render::{HudState, OutlineBox};
use umicraft_world::{BlockPos, ChunkPos, World};

use super::combat;
use super::look::LookSlot;
use super::messages::GameStatus;
use crate::connection_end::ConnectionEnd;
use crate::cracks::Crack;
use crate::social_view;

pub(super) mod chat;
pub(super) mod controls;
pub(super) mod hotbar;
pub(super) mod metrics;
pub(super) mod social;
pub(super) mod vitals;

pub(super) use chat::ChatState;
pub(super) use controls::Controls;
pub(super) use hotbar::Hotbar;
pub(super) use metrics::TickMetrics;
pub(super) use social::Social;
pub(super) use vitals::Vitals;

/// The seed of the particle random source; particles are cosmetic, so a fixed
/// seed only makes a session repeatable.
const PARTICLE_SEED: u64 = 0x5041_5254;

/// The game state owned by the game thread.
pub(super) struct Game {
    /// The loaded world; `None` before `Join Game`.
    pub(super) world: Option<World>,
    /// The client-side particles (P8).
    pub(super) particles: umicraft_game::particles::ParticleEngine,
    /// The ambient sources of the particles (blocks, rain, sprint dust).
    pub(super) ambient: crate::particle_ambient::AmbientSpawner,
    /// How many particles are created (`GameSettings.particleSetting`).
    pub(super) particle_setting: umicraft_game::particles::ParticleSetting,
    /// What the screen overlays need to know of the player (P8-f).
    pub(super) overlays: crate::overlay_state::OverlayTracker,
    /// The chunks whose geometry changed and still need a snapshot.
    pub(super) dirty: BTreeSet<ChunkPos>,
    /// The human-readable status.
    pub(super) status: String,
    /// Why the session ended, once the network thread said so.
    pub(super) end: Option<ConnectionEnd>,
    /// The entity id of the local player.
    pub(super) entity_id: i32,
    /// The gamemode of the local player.
    pub(super) gamemode: GameMode,
    /// The lookup of the skin and cape of the account at Mojang, when the
    /// session asked for one.
    pub(super) account_skin: Option<AccountSkinFetcher>,
    /// What the lookup found: worn by the local player for what the server
    /// does not send.
    pub(super) account_textures: Option<ProfileTextures>,
    /// The current dimension; `0` is the overworld.
    pub(super) dimension: i8,
    /// Whether the world is a superflat one, from the world type the server
    /// names in `Join Game` and in the `Respawn` that changes the dimension.
    pub(super) flat_world: bool,
    /// The latest signed time of day from `Time Update`.
    ///
    /// It is never advanced locally: the server sends updates regularly and a
    /// negative value means the clock is frozen.
    pub(super) world_time: i64,
    /// Whether the server is in the play state.
    pub(super) playing: bool,
    /// The world spawn point, as the server announced it.
    pub(super) spawn: Option<(i32, i32, i32)>,
    /// The player state; `None` before the first placement.
    pub(super) state: Option<GameState>,
    /// The view angle in degrees, never wrapped (MCP `rotationYaw`,
    /// ADR-0076); yaw zero looks towards +Z (south).
    pub(super) yaw: f32,
    /// The view angle in degrees; a positive pitch looks down.
    pub(super) pitch: f32,
    /// The keys held and the buttons pressed since the last tick.
    pub(super) controls: Controls,
    /// The health, food, armour and experience of the local player.
    pub(super) vitals: Vitals,
    /// The selected hotbar slot, what the server was told of it, and the key
    /// presses the next tick takes.
    pub(super) hotbar: Hotbar,
    /// The chat history, its visibility and the tab completions.
    pub(super) chat: ChatState,
    /// What the server was last told about the player's movement.
    pub(super) server_view: ServerView,
    /// The vehicle the player rides (`Attach Entity`), `None` on foot.
    pub(super) vehicle: Option<i32>,
    /// The dig, placement and acknowledgement rules.
    pub(super) interaction: Interaction,
    /// The use of the held item (eating, blocking, drawing a bow).
    pub(super) item_use: ItemUse,
    /// Where the camera sits (`F5`).
    pub(super) perspective: Perspective,
    /// The animation state of the first-person hand.
    pub(super) hand: HandState,
    /// The body of the local player, animated like any other player; drawn
    /// in the third-person perspectives.
    pub(super) body: EntityRecord,
    /// Where the mouse look is published to the render thread.
    pub(super) look: LookSlot,
    /// How many server placements (`Player Position and Look`) were applied;
    /// every [`Frame`](super::Frame) carries it so the render thread can
    /// tell that it missed a [`RenderEvent::Teleport`](super::RenderEvent).
    pub(super) placements: u32,
    /// The statistics of the session.
    pub(super) status_data: GameStatus,
    /// Whether the HUD has to be published again.
    pub(super) hud_dirty: bool,
    /// The bursts of critical stars still going (`EntityParticleEmitter`).
    pub(super) hit_emitters: Vec<umicraft_game::combat::HitEmitter>,
    /// The collision boxes of the boats, gathered before each tick; the
    /// buffer is reused so the tick does not allocate.
    pub(super) solid_entities: Vec<umicraft_game::collision::Aabb>,
    /// The boxes of the entities that keep a placed block out, gathered on
    /// a use click; the buffer is reused.
    pub(super) blocking_entities: Vec<umicraft_game::collision::Aabb>,
    /// The status rows, bars and held-item name of the HUD.
    pub(super) hud_view: crate::hud_view::HudTracker,
    /// Entities in the world (mobs, players, objects).
    pub(super) entities: EntityStore,
    /// The inventory and container window model.
    pub(super) windows: WindowManager,
    /// A book the player used this tick, whose screen the window opens.
    pub(super) pending_book: Option<ItemStack>,
    /// The player's own name; it picks the team sidebar of the scoreboard and
    /// the team rules of name tags.
    pub(super) own_name: String,
    /// The tick-driven parity scenario and its state log (phase E.2);
    /// `None` outside `--script e1`.
    pub(super) e1: Option<crate::e1::E1Run>,
    /// The tab list, the scoreboard, the title, the action bar and the boss bar.
    pub(super) social: Social,
    /// The state of the creative inventory screen.
    pub(super) creative: CreativeScreen,
    /// The session state kept for later consumers (ADR-0020 section 3).
    pub(super) server: ServerState,
    /// The timings the F3 screen shows: tick, network and input times, TPS.
    pub(super) metrics: TickMetrics,
}

impl Game {
    pub(super) fn new() -> Self {
        Self {
            world: None,
            particles: umicraft_game::particles::ParticleEngine::new(PARTICLE_SEED),
            ambient: crate::particle_ambient::AmbientSpawner::default(),
            particle_setting: umicraft_game::particles::ParticleSetting::default(),
            overlays: crate::overlay_state::OverlayTracker::default(),
            dirty: BTreeSet::new(),
            status: "CONNECTING".to_string(),
            end: None,
            entity_id: 0,
            gamemode: GameMode::Survival,
            account_skin: None,
            account_textures: None,
            dimension: 0,
            flat_world: false,
            world_time: 0,
            playing: false,
            spawn: None,
            state: None,
            yaw: 0.0,
            pitch: 0.0,
            controls: Controls::default(),
            vitals: Vitals::default(),
            hotbar: Hotbar::default(),
            chat: ChatState::default(),
            server_view: ServerView::default(),
            vehicle: None,
            interaction: Interaction::new(),
            item_use: ItemUse::new(),
            perspective: Perspective::default(),
            hand: HandState::new(),
            body: EntityRecord::new(EntityId(0), EntityKind::Player, DVec3::ZERO, 0.0, 0.0),
            look: LookSlot::default(),
            placements: 0,
            status_data: GameStatus::default(),
            hud_dirty: true,
            hit_emitters: Vec::new(),
            solid_entities: Vec::new(),
            blocking_entities: Vec::new(),
            hud_view: crate::hud_view::HudTracker::default(),
            entities: EntityStore::new(),
            windows: WindowManager::new(),
            pending_book: None,
            own_name: String::new(),
            e1: None,
            social: Social::default(),
            creative: CreativeScreen::new(),
            server: ServerState::new(),
            metrics: TickMetrics::default(),
        }
    }

    /// The reach of the block interaction, in blocks.
    ///
    /// MCP `PlayerControllerMP.java:340-343`: 4.5 blocks in survival and 5.0
    /// in creative.
    pub(super) fn reach(&self) -> f64 {
        self.gamemode.reach()
    }

    /// Whether the local player is dead (MCP `EntityLivingBase.getHealth() <= 0`).
    pub(super) fn dead(&self) -> bool {
        self.vitals.dead()
    }

    /// The entity the view follows (`Camera`, 0x43), if it is another one
    /// than the local player.
    pub(super) fn camera_entity(&self) -> Option<EntityId> {
        self.server.camera_target().map(EntityId)
    }

    /// True while the view follows another entity.
    pub(super) fn spectating(&self) -> bool {
        self.camera_entity()
            .is_some_and(|id| self.entities.camera_view(id, 1.0).is_some())
    }

    /// The eye position of the player.
    pub(super) fn eye(&self) -> Option<DVec3> {
        self.state
            .as_ref()
            .map(|state| state.player().eye_position())
    }

    /// The unit vector the player looks along. The game keeps degrees
    /// (ADR-0076); the ray takes radians.
    pub(super) fn look(&self) -> DVec3 {
        look_direction(self.yaw.to_radians(), self.pitch.to_radians())
    }

    /// The block the crosshair points at, entities aside.
    pub(super) fn block_target(&self) -> Option<RayHit> {
        let eye = self.eye()?;
        raycast(eye, self.look(), self.reach(), self.world.as_ref()?)
    }

    /// The block the crosshair points at: none when an entity is nearer
    /// (MCP `objectMouseOver`, see [`combat::pointed`]).
    pub(super) fn target(&self) -> Option<RayHit> {
        combat::pointed(self).block()
    }

    /// The boxes of the selection outline of the targeted block.
    ///
    /// MCP `EntityRenderer.isDrawBlockOutline` hides the outline for a
    /// spectator unless a container is targeted (not modelled: spectators
    /// never get an outline) and, without `allowEdit` (adventure), unless the
    /// held item's `CanDestroy`/`CanPlaceOn` lists the block (item NBT is not
    /// read yet: adventure never gets an outline). `RenderGlobal.drawSelectionBox`
    /// draws the block's selected bounding box, offset to its position.
    pub(super) fn selection_outline(&self) -> Vec<OutlineBox> {
        if matches!(self.gamemode, GameMode::Spectator | GameMode::Adventure) {
            return Vec::new();
        }
        let (Some(hit), Some(world)) = (self.target(), self.world.as_ref()) else {
            return Vec::new();
        };
        let origin = DVec3::new(
            f64::from(hit.block.x),
            f64::from(hit.block.y),
            f64::from(hit.block.z),
        );
        let state = world.block(hit.block);
        let shape = selection_shape(state, world, hit.block);
        shape
            .boxes()
            .iter()
            .map(|aabb| OutlineBox {
                min: (aabb.min + origin).as_vec3().to_array(),
                max: (aabb.max + origin).as_vec3().to_array(),
            })
            .collect()
    }

    /// The cracks of the blocks being dug (`Block Break Animation`, and the
    /// local dig which is recorded the same way).
    ///
    /// A block that is already air has no shape and no cracks.
    pub(super) fn cracks(&self) -> Vec<Crack> {
        let Some(world) = self.world.as_ref() else {
            return Vec::new();
        };
        let mut cracks = Vec::new();
        for (_, progress) in self.server.block_entities().break_animations() {
            let (x, y, z) = progress.pos;
            let position = BlockPos::new(x, y, z);
            let origin = DVec3::new(f64::from(x), f64::from(y), f64::from(z));
            let shape = selection_shape(world.block(position), world, position);
            cracks.extend(shape.boxes().iter().map(|aabb| Crack {
                bounds: OutlineBox {
                    min: (aabb.min + origin).as_vec3().to_array(),
                    max: (aabb.max + origin).as_vec3().to_array(),
                },
                stage: progress.stage,
            }));
        }
        cracks
    }

    /// True when the chunk the player stands in is loaded.
    ///
    /// The simulation must not run without it: the collision solver would see
    /// air, the player would fall through the floor and the server would reject
    /// the position. Only that chunk counts (MCP `EntityPlayerSP.onUpdate`,
    /// `isBlockLoaded(posX, 0, posZ)`): a server that sends a single chunk, such
    /// as a proxy's waiting world, still sees the player fall.
    pub(super) fn world_ready(&self) -> bool {
        let (Some(world), Some(state)) = (self.world.as_ref(), self.state.as_ref()) else {
            return false;
        };
        let position = state.position();
        let chunk = ChunkPos::new(
            (position.x.floor() as i32).div_euclid(16),
            (position.z.floor() as i32).div_euclid(16),
        );
        world.chunk(chunk).is_some()
    }

    /// The HUD state of the session.
    pub(super) fn hud(&self) -> HudState {
        HudState {
            health: self.vitals.health,
            food: self.vitals.food,
            armor: self.vitals.armor,
            level: self.vitals.level,
            score: self.vitals.score,
            selected_slot: self.hotbar.selected,
            crosshair: true,
            chat: self.chat.lines.iter().cloned().collect(),
            action_bar: social_view::action_bar_view(&self.social.action_bar),
            title: social_view::title_view(&self.social.title),
            sidebar: social_view::sidebar_view(&self.social.scoreboard, &self.own_name),
            tab_list: social_view::tab_list_view(
                &self.social.player_list,
                &self.social.scoreboard,
                true,
            ),
            show_tab_list: false,
            completions: self.chat.completions.clone(),
            completions_serial: self.chat.completions_serial,
            boss: social_view::boss_view(&self.social.boss_bar),
            vitals: self.hud_view.views().vitals.clone(),
            bar: self.hud_view.views().bar,
            held_name: self.hud_view.views().held_name.clone(),
            effects: self.hud_view.views().effects.clone(),
            ping_ms: self.local_ping_ms(),
            ..HudState::default()
        }
    }

    /// The latency in the local player's own tab list entry, which is how the
    /// server reports it (MCP `NetworkPlayerInfo.getResponseTime`). A negative
    /// value is not a measure.
    pub(super) fn local_ping_ms(&self) -> Option<u32> {
        let entry = self.social.player_list.by_name(&self.own_name)?;
        u32::try_from(entry.ping).ok()
    }
}
