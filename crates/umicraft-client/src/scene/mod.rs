// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the renderer draws of the world besides the terrain: entities, block
//! entities, items, maps, particles, weather, fog, overlays and cracks.

pub(crate) mod block_entity;
pub(crate) mod border;
pub(crate) mod cracks;
pub mod dynamic_lights;
pub mod entity;
pub(crate) mod fog;
pub(crate) mod item_world;
pub(crate) mod map;
pub(crate) mod moving_blocks;
pub(crate) mod nametag;
pub(crate) mod overlay;
pub(crate) mod overlay_state;
pub(crate) mod particle_ambient;
pub(crate) mod particles;
pub(crate) mod weather;
