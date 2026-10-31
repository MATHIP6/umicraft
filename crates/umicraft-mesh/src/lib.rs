// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-mesh
//!
//! Naive CPU chunk meshing (spec RND-2, M2 slice): for every renderable block
//! in a 16×256×16 column, emit a quad for each of the six faces whose
//! neighbour does not occlude it. Opaque cubes cull faces; identical
//! transparent/liquid blocks (glass-glass, water-water) cull each other;
//! non-cube decorations are skipped until the block registry lands in M5.
//!
//! Meshing reads blocks through [`BlockSource`]. [`ChunkSnapshot`] captures a
//! 3×3 neighbourhood of [`umicraft_world::ChunkColumn`] handles, so a worker
//! thread can mesh a column without touching the live [`World`]:
//!
//! ```
//! use umicraft_mesh::{mesh_chunk, ChunkSnapshot};
//! use umicraft_world::{ChunkPos, World};
//!
//! let world = World::new(0);
//! let snapshot = ChunkSnapshot::from_world(&world, ChunkPos::new(0, 0));
//! let mesh = mesh_chunk(ChunkPos::new(0, 0), &snapshot);
//! assert!(mesh.is_empty());
//! ```
//!
//! Texture layer indices in [`Vertex::layer`] index the renderer's fallback
//! texture array; see [`texture_layer`].
//!
//! M5 adds the model path in [`models`]: [`mesh_chunk_with_models`] places the
//! quads that `umicraft-model` baked from the 1.8 block models and writes the
//! layers of the real texture atlas. [`mesh_chunk`] stays the no-assets
//! fallback.
//!
//! M6 adds smooth lighting, corner ambient occlusion and the transparent pass
//! (ADR-0008):
//!
//! - every emitted quad samples the four cells on the outward side of its face
//!   per corner and averages block and sky light; the two channels stay
//!   separate so sky light follows the clock while torches do not;
//! - AO counts the two edge neighbours and the diagonal at each corner and is
//!   quantized to `15/10/5/0` for zero/one/two/three occluders;
//! - faces whose state kind is [`BlockKind::Transparent`] or
//!   [`BlockKind::Liquid`] go to [`Mesh::transparent_vertices`] /
//!   [`Mesh::transparent_indices`], everything else to the opaque pass.
//!
//! Biome tints (ADR-0009) add one `u32` per vertex:
//!
//! - [`tint`] turns the biome byte of a column and the two runtime colormaps
//!   into the 1.8 grass/foliage/water colours;
//! - a model quad is tinted only when its face carries `tintindex`;
//! - [`ModelMeshContext::tints`] is immutable and shared by all mesh workers.
//!   Without it every vertex is white, so the no-assets path is unchanged.
//!
//! [`ModelMeshContext::tints`]: models::ModelMeshContext::tints
//!
//! The corner mapping and the light averaging follow
//! `BlockModelRenderer.AmbientOcclusionFace` and
//! `WorldProvider.generateLightBrightnessTable` in
//! the local 1.8 reference (ADR-0001); ADR-0008 §2 freezes the simplified
//! quantized rule and [`docs/sources.md`] records the provenance.
//!
//! [`docs/sources.md`]: ../../../docs/sources.md

mod actual_state;
mod block_entity;
mod face;
pub mod fluid;
pub mod item;
mod legacy;
mod lighting;
mod mesh;
pub mod models;
mod moving;
mod source;
pub mod tint;
mod vertex;

pub use block_entity::{BlockEntityKind, BlockEntityRef, SameNeighbours};
pub use face::Face;
pub use legacy::{mesh_chunk, texture_layer};
pub use mesh::Mesh;
pub use models::{mesh_chunk_with_models, ModelMeshContext};
pub use moving::{push_moving_block, push_piston_head, PistonHead};
pub use source::{BlockSource, ChunkSnapshot};
pub use vertex::{Vertex, VertexLight};

/// Number of layers in the fallback block texture array.
pub const TEXTURE_LAYERS: u32 = 19;
