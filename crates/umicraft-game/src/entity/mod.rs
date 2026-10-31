// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity tracking, movement interpolation, and render snapshots.
//!
//! The data types live in small modules ([`kind`], [`flags`], [`item`],
//! [`record`], [`snapshot`]); the animation maths in [`limbs`], [`body_yaw`],
//! [`motion`], [`anim`] and the typed metadata in [`meta`]; [`store`] owns the entities and drives them.

pub mod angle;
pub mod anim;
pub mod body_yaw;
pub mod boss;
pub mod cape;
pub mod flags;
pub mod flight;
pub mod horse_state;
pub mod item;
pub mod kind;
pub mod leash;
pub mod light;
pub mod limbs;
pub mod meta;
pub mod motion;
pub mod nametag;
pub mod objects;
pub mod painting;
pub mod profile;
pub mod rail;
pub mod record;
pub mod shadow;
pub mod snapshot;
pub mod store;

pub use anim::{Animation, BedPose};
pub use body_yaw::BodyYaw;
pub use boss::{BossInfo, BossTracker, DRAGON_MAX_HEALTH, WITHER_MAX_HEALTH};
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
