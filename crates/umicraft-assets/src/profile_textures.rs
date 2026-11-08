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

/// The Mojang service that turns a player name into a profile id.
const NAME_LOOKUP_URL: &str = "https://api.minecraftservices.com/minecraft/profile/lookup/name/";
/// The session server route that gives a profile with its `textures` property.
const PROFILE_URL: &str = "https://sessionserver.mojang.com/session/minecraft/profile/";
/// Largest profile answer accepted, in bytes (a profile is about a kilobyte).
const MAX_PROFILE_BYTES: usize = 1 << 18;

/// Whether `name` is a player name that is safe to put in a URL: 1 to 16
/// characters of `[A-Za-z0-9_]`.
fn is_valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whether `uuid` is 32 hexadecimal digits, without dashes.
fn is_valid_uuid(uuid: &str) -> bool {
    uuid.len() == 32 && uuid.chars().all(|c| c.is_ascii_hexdigit())
}

/// The URL that finds the profile id of the player named `name`, `None` for a
/// name that is not a valid player name.
#[must_use]
pub fn name_lookup_url(name: &str) -> Option<String> {
    is_valid_name(name).then(|| format!("{NAME_LOOKUP_URL}{name}"))
}

/// The URL of the session server profile of `uuid` (32 hex digits, no dashes),
/// `None` for anything else.
#[must_use]
pub fn profile_url(uuid: &str) -> Option<String> {
    is_valid_uuid(uuid).then(|| format!("{PROFILE_URL}{}", uuid.to_ascii_lowercase()))
}

#[derive(Deserialize)]
struct NameLookup {
    id: String,
}

/// The profile id of a name lookup answer (`{"id": "...", "name": "..."}`).
///
/// # Errors
///
/// [`AssetsError::Json`] for a document of another shape, and
/// [`AssetsError::Image`] when the id is not 32 hex digits.
pub fn parse_name_lookup(json: &[u8]) -> AssetsResult<String> {
    let lookup: NameLookup = serde_json::from_slice(json).map_err(|source| AssetsError::Json {
        what: "name lookup".to_string(),
        source,
    })?;
    if is_valid_uuid(&lookup.id) {
        Ok(lookup.id.to_ascii_lowercase())
    } else {
        Err(image_error("name lookup", "the id is not a profile id"))
    }
}

#[derive(Deserialize)]
struct ProfileBody {
    #[serde(default)]
    properties: Vec<ProfileProperty>,
}

#[derive(Deserialize)]
struct ProfileProperty {
    name: String,
    value: String,
}

/// The skin and cape of a session server profile answer: the `textures`
/// property among its `properties`. A profile without one is `Ok` and empty.
///
/// # Errors
///
/// [`AssetsError::Json`] for a document of another shape, and the errors of
/// [`parse_textures_property`].
pub fn parse_profile_textures(json: &[u8]) -> AssetsResult<ProfileTextures> {
    let profile: ProfileBody =
        serde_json::from_slice(json).map_err(|source| AssetsError::Json {
            what: "profile".to_string(),
            source,
        })?;
    profile
        .properties
        .iter()
        .find(|property| property.name == "textures")
        .map_or_else(
            || Ok(ProfileTextures::default()),
            |property| parse_textures_property(&property.value),
        )
}

/// Finds the skin and cape of an account, whether it is a premium one or not,
/// on a worker thread, so the game never waits on the network.
///
/// A premium session knows its profile id (the launcher gave it), so the
/// profile is fetched directly. An offline account only has a name: the name
/// is looked up first, so a player who logs in offline with the name of an
/// account wears its skin. Either way the skin shows whatever the server
/// sends, which on an offline server is nothing.
#[derive(Debug)]
pub struct AccountSkinFetcher {
    results: Receiver<ProfileTextures>,
}

impl AccountSkinFetcher {
    /// Starts the lookup for the player `name`, with the profile id `uuid`
    /// when the launcher gave one.
    #[must_use]
    pub fn spawn(name: &str, uuid: Option<&str>) -> Self {
        let (sender, results) = mpsc::channel();
        let (name, uuid) = (name.to_owned(), uuid.map(str::to_owned));
        thread::Builder::new()
            .name("account-skin".to_owned())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                match runtime.block_on(resolve_account(&name, uuid.as_deref())) {
                    Ok(textures) if textures.skin.is_some() || textures.cape.is_some() => {
                        sender.send(textures).ok();
                    }
                    Ok(_) => tracing::debug!(player = %name, "the account has no skin"),
                    // No skin is not an error for the game: the player keeps
                    // the default skin of their UUID.
                    Err(error) => {
                        tracing::debug!(player = %name, %error, "account skin not found");
                    }
                }
            })
            .ok();
        Self { results }
    }

    /// The skin and cape of the account once found; `None` until then, and
    /// for good when the account has none.
    pub fn poll(&self) -> Option<ProfileTextures> {
        self.results.try_recv().ok()
    }
}

/// The profile id (given, or looked up by name), then its textures.
async fn resolve_account(name: &str, uuid: Option<&str>) -> AssetsResult<ProfileTextures> {
    let client = HttpClient::new(Endpoints::official())?;
    let id = match uuid.filter(|uuid| is_valid_uuid(uuid)) {
        Some(uuid) => uuid.to_ascii_lowercase(),
        None => {
            let url = name_lookup_url(name)
                .ok_or_else(|| image_error(name, "not a valid player name"))?;
            parse_name_lookup(&client.get_bytes_limited(&url, MAX_PROFILE_BYTES).await?)?
        }
    };
    let url = profile_url(&id).ok_or_else(|| image_error(&id, "not a profile id"))?;
    parse_profile_textures(&client.get_bytes_limited(&url, MAX_PROFILE_BYTES).await?)
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
    fn the_account_urls_only_take_a_valid_name_or_profile_id() {
        assert_eq!(
            name_lookup_url("Notch").as_deref(),
            Some("https://api.minecraftservices.com/minecraft/profile/lookup/name/Notch")
        );
        assert_eq!(
            profile_url("069A79F444E94726A5BEFCA90E38AAF5").as_deref(),
            Some(
                "https://sessionserver.mojang.com/session/minecraft/profile/069a79f444e94726a5befca90e38aaf5"
            )
        );
        // Nothing that could change the path or the host gets into a URL.
        for name in ["", "seventeen_chars_xx", "a/../b", "a?b", "a b", "é"] {
            assert_eq!(name_lookup_url(name), None, "{name:?}");
        }
        for id in ["", "1234", "../../x", "zz9a79f444e94726a5befca90e38aaf5"] {
            assert_eq!(profile_url(id), None, "{id:?}");
        }
    }

    #[test]
    fn a_name_lookup_answer_gives_the_profile_id() {
        let json = br#"{"id":"069A79F444E94726A5BEFCA90E38AAF5","name":"Notch"}"#;
        assert_eq!(
            parse_name_lookup(json).expect("an id"),
            "069a79f444e94726a5befca90e38aaf5"
        );
        assert!(parse_name_lookup(br#"{"id":"not-an-id"}"#).is_err());
        assert!(parse_name_lookup(br#"{"errorMessage":"no such player"}"#).is_err());
        assert!(parse_name_lookup(b"<html>").is_err());
    }

    #[test]
    fn a_profile_answer_gives_the_skin_and_cape_of_its_textures_property() {
        let textures = encode(
            r#"{"textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/abc"},"CAPE":{"url":"http://textures.minecraft.net/texture/def"}}}"#,
        );
        let profile = format!(
            r#"{{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","properties":[{{"name":"other","value":"x"}},{{"name":"textures","value":"{textures}"}}]}}"#
        );
        let parsed = parse_profile_textures(profile.as_bytes()).expect("a profile");
        assert_eq!(
            parsed.skin.as_deref(),
            Some("http://textures.minecraft.net/texture/abc")
        );
        assert_eq!(
            parsed.cape.as_deref(),
            Some("http://textures.minecraft.net/texture/def")
        );
        // No textures property, or no properties at all: an empty profile.
        let bare = br#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#;
        assert_eq!(
            parse_profile_textures(bare).expect("an empty profile"),
            ProfileTextures::default()
        );
        assert!(parse_profile_textures(b"nope").is_err());
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
