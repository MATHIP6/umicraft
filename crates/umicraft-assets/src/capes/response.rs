// SPDX-License-Identifier: MIT OR Apache-2.0

//! The JSON answers of the services that do not serve a PNG directly.
//!
//! The types are tolerant on purpose: every field is optional, unknown
//! fields are ignored, and a missing piece means "no cape", because the
//! MinecraftCapes format changed in 2026 and may change again
//! (`docs/sources.md`, section "Capes"). Every URL found is checked against
//! an allowlist before anything is fetched.

use serde::Deserialize;

use super::error::CapeError;
use crate::skin::base64_decode;

/// The prefix of an inline PNG a service may send instead of a URL.
const PNG_DATA_PREFIX: &str = "data:image/png;base64,";

/// Where the bytes of a cape come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageSource {
    /// An allowed URL to download.
    Url(String),
    /// A PNG the answer already carried.
    Png(Vec<u8>),
}

/// A cape an answer points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapeSource {
    /// The image, or where to get it.
    pub image: ImageSource,
    /// True when the image is a strip of frames.
    pub animated: bool,
}

/// Decides which URLs an answer may name. The resolver passes
/// [`is_allowed_cape_url`](super::is_allowed_cape_url); the tests that run a
/// local server pass a permissive check.
pub(crate) type UrlCheck = fn(&str) -> bool;

#[derive(Deserialize, Default)]
#[serde(default)]
struct CosmeticaAnswer {
    cloak: Option<Cloak>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Cloak {
    texture: Option<String>,
    frames: Option<u32>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct MinecraftCapesAnswer {
    #[serde(alias = "capeUrl")]
    cape_url: Option<String>,
    #[serde(alias = "animatedCapeUrl")]
    animated_cape_url: Option<String>,
    #[serde(rename = "animatedCape")]
    animated_cape: Option<bool>,
    textures: Option<LegacyTextures>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LegacyTextures {
    cape: Option<String>,
}

fn parse<'a, T: Deserialize<'a>>(json: &'a [u8]) -> Result<T, CapeError> {
    serde_json::from_slice(json).map_err(|error| CapeError::BadJson(error.to_string()))
}

/// A URL or an inline PNG data URL, turned into an [`ImageSource`].
fn image_source(text: &str, allow: UrlCheck) -> Result<ImageSource, CapeError> {
    if text.starts_with("data:") {
        let payload = text
            .strip_prefix(PNG_DATA_PREFIX)
            .ok_or_else(|| CapeError::Decode("data url is not a base64 png".to_owned()))?;
        return decode_base64(payload).map(ImageSource::Png);
    }
    if allow(text) {
        Ok(ImageSource::Url(text.to_owned()))
    } else {
        Err(CapeError::HostNotAllowed(text.to_owned()))
    }
}

fn decode_base64(text: &str) -> Result<Vec<u8>, CapeError> {
    base64_decode(text).map_err(|error| CapeError::Decode(error.to_string()))
}

/// A non-empty string, so `""` counts as absent like `null`.
fn present(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|text| !text.is_empty())
}

/// Reads a Cosmetica `cape` answer: `{"cloak": {"texture", "frames"}}`.
///
/// `Ok(None)` when the answer holds no cloak. The cape is animated when
/// `frames` is above 1.
///
/// # Errors
///
/// [`CapeError::BadJson`], [`CapeError::HostNotAllowed`] or
/// [`CapeError::Decode`] (a broken inline image).
pub(crate) fn parse_cosmetica(
    json: &[u8],
    allow: UrlCheck,
) -> Result<Option<CapeSource>, CapeError> {
    let answer: CosmeticaAnswer = parse(json)?;
    let Some(cloak) = answer.cloak else {
        return Ok(None);
    };
    let Some(texture) = present(&cloak.texture) else {
        return Ok(None);
    };
    Ok(Some(CapeSource {
        image: image_source(texture, allow)?,
        animated: cloak.frames.is_some_and(|frames| frames > 1),
    }))
}

/// Reads a MinecraftCapes profile.
///
/// The animated URL wins over the static one, which wins over the legacy
/// base64 `textures.cape`. A source taken from the static fields is animated
/// only when the profile says `animatedCape`. `Ok(None)` when none is given.
///
/// # Errors
///
/// [`CapeError::BadJson`], [`CapeError::HostNotAllowed`] or
/// [`CapeError::Decode`].
pub(crate) fn parse_minecraftcapes(
    json: &[u8],
    allow: UrlCheck,
) -> Result<Option<CapeSource>, CapeError> {
    let answer: MinecraftCapesAnswer = parse(json)?;
    if let Some(url) = present(&answer.animated_cape_url) {
        return Ok(Some(CapeSource {
            image: image_source(url, allow)?,
            animated: true,
        }));
    }
    let legacy = answer.textures.as_ref().and_then(|t| present(&t.cape));
    let image = match (present(&answer.cape_url), legacy) {
        (Some(url), _) => image_source(url, allow)?,
        // The legacy field is bare base64, not a data URL.
        (None, Some(base64)) => ImageSource::Png(decode_base64(base64)?),
        (None, None) => return Ok(None),
    };
    Ok(Some(CapeSource {
        image,
        animated: answer.animated_cape.unwrap_or(false),
    }))
}

#[cfg(test)]
mod tests {
    use super::super::allowlist::is_allowed_cape_url;
    use super::*;

    const URL: &str = "https://api.cosmetica.cc/cloaks/x.png";

    fn cosmetica(json: &str) -> Result<Option<CapeSource>, CapeError> {
        parse_cosmetica(json.as_bytes(), is_allowed_cape_url)
    }

    fn mc(json: &str) -> Result<Option<CapeSource>, CapeError> {
        parse_minecraftcapes(json.as_bytes(), is_allowed_cape_url)
    }

    fn url_source(url: &str, animated: bool) -> Option<CapeSource> {
        Some(CapeSource {
            image: ImageSource::Url(url.to_owned()),
            animated,
        })
    }

    fn png_source(animated: bool) -> Option<CapeSource> {
        Some(CapeSource {
            // "iVBORw==" is the PNG signature start.
            image: ImageSource::Png(vec![0x89, b'P', b'N', b'G']),
            animated,
        })
    }

    #[test]
    fn cosmetica_url_is_static_for_one_frame() {
        let json = format!(r#"{{"cloak":{{"texture":"{URL}","frames":1,"extra":3}}}}"#);
        assert_eq!(cosmetica(&json), Ok(url_source(URL, false)));
    }

    #[test]
    fn cosmetica_many_frames_are_animated() {
        let json = format!(r#"{{"cloak":{{"texture":"{URL}","frames":20}}}}"#);
        assert_eq!(cosmetica(&json), Ok(url_source(URL, true)));
    }

    #[test]
    fn cosmetica_without_a_cloak_is_no_cape() {
        for json in [
            "{}",
            r#"{"cloak":null}"#,
            r#"{"cloak":{}}"#,
            r#"{"cloak":{"texture":""}}"#,
        ] {
            assert_eq!(cosmetica(json), Ok(None), "{json}");
        }
    }

    #[test]
    fn cosmetica_inline_png_is_decoded() {
        let json = r#"{"cloak":{"texture":"data:image/png;base64,iVBORw==","frames":0}}"#;
        assert_eq!(cosmetica(json), Ok(png_source(false)));
    }

    #[test]
    fn a_data_url_of_another_type_is_refused() {
        let json = r#"{"cloak":{"texture":"data:text/html;base64,AAAA"}}"#;
        assert!(matches!(cosmetica(json), Err(CapeError::Decode(_))));
    }

    #[test]
    fn a_url_outside_the_allowlist_is_refused() {
        let json = r#"{"cloak":{"texture":"https://evil.example/c.png"}}"#;
        assert_eq!(
            cosmetica(json),
            Err(CapeError::HostNotAllowed(
                "https://evil.example/c.png".into()
            ))
        );
    }

    #[test]
    fn invalid_json_is_bad_json() {
        assert!(matches!(cosmetica("nope"), Err(CapeError::BadJson(_))));
        assert!(matches!(mc("[1]"), Err(CapeError::BadJson(_))));
    }

    #[test]
    fn minecraftcapes_without_a_cape_is_none() {
        let json = r#"{"animatedCape":false,"capeGlint":false,"upsideDown":false,
            "textures":{"_IMPORTANT":"We are removing base64 textures"}}"#;
        assert_eq!(mc(json), Ok(None));
        assert_eq!(mc("{}"), Ok(None));
    }

    #[test]
    fn minecraftcapes_accepts_both_spellings_of_the_urls() {
        let u = "https://api.minecraftcapes.net/cape.png";
        for key in ["cape_url", "capeUrl"] {
            let json = format!(r#"{{"{key}":"{u}"}}"#);
            assert_eq!(mc(&json), Ok(url_source(u, false)));
        }
        for key in ["animated_cape_url", "animatedCapeUrl"] {
            let json = format!(r#"{{"{key}":"{u}"}}"#);
            assert_eq!(mc(&json), Ok(url_source(u, true)));
        }
    }

    #[test]
    fn the_animated_url_wins_over_the_static_one() {
        let json = r#"{"cape_url":"https://api.minecraftcapes.net/a.png",
            "animated_cape_url":"https://api.minecraftcapes.net/b.png"}"#;
        assert_eq!(
            mc(json),
            Ok(url_source("https://api.minecraftcapes.net/b.png", true))
        );
    }

    #[test]
    fn the_animated_flag_marks_a_static_url_field() {
        let u = "https://api.minecraftcapes.net/a.png";
        let json = format!(r#"{{"cape_url":"{u}","animatedCape":true}}"#);
        assert_eq!(mc(&json), Ok(url_source(u, true)));
    }

    #[test]
    fn the_legacy_base64_cape_is_a_png() {
        let json = r#"{"textures":{"cape":"iVBORw=="}}"#;
        assert_eq!(mc(json), Ok(png_source(false)));
    }

    #[test]
    fn minecraftcapes_urls_are_checked() {
        let json = r#"{"cape_url":"http://api.minecraftcapes.net/a.png"}"#;
        assert!(matches!(mc(json), Err(CapeError::HostNotAllowed(_))));
    }
}
