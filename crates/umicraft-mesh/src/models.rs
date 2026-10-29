// SPDX-License-Identifier: MIT OR Apache-2.0

//! Chunk meshing from baked block models and the texture atlas (M5).
//!
//! [`mesh_chunk_with_models`] walks the same 16×256×16 column as
//! [`mesh_chunk`](crate::mesh_chunk), but takes the geometry from the models
//! that [`umicraft_model`] baked from the 1.8 assets:
//!
//! - a state with a baked model emits the quads of one weighted variant,
//!   chosen deterministically from the world position;
//! - a quad with a `cullface` is skipped when the neighbour in that direction
//!   is a full opaque cube or the same transparent/liquid state, the rule
//!   frozen in ADR-0007 and shared with the legacy mesher;
//! - a state without a baked model emits the legacy procedural cube, so
//!   technical and unknown ids still render instead of opening holes. Its
//!   texture is the real water/lava atlas layer when available; every other
//!   no-model state uses the missing-texture checker (or the procedural layer
//!   when no real atlas is present).
//!
//! Every quad becomes four [`Vertex`]es and two triangles, like the legacy
//! path. Its texture layer comes from [`AtlasLayout`]; a key the atlas does
//! not contain falls back to layer 0, the missing-texture checker. A face
//! whose model entry carries `tintindex` samples the biome colour table from
//! [`ModelMeshContext::tints`] (ADR-0009).
//!
//! References (ADR-0001): the 1.8 client selects the weighted model from the
//! block position in `BlockRendererDispatcher.getModelFromBlockState` via
//! `WeightedBakedModel.getAlternativeModel`, which reduces the
//! `MathHelper.getCoordinateRandom` position hash modulo the total weight.
//! The cullface rule is the one frozen in ADR-0007 (it is not
//! `Block.shouldSideBeRendered`).

use std::sync::Arc;

use umicraft_model::{AtlasLayout, BlockModel, BlockModelSet, ModelVariant};
use umicraft_world::fluid::Liquid;
use umicraft_world::{BlockState, ChunkPos};

use crate::fluid::{self, FluidLayers};

use crate::{
    emit_legacy_faces_with, is_transparent_pass, occludes, quad_light, tint, BlockSource, Mesh,
    Vertex,
};

/// The baked models and the texture layers the model mesher reads.
///
/// Both halves must come from the same client installation: the set bakes
/// concrete texture keys (`blocks/stone`) and the atlas assigns their layers.
/// The client keeps one context and shares its `Arc`s with every meshing
/// worker, so building a context never touches the disk.
///
/// `tints` is the optional biome-colour table built from the two runtime
/// colormaps (ADR-0009). `None` keeps every face white, which is the M5
/// behaviour when the installation has no colormaps.
#[derive(Debug, Clone)]
pub struct ModelMeshContext {
    /// Baked block models keyed by wire state.
    pub models: Arc<BlockModelSet>,
    /// Texture-key to layer mapping of the texture array.
    pub atlas: Arc<AtlasLayout>,
    /// Per-biome grass/foliage/water colours, or `None` for untinted output.
    pub tints: Option<Arc<tint::BiomeTints>>,
}

/// Builds the mesh for one chunk column from baked block models.
///
/// Air is skipped. A state with a baked model emits the model's quads with
/// the atlas layer of each `ModelQuad::texture`; its weighted variant is
/// picked from the world position, so a given block always looks the same and
/// neighbouring blocks of one ore/granite variant differ. A `cullface` quad
/// is omitted when the neighbour occludes it (full opaque cube, or the same
/// transparent/liquid state). A state without a model takes the legacy cube
/// path of [`mesh_chunk`](crate::mesh_chunk), which also covers technical and
/// unknown ids.
///
/// `source` must answer queries in world coordinates and cover at least the
/// 3×3 chunk neighbourhood; [`ChunkSnapshot`](crate::ChunkSnapshot) does this.
/// The function allocates nothing beyond the returned [`Mesh`] vectors and
/// performs no parsing.
#[must_use]
pub fn mesh_chunk_with_models<S: BlockSource + ?Sized>(
    pos: ChunkPos,
    source: &S,
    context: &ModelMeshContext,
) -> Mesh {
    let mut mesh = Mesh::default();
    let base_x = pos.x * 16;
    let base_z = pos.z * 16;

    for y in 0..256i32 {
        for z in 0..16i32 {
            for x in 0..16i32 {
                let wx = base_x + x;
                let wz = base_z + z;
                let state = source.block(wx, y, wz);
                // Air is the overwhelming majority of a column: checking it
                // before the model lookup keeps the hot path cheap.
                if state.is_air() {
                    continue;
                }
                let position = [wx, y, wz];
                if let Some(liquid) = Liquid::of(state) {
                    let tint = context.tints.as_deref().map_or(tint::WHITE, |tints| {
                        tint::biome_tint(source, tint::tint_class(state), wx, wz, tints)
                    });
                    let layers = fluid_layers(liquid, &context.atlas);
                    fluid::emit_fluid(&mut mesh, source, position, liquid, layers, tint);
                    continue;
                }
                let variant = context
                    .models
                    .model(state)
                    .and_then(|model| select_variant(model, position));
                if let Some(variant) = variant {
                    emit_model_quads(&mut mesh, source, context, state, variant, position);
                } else if state.is_renderable() {
                    let tint = context.tints.as_deref().map_or(tint::WHITE, |tints| {
                        tint::biome_tint(source, tint::tint_class(state), wx, wz, tints)
                    });
                    // No model and not a liquid: the checker, never a
                    // procedural layer index (ADR-0012).
                    let missing = context.atlas.missing_layer();
                    emit_legacy_faces_with(&mut mesh, source, [wx, y, wz], state, tint, |_| {
                        missing
                    });
                }
            }
        }
    }
    mesh
}

/// The still and flowing layers of a liquid in the atlas
/// (`BlockFluidRenderer.initAtlasSprites`), or the checker when the atlas
/// lacks them.
fn fluid_layers(liquid: Liquid, atlas: &AtlasLayout) -> FluidLayers {
    let (still, flow) = match liquid {
        Liquid::Water => ("blocks/water_still", "blocks/water_flow"),
        Liquid::Lava => ("blocks/lava_still", "blocks/lava_flow"),
    };
    let layer = |key| atlas.layer(key).unwrap_or_else(|| atlas.missing_layer());
    FluidLayers {
        still: layer(still),
        flow: layer(flow),
    }
}

/// Picks the weighted variant of `model` for a block position.
///
/// The position hash and the range reduction reproduce the 1.8
/// `WeightedBakedModel.getAlternativeModel`: `(hash >> 16).abs() % total` then
/// a walk over the weights. The reference sorts the variants by weight before
/// that walk; document order is used here, which changes only which variant a
/// given position sees, not the distribution. `None` when the model has no
/// variants (the builder never bakes such a model).
fn select_variant(model: &BlockModel, position: [i32; 3]) -> Option<&ModelVariant> {
    // Almost every blockstate has a single variant; do not hash for it.
    if let [only] = model.variants.as_slice() {
        return Some(only);
    }
    let total_weight: u64 = model
        .variants
        .iter()
        .map(|variant| u64::from(variant.weight))
        .sum();
    if total_weight == 0 {
        return model.variants.first();
    }
    let hash = position_random(position);
    let mut weight = (((hash as i32) >> 16).wrapping_abs() as u64) % total_weight;
    for variant in &model.variants {
        if weight < u64::from(variant.weight) {
            return Some(variant);
        }
        weight -= u64::from(variant.weight);
    }
    None
}

/// The 1.8 `MathHelper.getCoordinateRandom(x, y, z)` position hash.
///
/// Java's `int`/`long` arithmetic wraps on overflow, hence the `wrapping_*`
/// calls; the bit pattern is the one the reference uses to seed the variant
/// choice. The hash is stable across runs and platforms.
fn position_random([x, y, z]: [i32; 3]) -> i64 {
    // `(long)(x * 3129871) ^ (long)z * 116129781L ^ (long)y`
    let mixed = i64::from(x.wrapping_mul(3_129_871))
        ^ i64::from(z).wrapping_mul(116_129_781)
        ^ i64::from(y);
    // `i = i * i * 42317861L + i * 11L`
    mixed
        .wrapping_mul(mixed)
        .wrapping_mul(42_317_861)
        .wrapping_add(mixed.wrapping_mul(11))
}

/// 1.8 `Block.EnumOffsetType`: random XZ/XYZ shift for vegetation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OffsetType {
    None,
    Xz,
    Xyz,
}

/// Returns the offset type for a block state following the 1.8 overrides:
/// `BlockTallGrass` → XYZ, `BlockFlower`/`BlockDoublePlant` → XZ.
fn offset_type(state: BlockState) -> OffsetType {
    match state.id() {
        31 => OffsetType::Xyz,
        37 | 38 | 175 => OffsetType::Xz,
        _ => OffsetType::None,
    }
}

/// 1.8 position-based random offset from `BlockModelRenderer`.
///
/// Uses the same hash as `MathHelper.getCoordinateRandom` (reused from
/// `select_variant`). XZ range: ±0.25 blocks. Y (XYZ only): −0.2..0.
fn block_offset(position: [i32; 3], kind: OffsetType) -> [f32; 3] {
    if kind == OffsetType::None {
        return [0.0; 3];
    }
    let hash = position_random(position);
    let ox = ((((hash >> 16) & 15) as f32) / 15.0 - 0.5) * 0.5;
    let oz = ((((hash >> 24) & 15) as f32) / 15.0 - 0.5) * 0.5;
    let oy = if kind == OffsetType::Xyz {
        ((((hash >> 20) & 15) as f32) / 15.0 - 1.0) * 0.2
    } else {
        0.0
    };
    [ox, oy, oz]
}

/// Emits every quad of `variant` at `position`, offset by the block position.
///
/// `state` is the block rendered by the quad; it is only needed for the
/// same-state half of the cull rule.
fn emit_model_quads<S: BlockSource + ?Sized>(
    mesh: &mut Mesh,
    source: &S,
    context: &ModelMeshContext,
    state: BlockState,
    variant: &ModelVariant,
    position: [i32; 3],
) {
    let [x, y, z] = position;
    let [ox, oy, oz] = block_offset(position, offset_type(state));
    // Tint is constant per block, so compute the 3×3 biome average at most
    // once and reuse it for every tinted quad (grass top + side overlay,
    // six leaf faces, ...). Untinted quads never sample the colormaps.
    let mut block_tint = None;
    for quad in &variant.model.quads {
        if let Some(cullface) = quad.cullface {
            let offset = cullface.offset();
            let neighbour = source.block(x + offset[0], y + offset[1], z + offset[2]);
            if occludes(neighbour, state) {
                continue;
            }
        }

        let layer = context.atlas.layer(&quad.texture).unwrap_or(0);
        let tint = if quad.tint.is_some() {
            *block_tint.get_or_insert_with(|| {
                context.tints.as_deref().map_or(tint::WHITE, |tints| {
                    tint::biome_tint(source, tint::tint_class(state), x, z, tints)
                })
            })
        } else {
            tint::WHITE
        };
        // Smooth light and AO sample the same outward plane as the legacy
        // cubes; the model's `ambientocclusion` flag and the per-quad `shade`
        // flag decide whether the corner AO applies.
        let lights = quad_light(
            source,
            position,
            quad.normal,
            &quad.positions,
            variant.model.ambient_occlusion,
            quad.shade,
        );
        let vertices = std::array::from_fn(|index| {
            let corner = quad.positions[index];
            Vertex {
                position: [
                    x as f32 + corner[0] + ox,
                    y as f32 + corner[1] + oy,
                    z as f32 + corner[2] + oz,
                ],
                normal: quad.normal,
                uv: quad.uvs[index],
                layer,
                light: lights[index].pack(),
                tint,
            }
        });
        mesh.push_quad(is_transparent_pass(state), vertices);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_model::ResolvedModel;

    fn variant(weight: u32) -> ModelVariant {
        ModelVariant {
            model: Arc::new(ResolvedModel::default()),
            weight,
        }
    }

    fn model(weights: &[u32]) -> BlockModel {
        BlockModel {
            variants: weights.iter().copied().map(variant).collect(),
        }
    }

    #[test]
    fn an_empty_model_selects_no_variant() {
        assert!(select_variant(&model(&[]), [0, 0, 0]).is_none());
    }

    #[test]
    fn a_single_variant_is_always_selected() {
        let model = model(&[7]);
        let selected = select_variant(&model, [1, 2, 3]).expect("the only variant");
        assert_eq!(selected.weight, 7);
    }

    #[test]
    fn zero_weights_fall_back_to_the_first_variant() {
        let model = model(&[0, 0]);
        let selected = select_variant(&model, [4, 5, 6]).expect("the first variant");
        assert!(std::ptr::eq(selected, &model.variants[0]));
    }

    #[test]
    fn liquids_take_their_still_and_flowing_layers_from_the_atlas() {
        let atlas = AtlasLayout::from_keys([
            "blocks/water_still",
            "blocks/water_flow",
            "blocks/lava_still",
            "blocks/lava_flow",
        ]);
        let water = fluid_layers(Liquid::Water, &atlas);
        assert_eq!(Some(water.still), atlas.layer("blocks/water_still"));
        assert_eq!(Some(water.flow), atlas.layer("blocks/water_flow"));
        let lava = fluid_layers(Liquid::Lava, &atlas);
        assert_eq!(Some(lava.still), atlas.layer("blocks/lava_still"));
        assert_eq!(Some(lava.flow), atlas.layer("blocks/lava_flow"));

        // An atlas without the liquid textures answers with its checker,
        // never a procedural layer index (ADR-0012).
        let empty = AtlasLayout::default();
        let missing = fluid_layers(Liquid::Water, &empty);
        assert_eq!(
            (missing.still, missing.flow),
            (empty.missing_layer(), empty.missing_layer())
        );
    }

    #[test]
    fn a_position_always_selects_the_same_weighted_variant() {
        let model = model(&[1, 3]);
        let first = select_variant(&model, [-17, 64, 42]).expect("a variant");
        for _ in 0..8 {
            let again = select_variant(&model, [-17, 64, 42]).expect("a variant");
            assert!(std::ptr::eq(first, again));
        }
    }

    #[test]
    fn the_weights_are_used_across_positions() {
        let model = model(&[1, 3]);
        let mut seen = [false, false];
        for x in -16..16 {
            for z in -16..16 {
                let selected = select_variant(&model, [x, 8, z]).expect("a variant");
                let index = if std::ptr::eq(selected, &model.variants[1]) {
                    1
                } else {
                    0
                };
                seen[index] = true;
            }
        }
        assert_eq!(seen, [true, true], "both weighted variants must appear");
    }
}
