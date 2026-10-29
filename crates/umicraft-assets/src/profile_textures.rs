// SPDX-License-Identifier: MIT OR Apache-2.0

//! Player skins and capes: the `textures` property of a game profile, the
//! trusted download of the PNGs it points to, and their decoding.
//!
//! Source: MCP `SkinManager.loadProfileTextures` / `loadSkin`,
//! `ImageBufferDownload`, `HttpTexture` and authlib's
//! `YggdrasilMinecraftSessionService.getTextures`. The property value is
//! base64 JSON `{"textures": {"SKIN": {"url", "metadata": {"model": "slim"}},
//! "CAPE": {"url"}}}`; the server sends it in `Player List Item`. Vanilla
//! only follows URLs on `minecraft.net` and `mojang.com`; so does this module.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread;

use serde::Deserialize;

use crate::error::{image_error, AssetsError, AssetsResult};
use crate::http::{Endpoints, HttpClient};
use crate::skin::{base64_decode, convert_64x32_to_64x64, decode_png_bytes};

/// Largest texture download accepted, in bytes (a 64x64 PNG is far smaller).
const MAX_TEXTURE_BYTES: usize = 1 << 20;

/// The texture URLs and the arm model of a profile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileTextures {
    /// URL of the skin PNG.
    pub skin: Option<String>,
    /// True when the skin asks for the slim (3 pixel) arms.
    pub slim: bool,
    /// URL of the cape PNG.
    pub cape: Option<String>,
}

#[derive(Deserialize)]
struct Payload {
    textures: Option<Textures>,
}

#[derive(Deserialize)]
struct Textures {
    #[serde(rename = "SKIN")]
    skin: Option<Record>,
    #[serde(rename = "CAPE")]
    cape: Option<Record>,
}

#[derive(Deserialize)]
struct Record {
    url: String,
    metadata: Option<Metadata>,
}

#[derive(Deserialize)]
struct Metadata {
    model: Option<String>,
}

/// Parses the value of a `textures` profile property.
///
/// # Errors
///
/// [`AssetsError::Image`] for invalid base64 and [`AssetsError::Json`] for
/// invalid JSON. A profile without textures is `Ok` with empty fields.
pub fn parse_textures_property(value: &str) -> AssetsResult<ProfileTextures> {
    let bytes = base64_decode(value)?;
    let payload: Payload = serde_json::from_slice(&bytes).map_err(|source| AssetsError::Json {
        what: "profile textures payload".to_string(),
        source,
    })?;
    let Some(textures) = payload.textures else {
        return Ok(ProfileTextures::default());
    };
    let slim = textures
        .skin
        .as_ref()
        .and_then(|skin| skin.metadata.as_ref())
        .and_then(|metadata| metadata.model.as_deref())
        == Some("slim");
    Ok(ProfileTextures {
        skin: textures.skin.map(|skin| skin.url),
        slim,
        cape: textures.cape.map(|cape| cape.url),
    })
}

/// True for a URL the client may download a texture from: `http` or `https`
/// on `minecraft.net`, `mojang.com` or a subdomain of either (MCP
/// `YggdrasilMinecraftSessionService.isWhitelistedDomain`).
#[must_use]
pub fn is_trusted_texture_url(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // A userinfo or a port would let `trusted.net@evil.example` through.
    if authority.contains('@') || authority.contains(':') {
        return false;
    }
    let host = authority.to_ascii_lowercase();
    ["minecraft.net", "mojang.com"]
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

/// Turns a decoded skin image into 64x64 RGBA; a legacy 64x32 skin is
/// converted.
///
/// # Errors
///
/// [`AssetsError::Image`] for an image of any other size.
pub fn skin_from_image(rgba: &[u8], width: u32, height: u32) -> AssetsResult<Vec<u8>> {
    match (width, height) {
        (64, 64) => Ok(rgba.to_vec()),
        (64, 32) => convert_64x32_to_64x64(rgba),
        _ => Err(image_error(
            "skin",
            format!("unsupported skin size {width}x{height}"),
        )),
    }
}

/// Checks that a decoded cape image is 64x32.
///
/// # Errors
///
/// [`AssetsError::Image`] for an image of any other size.
pub fn check_cape_image(width: u32, height: u32) -> AssetsResult<()> {
    if (width, height) == (64, 32) {
        Ok(())
    } else {
        Err(image_error(
            "cape",
            format!("unsupported cape size {width}x{height}"),
        ))
    }
}

/// One finished download: the decoded RGBA or why it failed.
#[derive(Debug)]
pub struct FetchedTexture {
    /// The URL that was requested.
    pub url: String,
    /// Decoded `(rgba, width, height)`, or the failure text.
    pub image: Result<(Vec<u8>, u32, u32), String>,
}

/// Downloads textures on a worker thread, so the game and render threads
/// never wait on the network. Finished downloads are collected with
/// [`poll`](Self::poll). A cache directory, when given, keeps the PNGs by the
/// hash in their URL so a skin is fetched once.
#[derive(Debug)]
pub struct TextureFetcher {
    requests: Sender<String>,
    results: Receiver<FetchedTexture>,
    asked: Mutex<HashSet<String>>,
}

impl TextureFetcher {
    /// Starts the worker.
    #[must_use]
    pub fn spawn(cache_dir: Option<PathBuf>) -> Self {
        let (requests, request_queue) = mpsc::channel::<String>();
        let (sender, results) = mpsc::channel();
        thread::Builder::new()
            .name("texture-fetch".to_owned())
            .spawn(move || worker(&request_queue, &sender, cache_dir.as_deref()))
            .ok();
        Self {
            requests,
            results,
            asked: Mutex::new(HashSet::new()),
        }
    }

    /// Asks for a texture; a URL already asked for is ignored.
    pub fn request(&self, url: &str) {
        let first = self
            .asked
            .lock()
            .map(|mut asked| asked.insert(url.to_owned()))
            .unwrap_or(false);
        if first {
            // A closed channel means the worker is gone: no skins, no crash.
            self.requests.send(url.to_owned()).ok();
        }
    }

    /// The downloads that finished since the last call.
    pub fn poll(&self) -> Vec<FetchedTexture> {
        self.results.try_iter().collect()
    }
}

fn worker(
    requests: &Receiver<String>,
    results: &Sender<FetchedTexture>,
    cache: Option<&std::path::Path>,
) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    let client = HttpClient::new(Endpoints::official()).ok();
    while let Ok(url) = requests.recv() {
        let image = runtime.block_on(load(&url, client.as_ref(), cache));
        let finished = FetchedTexture {
            url,
            image: image.map_err(|error| error.to_string()),
        };
        if results.send(finished).is_err() {
            return;
        }
    }
}

/// The cache file of a texture URL: its last segment, when that is a hex hash.
fn cache_file(cache: &std::path::Path, url: &str) -> Option<PathBuf> {
    let name = url.rsplit('/').next()?;
    let valid =
        !name.is_empty() && name.len() <= 64 && name.bytes().all(|byte| byte.is_ascii_hexdigit());
    valid.then(|| cache.join(format!("{}.png", name.to_ascii_lowercase())))
}

async fn load(
    url: &str,
    client: Option<&HttpClient>,
    cache: Option<&std::path::Path>,
) -> AssetsResult<(Vec<u8>, u32, u32)> {
    if !is_trusted_texture_url(url) {
        return Err(image_error(url, "not a trusted texture host"));
    }
    let file = cache.and_then(|cache| cache_file(cache, url));
    if let Some(bytes) = file.as_ref().and_then(|file| std::fs::read(file).ok()) {
        if let Ok(decoded) = decode_png_bytes(&bytes) {
            return Ok(decoded);
        }
    }
    let client = client.ok_or_else(|| image_error(url, "no http client"))?;
    let bytes = client.get_bytes_limited(url, MAX_TEXTURE_BYTES).await?;
    let decoded = decode_png_bytes(&bytes)?;
    if let Some(file) = file {
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        // The cache is an optimisation: a failed write only costs a download.
        std::fs::write(file, &bytes).ok();
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(json: &str) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in json.as_bytes().chunks(3) {
            let mut buf = [0u8; 3];
            buf[..chunk.len()].copy_from_slice(chunk);
            let n = u32::from(buf[0]) << 16 | u32::from(buf[1]) << 8 | u32::from(buf[2]);
            for i in 0..=chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        out
    }

    #[test]
    fn a_slim_skin_and_a_cape_are_parsed() {
        let json = r#"{"timestamp":1,"textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/abc","metadata":{"model":"slim"}},"CAPE":{"url":"http://textures.minecraft.net/texture/def"}}}"#;
        let textures = parse_textures_property(&encode(json)).expect("valid property");
        assert_eq!(
            textures,
            ProfileTextures {
                skin: Some("http://textures.minecraft.net/texture/abc".into()),
                slim: true,
                cape: Some("http://textures.minecraft.net/texture/def".into()),
            }
        );
    }

    #[test]
    fn a_classic_skin_without_metadata_is_not_slim() {
        let json = r#"{"textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/abc"}}}"#;
        let textures = parse_textures_property(&encode(json)).expect("valid property");
        assert!(!textures.slim);
        assert_eq!(textures.cape, None);
    }

    #[test]
    fn a_profile_without_textures_is_empty_and_bad_data_is_an_error() {
        assert_eq!(
            parse_textures_property(&encode("{}")).expect("empty"),
            ProfileTextures::default()
        );
        assert!(parse_textures_property(&encode("not json")).is_err());
        assert!(parse_textures_property("!!!").is_err());
    }

    #[test]
    fn only_minecraft_and_mojang_hosts_are_trusted() {
        assert!(is_trusted_texture_url(
            "http://textures.minecraft.net/texture/ab"
        ));
        assert!(is_trusted_texture_url("https://minecraft.net/a"));
        assert!(is_trusted_texture_url("https://skins.mojang.com/x"));
        assert!(!is_trusted_texture_url("https://evilminecraft.net/x"));
        assert!(!is_trusted_texture_url("https://minecraft.net.evil.com/x"));
        assert!(!is_trusted_texture_url(
            "https://trusted.minecraft.net@evil.com/x"
        ));
        assert!(!is_trusted_texture_url("https://minecraft.net:8080/x"));
        assert!(!is_trusted_texture_url("ftp://minecraft.net/x"));
        assert!(!is_trusted_texture_url("file:///etc/passwd"));
    }

    #[test]
    fn only_hex_hashes_are_cached() {
        let cache = std::path::Path::new("/cache");
        assert_eq!(
            cache_file(cache, "http://textures.minecraft.net/texture/ABC123"),
            Some(PathBuf::from("/cache/abc123.png"))
        );
        assert_eq!(
            cache_file(cache, "http://textures.minecraft.net/../../x"),
            None
        );
        assert_eq!(cache_file(cache, "http://textures.minecraft.net/"), None);
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        writer
            .write_image_data(&vec![255; (width * height * 4) as usize])
            .expect("data");
        writer.finish().expect("finish");
        out
    }

    #[test]
    fn skins_and_capes_accept_their_sizes_only() {
        let (rgba, w, h) = decode_png_bytes(&png(64, 64)).expect("png");
        assert_eq!(
            skin_from_image(&rgba, w, h).expect("64x64").len(),
            64 * 64 * 4
        );
        let (rgba, w, h) = decode_png_bytes(&png(64, 32)).expect("png");
        assert_eq!(
            skin_from_image(&rgba, w, h).expect("legacy").len(),
            64 * 64 * 4
        );
        assert!(skin_from_image(&[], 128, 128).is_err());
        assert!(check_cape_image(64, 32).is_ok());
        assert!(check_cape_image(64, 64).is_err());
    }

    #[test]
    fn an_untrusted_url_is_refused_without_a_request() {
        let fetcher = TextureFetcher::spawn(None);
        fetcher.request("https://evil.example/texture/abc");
        let mut finished = Vec::new();
        for _ in 0..200 {
            finished = fetcher.poll();
            if !finished.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(finished.len(), 1);
        assert!(finished[0].image.is_err());
    }

    #[test]
    fn a_cached_texture_is_served_without_the_network() {
        let dir = std::env::temp_dir().join(format!("umicraft-skin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("abc123.png"), png(64, 64)).expect("write");
        let fetcher = TextureFetcher::spawn(Some(dir.clone()));
        fetcher.request("http://textures.minecraft.net/texture/abc123");
        let mut finished = Vec::new();
        for _ in 0..200 {
            finished = fetcher.poll();
            if !finished.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        std::fs::remove_dir_all(&dir).ok();
        let image = finished.pop().expect("a result").image;
        assert_eq!(image.map(|(_, w, h)| (w, h)), Ok((64, 64)));
    }
}
