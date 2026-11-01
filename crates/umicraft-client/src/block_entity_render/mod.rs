// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tile-entity rendering: the blocks that vanilla draws with a renderer
//! instead of a block model (MCP `TileEntityRendererDispatcher`, P7c).
//!
//! The chunk mesher hands the render thread one
//! [`BlockEntityRef`] per chest and sign. Each frame this module turns the
//! ones in range into entity vertices grouped by texture, which the entity
//! pass draws like any mob.
//!
//! - `chest`: chests, trapped chests, ender chests, single and double, with the
//!   lid angle of the `Block Action` state.
//! - `sign`: standing and wall signs with their four lines of text.
//! - `banner`: standing and wall banners, textured by `banner_texture`.
//! - `skull`: placed skulls, with the owner's skin for a player skull.
//! - `item`: skull and banner items, drawn with the same models (`builtin/entity`).

mod additive;
mod banner;
mod banner_patterns;
mod banner_texture;
mod beacon;
mod book;
mod book_state;
mod chest;
mod end_portal;
mod flower_pot;
mod item;
mod sign;
mod skull;
mod spawner;

use std::collections::HashMap;

use glam::Vec3;
use umicraft_game::server::block_entities::{EntityTable, SignTable};
use umicraft_game::server::chest_lids::ChestLids;
use umicraft_mesh::{BlockEntityKind, BlockEntityRef};
use umicraft_model::entity::{ModelVertex, TextureSlot};
use umicraft_render::EntityVertex;
use umicraft_ui::FontAtlas;
use umicraft_world::LightLevel;

pub(crate) use flower_pot::push_pot_plants;
pub(crate) use item::{entity_item_mesh, is_entity_item};

use crate::entity_render::SkinLookup;
pub(crate) use banner_texture::{compose as compose_banner, BannerDesign, Image};

/// Squared distance beyond which a tile entity is not drawn
/// (`TileEntity.getMaxRenderDistanceSquared`, 64 blocks).
const MAX_RENDER_DISTANCE_SQUARED: f32 = 4096.0;
/// The same for a beacon, whose beam is seen from afar
/// (`TileEntityBeacon.getMaxRenderDistanceSquared`, 256 blocks).
const BEACON_RENDER_DISTANCE_SQUARED: f32 = 65_536.0;

/// What the renderers need besides the block itself.
pub(crate) struct BlockEntityScene<'a> {
    /// The lid angles of the chests.
    pub lids: &'a ChestLids,
    /// The text of the signs.
    pub signs: &'a SignTable,
    /// The last update of the other block entities (skull type, banner
    /// patterns, beacon levels, spawner mob, flower pot contents).
    pub data: &'a EntityTable,
    /// The downloaded skins, for the player skulls.
    pub skins: &'a dyn SkinLookup,
    /// Seconds since the client started, for the animations that do not
    /// follow the world clock (the beam, the book).
    pub seconds: f32,
    /// The font the sign text is laid out with, `None` before it is loaded.
    /// It must be the atlas `EntityTextures::set_sign_font` was given, since
    /// the glyph UVs address that texture.
    pub font: Option<&'a FontAtlas>,
    /// Interpolation between two ticks.
    pub partial_tick: f32,
    /// The camera, for the distance cut.
    pub camera: Vec3,
}

/// Vertices grouped by the texture they sample.
pub(crate) type Batches = HashMap<TextureSlot, Vec<EntityVertex>>;

/// The banner designs of a frame by the slot their vertices use; the texture
/// cache composes the ones it has not got.
pub(crate) type BannerDesigns = HashMap<TextureSlot, BannerDesign>;

/// What the tile entities of a frame draw.
#[derive(Debug, Default)]
pub(crate) struct BlockEntityMeshes {
    /// Lit and opaque: chests, signs, skulls, banners.
    pub solid: Batches,
    /// Blended over the world: the beacon beam, the end portal.
    pub additive: Batches,
    /// The textures the banners of `solid` need.
    pub banners: BannerDesigns,
}

/// The vertices of every tile entity of `entries` in range, by texture.
#[must_use]
pub(crate) fn mesh_block_entities<'a>(
    entries: impl Iterator<Item = &'a BlockEntityRef>,
    scene: &BlockEntityScene<'_>,
) -> BlockEntityMeshes {
    let mut meshes = BlockEntityMeshes::default();
    let batches = &mut meshes.solid;
    let banners = &mut meshes.banners;
    let additive = &mut meshes.additive;
    for entry in entries.filter(|entry| in_range(entry, scene.camera)) {
        match BlockEntityKind::of(entry.state) {
            Some(BlockEntityKind::Beacon) => beacon::add_beam(additive, entry, scene),
            Some(BlockEntityKind::EndPortal) => end_portal::add_end_portal(additive, entry, scene),
            Some(
                BlockEntityKind::Chest
                | BlockEntityKind::TrappedChest
                | BlockEntityKind::EnderChest,
            ) => chest::add_chest(batches, entry, scene),
            Some(BlockEntityKind::StandingSign | BlockEntityKind::WallSign) => {
                sign::add_sign(batches, entry, scene);
            }
            Some(BlockEntityKind::StandingBanner | BlockEntityKind::WallBanner) => {
                banner::add_banner(batches, banners, entry, scene);
            }
            Some(BlockEntityKind::Skull) => skull::add_skull(batches, entry, scene),
            Some(BlockEntityKind::Spawner) => spawner::add_spawner(batches, entry, scene),
            Some(BlockEntityKind::EnchantingTable) => book::add_book(batches, entry, scene),
            // The plant is added to the terrain mesh, see `push_pot_plants`.
            Some(BlockEntityKind::FlowerPot) | None => {}
        }
    }
    meshes
}

/// Whether the block is near enough to draw.
fn in_range(entry: &BlockEntityRef, camera: Vec3) -> bool {
    let [x, y, z] = entry.position;
    let centre = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
    let limit = if BlockEntityKind::of(entry.state) == Some(BlockEntityKind::Beacon) {
        BEACON_RENDER_DISTANCE_SQUARED
    } else {
        MAX_RENDER_DISTANCE_SQUARED
    };
    centre.distance_squared(camera) <= limit
}

/// The packed light word of a tile entity's vertices (`block | sky << 4`).
fn light_word(entry: &BlockEntityRef) -> u32 {
    packed_light(entry.light)
}

/// The packed light word of `light` (`block | sky << 4`).
fn packed_light(light: LightLevel) -> u32 {
    u32::from(light.block.min(15)) | (u32::from(light.sky.min(15)) << 4)
}

/// Moves model-space vertices to the block at `entry` and appends them.
fn push_at_block(out: &mut Vec<EntityVertex>, vertices: Vec<ModelVertex>, entry: &BlockEntityRef) {
    let [x, y, z] = entry.position;
    let origin = Vec3::new(x as f32, y as f32, z as f32);
    let light = light_word(entry);
    out.extend(vertices.into_iter().map(|vertex| EntityVertex {
        position: (Vec3::from(vertex.position) + origin).to_array(),
        normal: vertex.normal,
        uv: vertex.uv,
        light,
    }));
}
