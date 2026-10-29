// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP plumbing for the official asset endpoints.
//!
//! The transport layer only needs plain `GET` requests against Mojang's
//! official distribution endpoints (spec §3 rule 4: assets are downloaded by
//! the user, never mirrored by Umicraft). This module owns the endpoint
//! configuration and the small set of request helpers the rest of the crate
//! uses; it deliberately contains no asset knowledge.

use std::path::Path;
use std::time::Duration;

use sha1::Digest;
use tokio::io::AsyncWriteExt;

use crate::error::{AssetsError, AssetsResult};
use crate::hash;
use crate::progress::Progress;

/// The official version manifest endpoint.
///
/// Verified live on 2026-10-21 (see `docs/sources.md`).
pub const OFFICIAL_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest.json";

/// The official asset object endpoint prefix.
///
/// The **asset objects** of the 1.8 asset index live on the classic Mojang
/// resource host, not on the launcher object store: an object is served from
/// `<base>/<hash[0..2]>/<hash>`. Verified live on 2026-10-21 — the launcher
/// object store (`https://launcher.mojang.com/v1/objects/...`) answers `404`
/// for those hashes; it serves the client jar and the libraries, whose URLs
/// the version JSON carries explicitly.
pub const OFFICIAL_OBJECT_BASE: &str = "https://resources.download.minecraft.net";

/// The official version JSON endpoint prefix.
///
/// The version manifest already carries the absolute URL of every version
/// JSON, so this constant is only used as a documented default.
pub const OFFICIAL_VERSION_BASE: &str = "https://piston-meta.mojang.com/v1/packages";

/// The official asset index endpoint prefix.
///
/// The version JSON carries the absolute URL of the asset index, so this
/// constant is only used as a documented default.
pub const OFFICIAL_ASSET_INDEX_BASE: &str = "https://launchermeta.mojang.com/v1/packages";

/// Where the transport layer fetches from.
///
/// The official endpoints are the default; the struct is configurable so the
/// unit tests can point it at a local HTTP server (the test suite never
/// touches the network, spec §11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    manifest_url: String,
    object_base: String,
}

impl Endpoints {
    /// The official Mojang endpoints.
    ///
    /// # Examples
    ///
    /// ```
    /// use umicraft_assets::http::Endpoints;
    ///
    /// let endpoints = Endpoints::official();
    /// assert!(endpoints.manifest_url().starts_with("https://launchermeta.mojang.com/"));
    /// ```
    #[must_use]
    pub fn official() -> Self {
        Self {
            manifest_url: OFFICIAL_MANIFEST_URL.to_string(),
            object_base: OFFICIAL_OBJECT_BASE.to_string(),
        }
    }

    /// Endpoints rooted at a local server, for tests and offline mirrors.
    ///
    /// The manifest is served from `<base>/version_manifest.json` and the
    /// objects from `<base>/objects`.
    ///
    /// # Examples
    ///
    /// ```
    /// use umicraft_assets::http::Endpoints;
    ///
    /// let endpoints = Endpoints::rooted_at("http://127.0.0.1:8080");
    /// assert_eq!(
    ///     endpoints.manifest_url(),
    ///     "http://127.0.0.1:8080/version_manifest.json"
    /// );
    /// ```
    #[must_use]
    pub fn rooted_at(base: &str) -> Self {
        let base = base.trim_end_matches('/');
        Self {
            manifest_url: format!("{base}/version_manifest.json"),
            object_base: format!("{base}/objects"),
        }
    }

    /// The URL of the version manifest.
    #[must_use]
    pub fn manifest_url(&self) -> &str {
        &self.manifest_url
    }

    /// The prefix under which asset objects are stored.
    #[must_use]
    pub fn object_base(&self) -> &str {
        &self.object_base
    }

    /// The URL of the asset object with the given SHA-1 `hash`.
    ///
    /// # Examples
    ///
    /// ```
    /// use umicraft_assets::http::Endpoints;
    ///
    /// let endpoints = Endpoints::rooted_at("http://127.0.0.1:8080");
    /// let hash = "0983f08be6a4e624f5d85689d1aca869ed99c738";
    /// assert_eq!(
    ///     endpoints.object_url(hash),
    ///     "http://127.0.0.1:8080/objects/09/0983f08be6a4e624f5d85689d1aca869ed99c738"
    /// );
    /// ```
    #[must_use]
    pub fn object_url(&self, hash: &str) -> String {
        crate::objects::object_url(&self.object_base, hash)
    }
}

/// A small HTTP client bound to a set of [`Endpoints`].
///
/// The client is cheap to clone (it shares one connection pool) and is meant
/// to be created once and shared by the download workers.
#[derive(Debug, Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    endpoints: Endpoints,
}

impl HttpClient {
    /// Builds a client for `endpoints`.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Http`] when the underlying TLS backend cannot be set up.
    pub fn new(endpoints: Endpoints) -> AssetsResult<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("umicraft-assets/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(30))
            // A whole asset object is small, but the client jar is ~8.5 MB;
            // the budget is generous enough for a slow link.
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|source| AssetsError::Http {
                url: endpoints.manifest_url().to_string(),
                source,
            })?;
        Ok(Self { client, endpoints })
    }

    /// The endpoints this client talks to.
    #[must_use]
    pub fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    /// Downloads `url` into memory.
    ///
    /// The bytes are **not** verified here; the caller knows the expected
    /// digest and size and applies them (see [`crate::objects`]).
    ///
    /// # Errors
    ///
    /// [`AssetsError::Http`] on a transport failure (including a truncated
    /// body) and [`AssetsError::HttpStatus`] on a non-success status.
    pub async fn get_bytes(&self, url: &str) -> AssetsResult<Vec<u8>> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|source| AssetsError::Http {
                url: url.to_string(),
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(AssetsError::HttpStatus {
                url: url.to_string(),
                status: status.as_u16(),
            });
        }
        let bytes = response.bytes().await.map_err(|source| AssetsError::Http {
            url: url.to_string(),
            source,
        })?;
        Ok(bytes.to_vec())
    }

    /// Downloads `url` into memory, refusing a body larger than `max_bytes`.
    ///
    /// For untrusted-size bodies (player textures): the body is read in
    /// chunks and the download stops as soon as the limit is crossed.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Http`], [`AssetsError::HttpStatus`], or
    /// [`AssetsError::Image`] when the body is larger than `max_bytes`.
    pub async fn get_bytes_limited(&self, url: &str, max_bytes: usize) -> AssetsResult<Vec<u8>> {
        let http_error = |source| AssetsError::Http {
            url: url.to_string(),
            source,
        };
        let mut response = self.client.get(url).send().await.map_err(http_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(AssetsError::HttpStatus {
                url: url.to_string(),
                status: status.as_u16(),
            });
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(http_error)? {
            if body.len() + chunk.len() > max_bytes {
                return Err(crate::error::image_error(
                    url,
                    format!("larger than {max_bytes} bytes"),
                ));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }

    /// Downloads `url` into `dest`, verifying the SHA-1 and the size while
    /// the bytes stream in.
    ///
    /// The file is written to a sibling `.part` file first and only renamed
    /// to `dest` after the digest matched, so an interrupted or corrupted
    /// download never leaves a half-written file behind.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Http`], [`AssetsError::HttpStatus`],
    /// [`AssetsError::HashMismatch`], [`AssetsError::SizeMismatch`], or
    /// [`AssetsError::Io`].
    pub async fn download_to_file(
        &self,
        url: &str,
        dest: &Path,
        expected_sha1: &str,
        expected_size: u64,
        progress: Option<&Progress>,
    ) -> AssetsResult<()> {
        let mut response =
            self.client
                .get(url)
                .send()
                .await
                .map_err(|source| AssetsError::Http {
                    url: url.to_string(),
                    source,
                })?;
        let status = response.status();
        if !status.is_success() {
            return Err(AssetsError::HttpStatus {
                url: url.to_string(),
                status: status.as_u16(),
            });
        }

        let temp = temp_path(dest);
        let write_result = self
            .write_stream(&mut response, &temp, expected_sha1, expected_size, progress)
            .await;
        if let Err(err) = write_result {
            // A partially written temporary file must never be mistaken for a
            // complete download.
            let _ = tokio::fs::remove_file(&temp).await;
            return Err(err);
        }
        tokio::fs::rename(&temp, dest)
            .await
            .map_err(|source| AssetsError::Io {
                path: dest.display().to_string(),
                source,
            })
    }

    /// Streams the response body into `temp`, hashing as it goes.
    async fn write_stream(
        &self,
        response: &mut reqwest::Response,
        temp: &Path,
        expected_sha1: &str,
        expected_size: u64,
        progress: Option<&Progress>,
    ) -> AssetsResult<()> {
        let mut file = tokio::fs::File::create(temp)
            .await
            .map_err(|source| AssetsError::Io {
                path: temp.display().to_string(),
                source,
            })?;
        let mut hasher = sha1::Sha1::new();
        let mut written = 0_u64;

        loop {
            let chunk = response.chunk().await.map_err(|source| AssetsError::Http {
                url: response.url().to_string(),
                source,
            })?;
            let Some(chunk) = chunk else { break };
            hasher.update(&chunk);
            written += chunk.len() as u64;
            file.write_all(&chunk)
                .await
                .map_err(|source| AssetsError::Io {
                    path: temp.display().to_string(),
                    source,
                })?;
            if let Some(progress) = progress {
                progress.add_bytes(chunk.len() as u64);
            }
        }
        file.flush().await.map_err(|source| AssetsError::Io {
            path: temp.display().to_string(),
            source,
        })?;

        let actual = hash::to_hex(&hasher.finalize());
        if actual != expected_sha1 {
            return Err(AssetsError::HashMismatch {
                what: expected_sha1.to_string(),
                expected: expected_sha1.to_string(),
                actual,
            });
        }
        if written != expected_size {
            return Err(AssetsError::SizeMismatch {
                what: expected_sha1.to_string(),
                expected: expected_size,
                actual: written,
            });
        }
        Ok(())
    }
}

/// The temporary path a download writes to before it is renamed into place.
///
/// The name is unique per call, so two downloads of the same file cannot
/// clobber each other's partial file.
fn temp_path(dest: &Path) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".part{serial}"));
    dest.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_endpoints_are_the_documented_urls() {
        let endpoints = Endpoints::official();
        assert_eq!(
            endpoints.manifest_url(),
            "https://launchermeta.mojang.com/mc/game/version_manifest.json"
        );
        assert_eq!(
            endpoints.object_base(),
            "https://resources.download.minecraft.net"
        );
    }

    #[test]
    fn rooted_endpoints_build_object_urls() {
        let endpoints = Endpoints::rooted_at("http://127.0.0.1:1234/");
        assert_eq!(
            endpoints.manifest_url(),
            "http://127.0.0.1:1234/version_manifest.json"
        );
        assert_eq!(
            endpoints.object_url("f6ad102bcaa53b1a58358f16e376d548d44933ec"),
            "http://127.0.0.1:1234/objects/f6/f6ad102bcaa53b1a58358f16e376d548d44933ec"
        );
    }

    #[test]
    fn temp_path_is_a_sibling_of_the_destination() {
        let dest = Path::new("/tmp/umicraft/client.jar");
        let temp = temp_path(dest);
        assert_eq!(temp.parent(), dest.parent());
        assert!(temp
            .file_name()
            .expect("the temp file has a name")
            .to_string_lossy()
            .starts_with("client.jar.part"));
    }

    #[test]
    fn temp_paths_are_unique_per_call() {
        let dest = Path::new("/tmp/umicraft/client.jar");
        assert_ne!(temp_path(dest), temp_path(dest));
    }
}
