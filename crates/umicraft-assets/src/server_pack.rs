// SPDX-License-Identifier: MIT OR Apache-2.0

//! Server resource packs (NET-11): validating an offer and fetching the pack.
//!
//! A server offers a pack as a `(url, sha1)` pair. This module is
//! protocol-free: the caller maps [`ServerPackError`] to the status reply it
//! sends back (every failure here is a `FAILED_DOWNLOAD` in vanilla terms).
//!
//! Behavior mirrors the reference client (`NetHandlerPlayClient.handleResourcePack`
//! and `ResourcePackRepository.downloadResourcePack`, see `docs/sources.md`):
//!
//! - a `level://<name>` offer names a file in the local saves directory;
//! - the cache file is `<cache_root>/server-resourcepacks/<sha1>` when the
//!   server sent a well-formed 40-hex hash, else the shared slot `legacy`
//!   (vanilla uses the same fixed name; such a pack is never reused because
//!   there is no hash to prove it matches);
//! - a cached file is reused only if its SHA-1 equals the offered hash;
//! - downloads are capped at [`VANILLA_MAX_BYTES`] (50 MiB in vanilla).
//!
//! Unlike vanilla, `level://` names containing `..` or a path separator are
//! rejected, so a server cannot make the client open files outside `saves/`.

use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;

use crate::error::AssetsError;
use crate::hash;
use crate::resource_pack::ResourcePack;

/// The download size cap of the reference client (`52428800` bytes).
pub const VANILLA_MAX_BYTES: u64 = 50 * 1024 * 1024;

/// Directory under the cache root that holds downloaded server packs.
const CACHE_DIR: &str = "server-resourcepacks";
/// Cache slot used when the server sent no usable hash (as in vanilla).
const LEGACY_SLOT: &str = "legacy";
const LEVEL_PREFIX: &str = "level://";

/// Why a server pack could not be validated, downloaded or loaded.
#[derive(Debug, thiserror::Error)]
pub enum ServerPackError {
    /// The URL scheme is not `http`, `https` or `level`.
    #[error("unsupported resource pack url `{0}`")]
    UnsupportedScheme(String),
    /// A `level://` name that is empty or tries to leave the saves directory.
    #[error("invalid level resource pack name `{0}`")]
    InvalidLevelName(String),
    /// [`fetch_server_pack`] was given a `level://` offer, which names a
    /// local file the caller must resolve itself.
    #[error("level resource pack `{0}` is a local file, not a download")]
    LocalOffer(String),
    /// The request failed at the transport level.
    #[error("downloading `{url}` failed: {source}")]
    Http {
        /// The offered URL.
        url: String,
        /// Underlying transport error.
        #[source]
        source: reqwest::Error,
    },
    /// The server answered with a non-success status.
    #[error("downloading `{url}` returned status {status}")]
    Status {
        /// The offered URL.
        url: String,
        /// The HTTP status code.
        status: u16,
    },
    /// The pack is larger than the allowed maximum.
    #[error("resource pack is larger than {max_bytes} bytes")]
    TooLarge {
        /// The cap that was exceeded.
        max_bytes: u64,
    },
    /// The downloaded pack does not match the offered SHA-1.
    #[error("resource pack sha1 mismatch: expected `{expected}`, got `{actual}`")]
    HashMismatch {
        /// The hash the server offered.
        expected: String,
        /// The hash of the downloaded bytes.
        actual: String,
    },
    /// The cache file could not be read or written.
    #[error("io error on `{path}`: {source}")]
    Io {
        /// The path that failed.
        path: String,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// The downloaded file is not a valid resource pack.
    #[error(transparent)]
    Pack(#[from] AssetsError),
}

/// A resource pack offer as received from the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPackOffer {
    /// Where to get the pack (`http`, `https` or `level://`).
    pub url: String,
    /// 40-hex SHA-1 of the pack; may be empty or malformed.
    pub hash: String,
}

/// An offer that passed [`validate_offer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatedOffer {
    /// A file in the local saves directory; the caller resolves it.
    Level(String),
    /// A pack to download.
    Remote {
        /// The `http(s)` URL.
        url: String,
        /// The offered SHA-1, only if it is 40 lowercase hex digits.
        sha1: Option<String>,
    },
}

/// Checks the URL scheme and, for `level://`, the file name.
///
/// # Errors
///
/// [`ServerPackError::UnsupportedScheme`] for any scheme but `http`,
/// `https` and `level`; [`ServerPackError::InvalidLevelName`] for an empty
/// name or one containing `..`, `/` or `\`.
///
/// # Examples
///
/// ```
/// use umicraft_assets::server_pack::{validate_offer, ServerPackOffer, ValidatedOffer};
///
/// let offer = ServerPackOffer { url: "level://pack.zip".into(), hash: String::new() };
/// assert_eq!(validate_offer(&offer).unwrap(), ValidatedOffer::Level("pack.zip".into()));
/// ```
pub fn validate_offer(offer: &ServerPackOffer) -> Result<ValidatedOffer, ServerPackError> {
    if let Some(name) = offer.url.strip_prefix(LEVEL_PREFIX) {
        let escapes = name.is_empty() || name.contains("..") || name.contains(['/', '\\']);
        return if escapes {
            Err(ServerPackError::InvalidLevelName(name.to_string()))
        } else {
            Ok(ValidatedOffer::Level(name.to_string()))
        };
    }
    let scheme = offer.url.split_once("://").map(|(scheme, _)| scheme);
    match scheme {
        Some(s) if s.eq_ignore_ascii_case("http") || s.eq_ignore_ascii_case("https") => {
            Ok(ValidatedOffer::Remote {
                url: offer.url.clone(),
                sha1: is_sha1_hex(&offer.hash).then(|| offer.hash.clone()),
            })
        }
        _ => Err(ServerPackError::UnsupportedScheme(offer.url.clone())),
    }
}

/// Vanilla's `^[a-f0-9]{40}$` check.
fn is_sha1_hex(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Returns the cache path of a pack with the given (optional) hash.
fn cache_path(cache_root: &Path, sha1: Option<&str>) -> PathBuf {
    cache_root.join(CACHE_DIR).join(sha1.unwrap_or(LEGACY_SLOT))
}

/// Gets the offered pack, from the cache when its hash matches, else by
/// downloading it, and parses it.
///
/// The download is streamed to a temporary file, aborted once it exceeds
/// `max_bytes` (see [`VANILLA_MAX_BYTES`]), verified against the offered
/// SHA-1 when there is one, and only then moved into the cache. A wrong
/// cached file is deleted.
///
/// # Errors
///
/// See [`ServerPackError`]; a [`ValidatedOffer::Level`] yields
/// [`ServerPackError::LocalOffer`].
pub async fn fetch_server_pack(
    client: &reqwest::Client,
    offer: &ValidatedOffer,
    cache_root: &Path,
    max_bytes: u64,
) -> Result<ResourcePack, ServerPackError> {
    let (url, sha1) = match offer {
        ValidatedOffer::Remote { url, sha1 } => (url, sha1),
        ValidatedOffer::Level(name) => return Err(ServerPackError::LocalOffer(name.clone())),
    };
    let dest = cache_path(cache_root, sha1.as_deref());
    if let Some(expected) = sha1 {
        if cached_file_matches(&dest, expected).await {
            return Ok(ResourcePack::from_zip_file(&dest)?);
        }
    }
    download(client, url, &dest, sha1.as_deref(), max_bytes).await?;
    Ok(ResourcePack::from_zip_file(&dest)?)
}

/// True if `path` exists with the expected hash; a wrong file is deleted.
async fn cached_file_matches(path: &Path, expected: &str) -> bool {
    let Ok(bytes) = tokio::fs::read(path).await else {
        return false;
    };
    if hash::sha1_hex(&bytes) == expected {
        return true;
    }
    tracing::warn!(path = %path.display(), "cached server pack has the wrong hash, deleting it");
    let _ = tokio::fs::remove_file(path).await;
    false
}

/// Downloads `url` to `dest` through a `.part` file, deleting it on failure.
async fn download(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected: Option<&str>,
    max_bytes: u64,
) -> Result<(), ServerPackError> {
    let io_error = |path: &Path| {
        let path = path.display().to_string();
        move |source| ServerPackError::Io { path, source }
    };
    let temp = dest.with_extension("part");
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(io_error(dir))?;
    }
    let result = stream_to_file(client, url, &temp, expected, max_bytes).await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
    }
    result?;
    tokio::fs::rename(&temp, dest).await.map_err(io_error(dest))
}

/// Streams the body into `temp`, enforcing the cap and the hash.
async fn stream_to_file(
    client: &reqwest::Client,
    url: &str,
    temp: &Path,
    expected: Option<&str>,
    max_bytes: u64,
) -> Result<(), ServerPackError> {
    let http_error = |source| ServerPackError::Http {
        url: url.to_string(),
        source,
    };
    let mut response = client.get(url).send().await.map_err(http_error)?;
    let status = response.status();
    if !status.is_success() {
        return Err(ServerPackError::Status {
            url: url.to_string(),
            status: status.as_u16(),
        });
    }
    if response.content_length().is_some_and(|len| len > max_bytes) {
        return Err(ServerPackError::TooLarge { max_bytes });
    }
    let io_error = |source| ServerPackError::Io {
        path: temp.display().to_string(),
        source,
    };
    let mut file = tokio::fs::File::create(temp).await.map_err(io_error)?;
    let mut hasher = Sha1::new();
    let mut written = 0_u64;
    while let Some(chunk) = response.chunk().await.map_err(http_error)? {
        written += chunk.len() as u64;
        if written > max_bytes {
            return Err(ServerPackError::TooLarge { max_bytes });
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(io_error)?;
    }
    file.flush().await.map_err(io_error)?;
    let actual = hash::to_hex(&hasher.finalize());
    match expected {
        Some(expected) if expected != actual => Err(ServerPackError::HashMismatch {
            expected: expected.to_string(),
            actual,
        }),
        _ => Ok(()),
    }
}
