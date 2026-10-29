// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity tracking, movement interpolation, and render snapshots.

pub mod boss;
pub mod store;
pub mod types;

pub use boss::{BossInfo, BossTracker, DRAGON_MAX_HEALTH, WITHER_MAX_HEALTH};
pub use store::EntityStore;
pub use types::{
    EntityFlags, EntityFrameData, EntityId, EntityItem, EntityKind, EntityRecord,
    EntityRenderSnapshot,
};
