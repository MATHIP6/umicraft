// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-world
//!
//! The client-side world model (spec RND-1). It turns the 1.8 wire format
//! (`Chunk Data` 0x21 and `Map Chunk Bulk` 0x26) into compact per-section
//! storage and applies block updates from `Block Change` (0x23) and
//! `Multi Block Change` (0x22).
//!
//! Storage layout: a [`World`] owns [`ChunkColumn`]s keyed by [`ChunkPos`].
//! Each column holds up to 16 [`Section`]s. A section stores a small palette
//! of [`BlockState`]s plus bit-packed indices (1–12 bits per block) instead of
//! the wire's fixed 16 bits, and keeps the block/sky light nibble arrays for
//! later lighting work.
//!
//! ```
//! use umicraft_world::{BlockPos, BlockState, ChunkPos, World};
//!
//! let mut world = World::new(0);
//! assert_eq!(world.block(BlockPos::new(0, 64, 0)), BlockState::AIR);
//! assert!(!world.set_block(BlockPos::new(0, 64, 0), BlockState::new(1, 0)));
//! assert_eq!(world.chunk_count(), 0);
//! ```
//!
//! Unknown or out-of-range coordinates read as air; updates outside loaded
//! chunks are ignored (servers may race updates against chunk unloads).
//!
//! ## Sources
//! Wire layouts are cited in the `umicraft-protocol` packet modules. Storage
//! details follow MCP `ExtendedBlockStorage`/`NibbleArray`/`Chunk.fillChunk`
//! (see `docs/sources.md`).

pub mod block;
pub mod chunk;
pub mod error;
pub mod pos;
pub mod section;
pub mod world;

pub use block::{BlockKind, BlockState};
pub use chunk::{ChunkColumn, SECTION_COUNT, WORLD_HEIGHT};
pub use error::WorldError;
pub use pos::{BlockPos, ChunkPos};
pub use section::Section;
pub use world::World;
