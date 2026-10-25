// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity tracking, movement interpolation, and render snapshots.

pub mod store;
pub mod types;

pub use store::EntityStore;
pub use types::{
    EntityFlags, EntityFrameData, EntityId, EntityKind, EntityRecord, EntityRenderSnapshot,
};
