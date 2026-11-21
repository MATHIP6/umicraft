// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-packext
//!
//! Parses and evaluates the resource-pack feature formats that OptiFine
//! established and that packs rely on: custom sky, custom entity models,
//! connected textures, and the others of milestone F6 (ADR-0089).
//!
//! The crate has no GPU, window or world dependency. Each feature turns the
//! raw files of a pack into a typed plan, which the renderer, the mesher and
//! the model code consume. Formats can therefore be tested and fuzzed on their
//! own. A malformed file never panics: it gives a typed error, and the caller
//! drops that one layer, model or rule.
//!
//! Shared building blocks:
//! - [`properties`]: the `.properties` files most features are written in;
//! - [`ranges`]: integer range lists such as `0-3 5`;
//! - [`filters`]: the biome, height and weather conditions.
//!
//! Features:
//! - [`sky`]: custom sky layers.
//!
//! Sources: ADR-0088 and `docs/sources.md`, section "OptiFine features".

pub mod filters;
pub mod properties;
pub mod ranges;
pub mod sky;

pub use filters::{BiomeFilter, WeatherSet};
pub use properties::{Properties, PropertiesError};
pub use ranges::{Range, RangeError, RangeList};
