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

mod chest;
mod sign;

use std::collections::HashMap;

use glam::Vec3;
use umicraft_game::server::block_entities::{EntityTable, SignTable};
use umicraft_game::server::chest_lids::ChestLids;
use umicraft_mesh::{BlockEntityKind, BlockEntityRef};
use umicraft_model::entity::{ModelVertex, TextureSlot};
use umicraft_render::EntityVertex;
use umicraft_ui::FontAtlas;

/// Squared distance beyond which a tile entity is not drawn
/// (`TileEntity.getMaxRenderDistanceSquared`, 64 blocks).
const MAX_RENDER_DISTANCE_SQUARED: f32 = 4096.0;

/// What the renderers need besides the block itself.
pub(crate) struct BlockEntityScene<'a> {
    /// The lid angles of the chests.
    pub lids: &'a ChestLids,
    /// The text of the signs.
    pub signs: &'a SignTable,
    /// The last update of the other block entities (skull type, banner
    /// patterns, beacon levels, spawner mob, flower pot contents).
    pub data: &'a EntityTable,
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

/// What the tile entities of a frame draw.
#[derive(Debug, Default)]
pub(crate) struct BlockEntityMeshes {
    /// Lit and opaque: chests, signs, skulls, banners.
    pub solid: Batches,
    /// Blended over the world: the beacon beam, the end portal.
    pub additive: Batches,
}

/// The vertices of every tile entity of `entries` in range, by texture.
#[must_use]
pub(crate) fn mesh_block_entities<'a>(
    entries: impl Iterator<Item = &'a BlockEntityRef>,
    scene: &BlockEntityScene<'_>,
) -> BlockEntityMeshes {
    let mut meshes = BlockEntityMeshes::default();
    let batches = &mut meshes.solid;
    for entry in entries.filter(|entry| in_range(entry, scene.camera)) {
        match BlockEntityKind::of(entry.state) {
            Some(
                BlockEntityKind::Chest
                | BlockEntityKind::TrappedChest
                | BlockEntityKind::EnderChest,
            ) => chest::add_chest(batches, entry, scene),
            Some(BlockEntityKind::StandingSign | BlockEntityKind::WallSign) => {
                sign::add_sign(batches, entry, scene);
            }
            // Drawn by the renderers P7d adds, one per kind.
            Some(
                BlockEntityKind::Spawner
                | BlockEntityKind::EnchantingTable
                | BlockEntityKind::EndPortal
                | BlockEntityKind::Beacon
                | BlockEntityKind::FlowerPot
                | BlockEntityKind::Skull
                | BlockEntityKind::StandingBanner
                | BlockEntityKind::WallBanner,
            )
            | None => {}
        }
    }
    meshes
}

/// Whether the block is near enough to draw.
fn in_range(entry: &BlockEntityRef, camera: Vec3) -> bool {
    let [x, y, z] = entry.position;
    let centre = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
    centre.distance_squared(camera) <= MAX_RENDER_DISTANCE_SQUARED
}

/// The packed light word of a tile entity's vertices (`block | sky << 4`).
fn light_word(entry: &BlockEntityRef) -> u32 {
    u32::from(entry.light.block.min(15)) | (u32::from(entry.light.sky.min(15)) << 4)
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
