// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom colours: `color.properties`, colormap palettes and lightmaps.
//!
//! - [`ColorProperties`] reads `color.properties`: fixed colours (water
//!   particles, lily pad, fog and sky of the Nether and the End, text) and the
//!   `palette.block.*` links between colormap pictures and blocks.
//! - [`Palette`] is a decoded colormap picture with one of the three lookups
//!   of OptiFine's `CustomColormap`: `vanilla` (biome climate), `grid`
//!   (biome id by height) and `fixed`.
//! - [`lightmap`] turns a lightmap picture into the 256 light colours.
//!
//! The pictures are decoded by the caller, so this crate stays free of image
//! code. Behaviour from OptiFine's `CustomColors`, `CustomColormap`, `LightMap`
//! and `LightMapPack` (ADR-0088, `docs/sources.md`).

pub mod lightmap;
mod palette;
mod properties;

pub use palette::{Climate, Format, Palette, PaletteError, PaletteSpec};
pub use properties::{ColorProperties, Rgb};

/// Resolves a path written in a colour file to a feature-relative one
/// (`colormap/blocks/grass.png`), the way `TextureUtils.fixResourcePath`
/// does. `base` is the feature-relative folder of the file that wrote it.
///
/// - `assets/minecraft/` and the `mcpatcher/` or `optifine/` folder are
///   dropped: both folders name the same feature root.
/// - `./x` is relative to `base`; `~/x` and `/x` are relative to the root.
/// - Any other path is taken as already relative to the root.
#[must_use]
pub fn resolve_path(path: &str, base: &str) -> String {
    let path = path.trim();
    let path = path.strip_prefix("assets/minecraft/").unwrap_or(path);
    let path = ["mcpatcher/", "optifine/"]
        .iter()
        .find_map(|folder| path.strip_prefix(folder))
        .unwrap_or(path);
    if let Some(rest) = path.strip_prefix("./") {
        return if base.is_empty() {
            rest.to_string()
        } else {
            format!("{}/{rest}", base.trim_end_matches('/'))
        };
    }
    let path = path.strip_prefix("/~").unwrap_or(path);
    let path = path.strip_prefix("~/").unwrap_or(path);
    path.trim_start_matches('/').to_string()
}

#[cfg(test)]
mod tests;
