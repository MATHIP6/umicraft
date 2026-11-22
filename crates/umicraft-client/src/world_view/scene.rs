// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the newest frame shows of the world: entities, particles, overlays,
//! block entities and the facts of the debug screens.

use std::sync::Arc;
use std::time::Instant;

use umicraft_game::entity::{wrap_degrees, EntityId, EntityRenderSnapshot};
use umicraft_game::gamemode::GameMode;
use umicraft_game::piston::MovingBlocks;
use umicraft_game::server::block_entities::EntityTable;
use umicraft_game::server::block_entities::SignTable;
use umicraft_game::server::chest_lids::ChestLids;
use umicraft_game::server::maps::MapStore;
use umicraft_render::{BorderView, FogMedium, OutlineBox, PrecipitationField, ScreenOverlays};
use umicraft_world::LightLevel;

use super::WorldView;
use crate::scene::overlay::{compose, SpriteLayers, ViewFacts};

impl WorldView {
    /// The columns rain and snow fall on in the newest frame (empty while it
    /// does not rain), for `Renderer::set_precipitation`.
    #[must_use]
    pub fn precipitation(&self) -> Arc<PrecipitationField> {
        Arc::clone(&self.view.weather.precipitation)
    }

    /// The id of the biome at the player in the newest frame, `None` while
    /// its column is not loaded (the custom sky filters on it).
    #[must_use]
    pub fn sky_biome(&self) -> Option<u8> {
        self.view.sky_biome
    }

    /// How far through its life the elder guardian apparition is at
    /// `partial_ticks` into the tick (`0..=1`), `None` while there is none
    /// (MCP `MobAppearance.renderParticle`: `(age + partial) / maxAge`).
    #[must_use]
    pub fn mob_appearance(&self, partial_ticks: f32) -> Option<f32> {
        crate::scene::particles::mob_appearance(&self.view.particles, partial_ticks)
    }

    /// The quads of the newest frame's particles at `partial_ticks` into the
    /// tick, for `Renderer::set_particles`. Block fragments find their sprite
    /// in the bound terrain, the pieces of an item in `items`; the procedural
    /// terrain has neither, so they are left out. The centres are relative to
    /// `origin`, the render origin of the camera (F2-1b).
    pub fn particle_sprites_into(
        &self,
        out: &mut Vec<umicraft_render::ParticleSprite>,
        partial_ticks: f32,
        origin: glam::DVec3,
        items: Option<&crate::resources::item_icons::ItemResources>,
    ) {
        let context = self.mesher.model_context();
        crate::scene::particles::sprites_into(
            out,
            &self.view.particles,
            partial_ticks,
            origin,
            &crate::scene::particles::Pictures {
                blocks: |state| crate::scene::particles::block_sprite(context?, state),
                items: |id, damage| items?.particle_layer(id, damage),
            },
        );
    }

    /// The full-screen overlays of the frame at `now`, for
    /// `Renderer::set_screen_overlays`. `vignette` says whether the vignette is
    /// drawn (the Vignette option resolved against Graphics).
    pub fn screen_overlays(&mut self, now: Instant, vignette: bool) -> ScreenOverlays {
        let Some(state) = self.view.sample(now) else {
            return ScreenOverlays::default();
        };
        let (inputs, fog) = (self.view.overlays, self.view.fog);
        let facts = ViewFacts {
            underwater: fog.medium == FogMedium::Water,
            brightness: fog.light_brightness,
            yaw_degrees: state.yaw.to_degrees(),
            pitch_degrees: state.pitch.to_degrees(),
            partial_tick: self.view.partial_tick(now),
        };
        let vignette = vignette.then(|| {
            self.view
                .vignette
                .step(fog.light_brightness, inputs.border_danger)
        });
        let layers = SpriteLayers::find(self.mesher.model_context(), inputs.in_wall);
        compose(&inputs, &facts, layers, vignette)
    }

    /// The world border of the newest frame; `None` before the first one.
    #[must_use]
    pub fn border(&self) -> Option<BorderView> {
        self.view.border
    }

    /// The dimension and the spawn point of the newest frame, for the compass.
    #[must_use]
    pub fn compass_world(&self) -> (i8, Option<(i32, i32, i32)>) {
        (self.view.dimension, self.view.spawn)
    }

    /// The dimension and game mode of the newest frame, which pick the music.
    #[must_use]
    pub fn music_world(&self) -> (i8, GameMode) {
        (self.view.dimension, self.view.gamemode)
    }

    /// The world facts of the F3 screen from the newest frame.
    #[must_use]
    pub fn debug_world(&self) -> &crate::debug::world::DebugWorld {
        &self.view.debug
    }

    /// The entities and particles drawn in the newest frame, for F3.
    #[must_use]
    pub fn drawn_counts(&self) -> (usize, usize) {
        (self.entities.len(), self.view.particles.len())
    }

    /// The lid angles of the chests in the newest frame.
    #[must_use]
    pub fn chest_lids(&self) -> &ChestLids {
        &self.chest_lids
    }

    /// The text of the signs of the newest frame.
    #[must_use]
    pub fn signs(&self) -> &SignTable {
        &self.signs
    }

    /// The updates of the skulls, banners, beacons, spawners and flower pots.
    #[must_use]
    pub fn block_entity_data(&self) -> &EntityTable {
        &self.block_entity_data
    }

    /// The blocks pistons are moving in the newest frame.
    #[must_use]
    pub fn moving_blocks(&self) -> &MovingBlocks {
        &self.moving_blocks
    }

    /// The simulation frames received so far.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames
    }

    /// Interpolated entity render snapshots for the current render instant.
    ///
    /// Uses the same partial tick fraction as the camera so entities and the
    /// world stay in sync. Every position is relative to `origin`, the render
    /// origin of the camera (F2-1b).
    #[must_use]
    pub fn entities(&self, now: Instant, origin: glam::DVec3) -> Vec<EntityRenderSnapshot> {
        let mut out = Vec::with_capacity(self.entities.len());
        self.entities_into(now, origin, &mut out);
        out
    }

    /// Interpolates entity render snapshots into a reusable `out` buffer,
    /// retaining allocated capacity across frames.
    pub fn entities_into(
        &self,
        now: Instant,
        origin: glam::DVec3,
        out: &mut Vec<EntityRenderSnapshot>,
    ) {
        out.clear();
        out.reserve(self.entities.len());
        let alpha = self.view.partial_tick(now);
        out.extend(self.entities.iter().map(|entity| {
            let mut snapshot = entity.interpolate(alpha, origin);
            // `Entity.setAngles` shifts the pitch and its previous value
            // together every frame, so what `RendererLivingEntity` draws
            // is the pitch of the mouse, not one interpolated over a tick.
            if snapshot.local {
                if self.view.current.is_some() {
                    let live_yaw = self.view.player_yaw.to_degrees();
                    snapshot.yaw = live_yaw;
                    snapshot.head_yaw = live_yaw;
                    let diff = wrap_degrees(live_yaw - snapshot.body_yaw);
                    let twist = diff.clamp(-75.0, 75.0);
                    snapshot.body_yaw = wrap_degrees(live_yaw - twist);
                    if diff * diff > 2500.0 {
                        snapshot.body_yaw = wrap_degrees(snapshot.body_yaw + twist * 0.2);
                    }
                }
                if let Some(pitch) = self.view.body_pitch {
                    snapshot.pitch = pitch;
                }
            }
            snapshot
        }));
    }

    /// A tracked entity at the newest tick in world coordinates, for a
    /// portrait such as the horse of a horse window.
    #[must_use]
    pub fn entity_body(&self, id: EntityId) -> Option<EntityRenderSnapshot> {
        self.entities
            .iter()
            .find(|entity| entity.id == id)
            .map(|entity| entity.interpolate(1.0, glam::DVec3::ZERO))
    }

    /// The body of the local player at the newest tick, once a frame arrived.
    #[must_use]
    pub fn local_body(&self) -> Option<EntityRenderSnapshot> {
        self.local_body.as_ref().map(|body| {
            let mut snapshot = body.interpolate(1.0, glam::DVec3::ZERO);
            if self.view.current.is_some() {
                let live_yaw = self.view.player_yaw.to_degrees();
                snapshot.yaw = live_yaw;
                snapshot.head_yaw = live_yaw;
                let diff = wrap_degrees(live_yaw - snapshot.body_yaw);
                let twist = diff.clamp(-75.0, 75.0);
                snapshot.body_yaw = wrap_degrees(live_yaw - twist);
                if diff * diff > 2500.0 {
                    snapshot.body_yaw = wrap_degrees(snapshot.body_yaw + twist * 0.2);
                }
            }
            if let Some(pitch) = self.view.body_pitch {
                snapshot.pitch = pitch;
            }
            snapshot
        })
    }

    /// The light that lights the first-person item in the newest frame.
    #[must_use]
    pub fn hand_light(&self) -> LightLevel {
        self.hand_light
    }

    /// The world-space boxes of the block selection outline in the newest
    /// frame; empty when no block is targeted.
    #[must_use]
    pub fn selection(&self) -> &[OutlineBox] {
        &self.selection
    }

    /// The maps the server sent, as of the newest frame.
    #[must_use]
    pub fn maps(&self) -> &Arc<MapStore> {
        &self.maps
    }

    /// The number of active living entities in the newest frame.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }
}
