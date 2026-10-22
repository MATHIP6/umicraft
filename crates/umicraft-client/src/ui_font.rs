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

use std::path::{Path, PathBuf};

use thiserror::Error;
use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::install::OfficialInstall;
use umicraft_assets::manifest::VersionJson;
use umicraft_assets::AssetsError;
use umicraft_render::{FontAtlasError, Renderer};
use umicraft_ui::{FontAtlas, FontError};

/// The only game version M4 supports (protocol 47, spec §1.3).
const VERSION_ID: &str = "1.8.8";
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
    load_from_install(OfficialInstall::from_env())
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
fn load_official(install: &OfficialInstall) -> Result<FontAtlas, FontLoadError> {
    let version_dir = install.versions_dir().join(VERSION_ID);
    let json_path = version_dir.join(format!("{VERSION_ID}.json"));
    let json = std::fs::read(&json_path).map_err(|source| AssetsError::Io {
        path: json_path.display().to_string(),
        source,
    })?;
    let version = VersionJson::parse(&json)?;
    let descriptor = version.require_client_jar()?;

    let jar_path = client_jar_path(install, &version_dir)?;
    let jar = ClientJar::open(jar_path, &descriptor.sha1, descriptor.size)?;
    let png = jar.read_entry(FONT_ENTRY)?;
    Ok(FontAtlas::from_png(&png)?)
}

/// The client jar path of the installation, with a typed missing-file error.
fn client_jar_path(install: &OfficialInstall, version_dir: &Path) -> Result<PathBuf, AssetsError> {
    install
        .client_jar_path(VERSION_ID)
        .ok_or_else(|| AssetsError::Missing {
            what: format!("client jar of version {VERSION_ID}"),
            path: version_dir
                .join(format!("{VERSION_ID}.jar"))
                .display()
                .to_string(),
        })
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
    use std::sync::{Arc, Mutex};

    use tracing::field::{Field, Visit};
    use tracing::span;
    use tracing::{Event, Metadata, Subscriber};

    use super::*;

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<String>>>);

    impl Subscriber for Capture {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }

        fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

        fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

        fn event(&self, event: &Event<'_>) {
            let mut message = Message(String::new());
            event.record(&mut message);
            if let Ok(mut messages) = self.0.lock() {
                messages.push(message.0);
            }
        }

        fn enter(&self, _span: &span::Id) {}

        fn exit(&self, _span: &span::Id) {}
    }

    struct Message(String);

    impl Visit for Message {
        fn record_str(&mut self, field: &Field, value: &str) {
            if field.name() == "message" {
                self.0 = value.to_string();
            }
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0 = format!("{value:?}");
            }
        }
    }

    fn load_captured(install: Option<OfficialInstall>) -> (LoadedFont, Vec<String>) {
        let capture = Capture::default();
        let font =
            tracing::subscriber::with_default(capture.clone(), || load_from_install(install));
        let messages = capture
            .0
            .lock()
            .map(|messages| messages.clone())
            .unwrap_or_default();
        (font, messages)
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
}
