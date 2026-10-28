// SPDX-License-Identifier: MIT OR Apache-2.0

//! Item models (P3): `models/item/*.json` baked for every 1.8 item.
//!
//! The model documents and their `parent` chains reuse the block model
//! loader; this module adds what is item-specific:
//!
//! - [`names`]: which model an `(item id, metadata)` pair uses, generated
//!   from MCP `RenderItem.registerItems`;
//! - [`special`]: the potion, spawn egg, banner, enchanted book and map,
//!   which choose their model by code;
//! - [`generated`]: the `builtin/generated` extrusion of a flat sprite;
//! - [`set`]: [`ItemModelSet`], the lookup used by the renderers.
//!
//! The crate reads no files: the caller provides the jar through
//! [`crate::ResourceSource`] and the sprite silhouettes through
//! [`SpriteLookup`].

pub mod generated;
mod names;
pub mod set;
pub mod special;

pub use generated::SpriteMask;
pub use set::{ItemGeometry, ItemModel, ItemModelSet, SpriteLookup};
pub use special::special_model_name;
