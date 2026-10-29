// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity tracking, movement interpolation, and render snapshots.
//!
//! The data types live in small modules ([`kind`], [`flags`], [`item`],
//! [`record`], [`snapshot`]); the animation maths in [`limbs`], [`body_yaw`]
//! and [`motion`]; [`store`] owns the entities and drives them.

pub mod boss;
pub mod angle;
pub mod body_yaw;
pub mod flags;
pub mod item;
pub mod kind;
pub mod limbs;
pub mod motion;
pub mod record;
pub mod snapshot;
pub mod store;

pub use boss::{BossInfo, BossTracker, DRAGON_MAX_HEALTH, WITHER_MAX_HEALTH};
pub use body_yaw::BodyYaw;
pub use flags::EntityFlags;
pub use item::EntityItem;
pub use kind::{EntityId, EntityKind};
pub use limbs::LimbSwing;
pub use record::EntityRecord;
pub use snapshot::{EntityFrameData, EntityRenderSnapshot};
pub use store::EntityStore;
