// SPDX-License-Identifier: MIT OR Apache-2.0

//! Loading and uploading the user's 1.8 font, with the built-in fallback.
//!
//! Spec M4 and ADR-0006 require the real
//! `assets/minecraft/textures/font/ascii.png` when the user has an official
//! installation, and the original 3×5 fallback otherwise. The asset is read
//! from the user's client jar at runtime and is never copied into the
//! repository or a release (spec §3 rules 3 and 4).
//!
//! The loader is deliberately forgiving: a missing installation, a missing
//! version JSON or jar, a digest mismatch and a malformed PNG all log one
//! warning and select [`FontAtlas::fallback`]. Only a renderer that rejects
//! **both** the loaded atlas and the fallback is a fatal error, which
//! [`install`] reports to the caller.

use std::collections::HashMap;
use std::sync::Arc;

use thiserror::Error;
#[cfg(test)]
use umicraft_assets::install::OfficialInstall;
use umicraft_assets::{client_pack, AssetsError};
use umicraft_render::{FontAtlasError, Renderer};
use umicraft_ui::brand_font::{self, BrandFontError};
use umicraft_ui::{FontAtlas, FontError};

use crate::resources::game_files::GameFiles;
use crate::resources::official;

/// The font entry inside the 1.8 client jar.
const FONT_ENTRY: &str = "assets/minecraft/textures/font/ascii.png";
/// The OptiFine location of the glyph grid inside a pack-feature folder.
const CUSTOM_FONT: &str = "font/ascii.png";
/// The glyph widths of the Unicode pages (`FontRenderer.readGlyphSizes`).
const SIZES_ENTRY: &str = "assets/minecraft/font/glyph_sizes.bin";
/// The start of the names of the Unicode pages, `unicode_page_XX.png`.
const PAGE_PREFIX: &str = "assets/minecraft/textures/font/unicode_page_";

/// Where the UI font of a run came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSource {
    /// The user's official installation supplied the real 1.8 atlas.
    Official,
    /// The built-in original 3×5 atlas is in use.
    Fallback,
}

impl std::fmt::Display for FontSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Official => formatter.write_str("official"),
            Self::Fallback => formatter.write_str("fallback"),
        }
    }
}

/// A loaded UI atlas and the source it came from.
#[derive(Debug, Clone)]
pub struct LoadedFont {
    /// The atlas the UI should measure and draw with.
    pub atlas: FontAtlas,
    /// The Unicode pages of the jar the atlas was extended with, kept so a
    /// theme font can draw the same characters.
    pub unicode: Option<Arc<UnicodePages>>,
    /// Whether the atlas is the real 1.8 font or the built-in fallback.
    pub source: FontSource,
}

/// A font atlas that the renderer accepted.
#[derive(Debug, Clone)]
pub struct InstalledFont {
    /// The atlas actually bound for [`Renderer::render_with_ui`].
    pub atlas: FontAtlas,
    /// True when the requested atlas was rejected and the fallback is bound.
    pub used_fallback: bool,
}

/// Every failure of the official-font path.
#[derive(Debug, Error)]
enum FontLoadError {
    /// The installation layout, version JSON or jar could not be read or
    /// verified.
    #[error(transparent)]
    Assets(#[from] AssetsError),
    /// The entry exists but is not the 128×128 1.8 font PNG.
    #[error(transparent)]
    Font(#[from] FontError),
}

/// Loads the UI font from the official installation found in the environment.
///
/// This is the entry point of the windowed client and of `--ui-demo`. Any
/// failure is logged at `warn` and falls back to [`FontAtlas::fallback`]; the
/// function never panics and never fails.
#[must_use]
pub fn load() -> LoadedFont {
    // The official installation first, then the Umicraft asset cache that
    // `--setup-assets` fills; the terrain loader searches both the same way.
    let loaded = official::find_verified_jar()
        .map_err(FontLoadError::from)
        .and_then(|jar| {
            let grid = FontAtlas::from_png(&jar.read_entry(FONT_ENTRY)?)?;
            // The Unicode pages are an addition: without them the grid font
            // alone still works, so their failure is only a warning.
            let entries = jar
                .read_prefixes(&[SIZES_ENTRY, PAGE_PREFIX])
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "could not read the Unicode font pages");
                    Vec::new()
                });
            Ok::<_, FontLoadError>((grid, UnicodePages::from_entries(entries)))
        });
    match loaded {
        Ok((grid, unicode)) => {
            tracing::info!("loaded the 1.8 font from the client jar");
            LoadedFont {
                atlas: unicode
                    .as_ref()
                    .map_or(grid.clone(), |pages| pages.extend(&grid)),
                unicode: unicode.map(Arc::new),
                source: FontSource::Official,
            }
        }
        Err(error) => {
            tracing::warn!(%error, "could not load the 1.8 font; using the built-in fallback font");
            fallback_font()
        }
    }
}

/// Loads the UI font as [`load`] does, then lets the packs replace the glyph
/// grid.
///
/// The packs' own `textures/font/ascii.png` always counts, as in the
/// reference's resource manager. With "Custom Fonts" on, the OptiFine
/// location `font/ascii.png` inside a pack-feature folder (`mcpatcher/` or
/// `optifine/`) comes first (`FontUtils.getHdFontLocation`). The grid may be
/// a high-resolution copy of the 1.8 grid (a square whose side is a multiple
/// of 128); a picture that does not decode gives way to the next candidate.
/// The Unicode pages stay those of the jar.
///
/// `width.N` overrides of a font `.properties` are not read: the advances
/// come from the pixels of the grid, which gives the same widths for the HD
/// fonts that ship such a table.
#[must_use]
pub fn load_with(files: &GameFiles) -> LoadedFont {
    let mut font = load();
    let Some(grid) = pack_grid(files) else {
        return font;
    };
    font.atlas = font
        .unicode
        .as_ref()
        .map_or_else(|| grid.clone(), |pages| pages.extend(&grid));
    font.source = FontSource::Official;
    font
}

/// The first pack glyph grid that decodes: the OptiFine location when the
/// option is on, then the standard one.
fn pack_grid(files: &GameFiles) -> Option<FontAtlas> {
    let packs = files.packs();
    let custom = files
        .features()
        .custom_fonts
        .then(|| packs.feature(CUSTOM_FONT))
        .flatten()
        .map(|file| (file.path, file.bytes));
    let standard = packs.get(FONT_ENTRY).map(|bytes| (FONT_ENTRY, bytes));
    custom
        .into_iter()
        .chain(standard)
        .find_map(|(path, bytes)| {
            FontAtlas::from_hd_png(bytes)
                .map_err(|error| tracing::warn!(path, %error, "pack font ignored"))
                .ok()
        })
}

/// Uploads `atlas` to `renderer`, falling back to the built-in font once.
///
/// A rejected user atlas is not fatal: the function logs a warning and retries
/// with [`FontAtlas::fallback`]. The error is returned only when the renderer
/// rejects the fallback as well, which means no UI can be drawn safely.
///
/// # Errors
///
/// [`FontAtlasError`] from the second [`Renderer::set_font_atlas`] call.
pub fn install(
    renderer: &mut Renderer,
    atlas: &FontAtlas,
) -> Result<InstalledFont, FontAtlasError> {
    match renderer.set_font_atlas(atlas) {
        Ok(()) => Ok(InstalledFont {
            atlas: atlas.clone(),
            used_fallback: false,
        }),
        Err(error) => {
            tracing::warn!(%error, "font atlas rejected; retrying with the fallback font");
            let fallback = FontAtlas::fallback();
            renderer.set_font_atlas(&fallback)?;
            Ok(InstalledFont {
                atlas: fallback,
                used_fallback: true,
            })
        }
    }
}

/// Rasterizes the client pack's brand face and uploads it to `renderer`
/// (ADR-0056).
///
/// Returns the atlas the UI must measure brand text with, or `None` when the
/// font does not rasterize or the renderer rejects it; the failure is logged
/// and the caller then keeps the classic title screen, whose layout does not
/// depend on the brand face.
pub fn install_brand(renderer: &mut Renderer) -> Option<FontAtlas> {
    let atlas = match brand_atlas() {
        Ok(atlas) => atlas,
        Err(error) => {
            tracing::warn!(%error, "could not rasterize the brand font");
            return None;
        }
    };
    match renderer.set_brand_font(&atlas) {
        Ok(()) => Some(atlas),
        Err(error) => {
            tracing::warn!(%error, "brand font atlas rejected by the renderer");
            None
        }
    }
}

/// The brand atlas rasterized from the embedded TTF.
fn brand_atlas() -> Result<FontAtlas, BrandFontError> {
    brand_font::rasterize(client_pack::brand_font_ttf(), brand_font::BRAND_PIXEL_SIZE)
}

/// Loads from an optional installation, logging the reason on fallback.
#[cfg(test)]
fn load_from_install(install: Option<OfficialInstall>) -> LoadedFont {
    let Some(install) = install else {
        tracing::warn!(
            "no official Minecraft installation found; using the built-in fallback font"
        );
        return fallback_font();
    };
    match load_official(&install) {
        Ok(atlas) => {
            tracing::info!(
                root = %install.root().display(),
                "loaded the 1.8 font from the official installation"
            );
            LoadedFont {
                atlas,
                unicode: None,
                source: FontSource::Official,
            }
        }
        Err(error) => {
            tracing::warn!(
                %error,
                root = %install.root().display(),
                "could not load the 1.8 font; using the built-in fallback font"
            );
            fallback_font()
        }
    }
}

/// Reads and decodes the font out of one official installation.
#[cfg(test)]
fn load_official(install: &OfficialInstall) -> Result<FontAtlas, FontLoadError> {
    let jar = official::open_client_jar(install)?;
    let png = jar.read_entry(FONT_ENTRY)?;
    Ok(FontAtlas::from_png(&png)?)
}

/// The Unicode half of the 1.8 font: `glyph_sizes.bin` and the
/// `unicode_page_XX.png` files of the jar, kept as bytes so any glyph grid can
/// be extended with them (the game font and the HD theme font alike).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnicodePages {
    sizes: Vec<u8>,
    pages: HashMap<u8, Vec<u8>>,
}

impl UnicodePages {
    /// Collects the pages from the jar `entries`; `None` without the sizes
    /// file, since the pages alone say nothing about glyph widths.
    #[must_use]
    pub fn from_entries(entries: Vec<(String, Vec<u8>)>) -> Option<Self> {
        let mut sizes = None;
        let mut pages = HashMap::new();
        for (name, bytes) in entries {
            if name == SIZES_ENTRY {
                sizes = Some(bytes);
            } else if let Some(page) = name
                .strip_prefix(PAGE_PREFIX)
                .and_then(|rest| rest.strip_suffix(".png"))
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                pages.insert(page, bytes);
            }
        }
        Some(Self {
            sizes: sizes?,
            pages,
        })
    }

    /// `grid` extended with these pages, so that characters outside the 1.8
    /// grid draw as in vanilla instead of as `?`.
    #[must_use]
    pub fn extend(&self, grid: &FontAtlas) -> FontAtlas {
        let atlas = grid.with_unicode(&self.sizes, |page| self.pages.get(&page).cloned());
        tracing::info!(pages = self.pages.len(), "loaded the Unicode font pages");
        atlas
    }
}

/// The built-in original font.
fn fallback_font() -> LoadedFont {
    LoadedFont {
        atlas: FontAtlas::fallback(),
        unicode: None,
        source: FontSource::Fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::game_files::test_support::png;
    use crate::resources::game_files::PackFeatures;
    use crate::test_support;

    fn pack_files(entries: &[(&str, Vec<u8>)], features: PackFeatures) -> GameFiles {
        let files = entries
            .iter()
            .map(|(path, bytes)| (format!("assets/minecraft/{path}"), bytes.clone()))
            .collect();
        let pack = umicraft_assets::ResourcePack::from_parts("fonts", None, None, files);
        GameFiles::new(umicraft_assets::PackFiles::from_packs([pack]), None).with_features(features)
    }

    #[test]
    fn the_optifine_grid_beats_the_standard_one() {
        let files = pack_files(
            &[
                ("mcpatcher/font/ascii.png", png(256, 256)),
                ("textures/font/ascii.png", png(128, 128)),
            ],
            PackFeatures::ALL,
        );
        assert_eq!(pack_grid(&files).map(|atlas| atlas.height), Some(256));
    }

    #[test]
    fn the_option_off_ignores_the_optifine_grid_only() {
        let off = PackFeatures {
            custom_fonts: false,
            ..PackFeatures::ALL
        };
        let both = pack_files(
            &[
                ("optifine/font/ascii.png", png(256, 256)),
                ("textures/font/ascii.png", png(128, 128)),
            ],
            off,
        );
        assert_eq!(pack_grid(&both).map(|atlas| atlas.height), Some(128));
        let only_custom = pack_files(&[("optifine/font/ascii.png", png(256, 256))], off);
        assert!(pack_grid(&only_custom).is_none());
    }

    #[test]
    fn a_broken_custom_grid_gives_way_to_the_standard_one() {
        let files = pack_files(
            &[
                ("mcpatcher/font/ascii.png", b"not a png".to_vec()),
                ("textures/font/ascii.png", png(128, 128)),
            ],
            PackFeatures::ALL,
        );
        assert_eq!(pack_grid(&files).map(|atlas| atlas.height), Some(128));
        let odd_size = pack_files(
            &[("textures/font/ascii.png", png(100, 100))],
            PackFeatures::ALL,
        );
        assert!(pack_grid(&odd_size).is_none());
    }

    #[test]
    fn a_pack_grid_replaces_the_font_even_without_a_jar() {
        let files = pack_files(
            &[("textures/font/ascii.png", png(128, 128))],
            PackFeatures::ALL,
        );
        let font = load_with(&files);
        assert_eq!(font.source, FontSource::Official);
    }

    fn load_captured(install: Option<OfficialInstall>) -> (LoadedFont, Vec<String>) {
        test_support::capture(|| load_from_install(install))
    }

    #[test]
    fn no_installation_falls_back_and_warns() {
        let (font, messages) = load_captured(None);
        assert_eq!(font.source, FontSource::Fallback);
        assert_eq!(font.atlas.width, FontAtlas::fallback().width);
        assert!(
            messages
                .iter()
                .any(|message| message.contains("fallback font")),
            "expected a warning, got {messages:?}"
        );
    }

    #[test]
    fn a_missing_version_json_falls_back_and_warns() {
        let root = std::env::temp_dir().join(format!(
            "umicraft-client-font-missing-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("temp install is created");
        let install = OfficialInstall::new(&root);
        let (font, messages) = load_captured(Some(install));
        std::fs::remove_dir_all(&root).ok();

        assert_eq!(font.source, FontSource::Fallback);
        assert!(
            messages.iter().any(|message| message.contains("1.8 font")),
            "expected a warning, got {messages:?}"
        );
    }

    /// With a real jar the symbols outside the 1.8 grid have their glyph, as
    /// the star of a scoreboard line; without one the test has nothing to read.
    #[test]
    fn an_official_font_draws_unicode_symbols() {
        let font = load();
        if font.source != FontSource::Official {
            return;
        }
        let atlas = font.atlas;
        for symbol in ['\u{2726}', '\u{2764}', '\u{4e2d}'] {
            let glyph = atlas.glyph(symbol).expect("the Unicode pages have it");
            assert_eq!(glyph.scale, 0.5, "{symbol}");
            assert!(atlas.advance(symbol, 1.0) > 0.0, "{symbol}");
        }
        assert_eq!(atlas.glyph('A').map(|glyph| glyph.scale), Some(1.0));
        assert!(atlas.width <= 8192 && atlas.height <= 8192);
    }

    #[test]
    fn the_font_source_labels_are_stable() {
        assert_eq!(FontSource::Official.to_string(), "official");
        assert_eq!(FontSource::Fallback.to_string(), "fallback");
    }

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn a_rejected_atlas_retries_the_fallback() {
        let mut renderer = Renderer::new_offscreen(16, 16).expect("offscreen renderer");
        let rejected = FontAtlas::new(0, 0, Vec::new(), Vec::new(), 8.0, [0.0; 4]);
        let (installed, messages) = test_support::capture(|| install(&mut renderer, &rejected));
        let installed = installed.expect("the fallback atlas is accepted");

        assert!(installed.used_fallback);
        assert_eq!(installed.atlas, FontAtlas::fallback());
        assert!(
            messages.iter().any(|message| message.contains("fallback")),
            "expected a fallback warning, got {messages:?}"
        );
    }
}
