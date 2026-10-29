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

use thiserror::Error;
#[cfg(test)]
use umicraft_assets::install::OfficialInstall;
use umicraft_assets::AssetsError;
use umicraft_render::{FontAtlasError, Renderer};
use umicraft_ui::{FontAtlas, FontError};

use crate::official;

/// The font entry inside the 1.8 client jar.
const FONT_ENTRY: &str = "assets/minecraft/textures/font/ascii.png";

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
        .and_then(|jar| Ok(FontAtlas::from_png(&jar.read_entry(FONT_ENTRY)?)?));
    match loaded {
        Ok(atlas) => {
            tracing::info!("loaded the 1.8 font from the client jar");
            LoadedFont {
                atlas,
                source: FontSource::Official,
            }
        }
        Err(error) => {
            tracing::warn!(%error, "could not load the 1.8 font; using the built-in fallback font");
            fallback_font()
        }
    }
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

/// The built-in original font.
fn fallback_font() -> LoadedFont {
    LoadedFont {
        atlas: FontAtlas::fallback(),
        source: FontSource::Fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

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

    #[test]
    fn the_font_source_labels_are_stable() {
        assert_eq!(FontSource::Official.to_string(), "official");
        assert_eq!(FontSource::Fallback.to_string(), "fallback");
    }

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn a_rejected_atlas_retries_the_fallback() {
        let mut renderer = Renderer::new_offscreen(16, 16).expect("offscreen renderer");
        let rejected = FontAtlas {
            width: 0,
            height: 0,
            pixels: Vec::new(),
            glyphs: Vec::new(),
            line_height: 8.0,
            white_uv: [0.0; 4],
        };
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
