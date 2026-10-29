// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity tracking, movement interpolation, and render snapshots.
//!
//! The data types live in small modules ([`kind`], [`flags`], [`item`],
//! [`record`], [`snapshot`]); the animation maths in [`limbs`], [`body_yaw`],
//! [`motion`], [`anim`] and the typed metadata in [`meta`]; [`store`] owns the entities and drives them.

pub mod boss;
pub mod angle;
pub mod anim;
pub mod body_yaw;
pub mod cape;
pub mod flags;
pub mod item;
pub mod kind;
pub mod limbs;
pub mod meta;
pub mod motion;
pub mod nametag;
pub mod profile;
pub mod record;
pub mod snapshot;
pub mod store;

pub use boss::{BossInfo, BossTracker, DRAGON_MAX_HEALTH, WITHER_MAX_HEALTH};
pub use anim::{Animation, BedPose};
pub use body_yaw::BodyYaw;
pub use cape::{CapeAngles, CapeChase};
pub use flags::EntityFlags;
pub use item::EntityItem;
pub use kind::{EntityId, EntityKind};
pub use limbs::LimbSwing;
pub use meta::{EntityMeta, Variant};
pub use profile::PlayerProfile;
pub use record::EntityRecord;
pub use snapshot::{EntityFrameData, EntityRenderSnapshot};
pub use store::EntityStore;
