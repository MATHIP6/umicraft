// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the renderer draws of the world besides the terrain: entities, block
//! entities, items, maps, particles, weather, fog, overlays and cracks.

pub(crate) mod block_entity;
pub(crate) mod border;
pub mod cracks;
pub mod entity;
pub(crate) mod fog;
pub mod item_world;
pub mod map;
pub mod moving_blocks;
pub mod nametag;
pub mod overlay;
pub mod overlay_state;
pub mod particle_ambient;
pub mod particles;
pub mod weather;
