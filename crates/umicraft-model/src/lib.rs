// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-model
//!
//! The pure 1.8 block model pipeline (spec M5): blockstate `variants`, the
//! model `parent` chain, `textures`, `elements`, element/face rotation, the
//! block-id registry and baked block-local quads.
//!
//! The crate performs no I/O and knows no Minecraft installation. A caller
//! supplies the files of the client jar through [`ResourceSource`]; the
//! `umicraft-assets` crate implements that trait for a verified jar. This
//! keeps every parser testable with synthetic JSON and keeps Mojang assets out
//! of the repository.
//!
//! Geometry is baked into block-local coordinates in `0.0..=1.0` (elements may
//! extend outside, e.g. crops start at `y = -1/16`). Quad corners are
//! counter-clockwise seen from outside, UVs are normalized with `v = 0` at the
//! top of the texture, and [`ModelQuad::texture`] is the un-prefixed texture key
//! from the model (`blocks/stone`). The atlas assigns the layer.
//!
//! ```
//! use umicraft_model::{AtlasLayout, ResourceSource};
//! use std::collections::BTreeMap;
//!
//! struct Memory(BTreeMap<String, Vec<u8>>);
//! impl ResourceSource for Memory {
//!     fn read(&self, path: &str) -> umicraft_model::ModelResult<Vec<u8>> {
//!         self.0.get(path).cloned().ok_or_else(|| umicraft_model::ModelError::Resource {
//!             path: path.to_string(),
//!             message: "missing test resource".to_string(),
//!         })
//!     }
//!
//!     fn list(&self, prefix: &str) -> Vec<String> {
//!         self.0.keys().filter(|path| path.starts_with(prefix)).cloned().collect()
//!     }
//! }
//!
//! let layout = AtlasLayout::from_keys(["blocks/stone"]);
//! assert_eq!(layout.layer("blocks/stone"), Some(1));
//! ```

pub mod atlas;
pub mod blockstate;
pub mod entity;
pub mod error;
pub mod geometry;
pub mod model;
pub mod registry;

pub use atlas::{AtlasLayout, MISSING_TEXTURE};
pub use blockstate::BlockStateFile;
pub use entity::{HumanoidModel, ModelBox, ModelPart, ModelVertex, QuadrupedModel};
pub use error::{ModelError, ModelResult};
pub use geometry::{Face, ModelQuad};
pub use model::{BlockModel, BlockModelSet, ModelVariant, ResolvedModel, ResourceSource};
pub use registry::{definition, definitions, BlockDefinition, StateDef};
