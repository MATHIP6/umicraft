// SPDX-License-Identifier: MIT OR Apache-2.0

//! Version manifest and version JSON (spec AST-1, first hop).
//!
//! The official download chain is:
//!
//! 1. the **version manifest** at
//!    `https://launchermeta.mojang.com/mc/game/version_manifest.json` lists
//!    every game version with the absolute URL of its version JSON;
//! 2. the **version JSON** (e.g. `1.8.8.json`) names the **asset index**
//!    (id, url, sha1, size) and the **client jar** (url, sha1, size).
//!
//! Both documents are small JSON files; the digests they carry are the
//! contract the transport layer verifies before anything is written to disk.

use serde::Deserialize;

use crate::error::{AssetsError, AssetsResult};
use crate::http::HttpClient;

/// The version manifest: every published game version and where its
/// version JSON lives.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    /// The most recent release and snapshot.
    pub latest: LatestVersions,
    /// Every published version, newest first.
    #[serde(default)]
    pub versions: Vec<VersionSummary>,
}

impl VersionManifest {
    /// Parses a version manifest document.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Json`] when the document is not valid JSON or does not
    /// have the expected shape.
    ///
    /// # Examples
    ///
    /// ```
    /// use umicraft_assets::manifest::VersionManifest;
    ///
    /// let json = r#"{"latest":{"release":"1.8.8","snapshot":"1.8.8"},
    ///                "versions":[{"id":"1.8.8","type":"release","url":"https://example.invalid/1.8.8.json",
    ///                              "time":"2021-01-01T00:00:00+00:00","releaseTime":"2015-07-27T00:00:00+00:00"}]}"#;
    /// let manifest = VersionManifest::parse(json.as_bytes()).unwrap();
    /// assert_eq!(manifest.find("1.8.8").unwrap().id, "1.8.8");
    /// ```
    pub fn parse(bytes: &[u8]) -> AssetsResult<Self> {
        serde_json::from_slice(bytes).map_err(|source| AssetsError::Json {
            what: "version manifest".to_string(),
            source,
        })
    }

    /// Looks up a version by its id, e.g. `"1.8.8"`.
    ///
    /// # Errors
    ///
    /// [`AssetsError::UnknownVersion`] when the manifest does not list it.
    pub fn find(&self, id: &str) -> AssetsResult<&VersionSummary> {
        self.versions
            .iter()
            .find(|version| version.id == id)
            .ok_or_else(|| AssetsError::UnknownVersion(id.to_string()))
    }

    /// Downloads and parses the version JSON of `version_id`.
    ///
    /// # Errors
    ///
    /// [`AssetsError::UnknownVersion`] when the version is not listed,
    /// [`AssetsError::Http`] when the version JSON cannot be downloaded, and
    /// [`AssetsError::Json`] when it cannot be parsed.
    pub async fn resolve(
        &self,
        client: &HttpClient,
        version_id: &str,
    ) -> AssetsResult<VersionJson> {
        let summary = self.find(version_id)?;
        let bytes = client.get_bytes(&summary.url).await?;
        VersionJson::parse(&bytes)
    }
}

/// The `latest` block of the version manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct LatestVersions {
    /// Id of the newest release.
    pub release: String,
    /// Id of the newest snapshot.
    pub snapshot: String,
}

/// One entry of the version manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionSummary {
    /// The version id, e.g. `"1.8.8"`.
    pub id: String,
    /// `"release"`, `"snapshot"`, or `"old_beta"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Absolute URL of the version JSON.
    pub url: String,
    /// When the version was published.
    #[serde(default)]
    pub time: String,
    /// When the version was released.
    #[serde(default, rename = "releaseTime")]
    pub release_time: String,
}

/// The parts of a version JSON the asset transport needs.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionJson {
    /// The version id.
    pub id: String,
    /// The asset index descriptor, when the version has one.
    #[serde(default, rename = "assetIndex")]
    pub asset_index: Option<AssetIndexDescriptor>,
    /// The legacy assets folder name, e.g. `"1.8"` (1.8 only).
    #[serde(default)]
    pub assets: Option<String>,
    /// The downloadable files of this version.
    #[serde(default)]
    pub downloads: Downloads,
}

impl VersionJson {
    /// Parses a version JSON document.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Json`] when the document is not valid JSON or does not
    /// have the expected shape.
    pub fn parse(bytes: &[u8]) -> AssetsResult<Self> {
        serde_json::from_slice(bytes).map_err(|source| AssetsError::Json {
            what: "version json".to_string(),
            source,
        })
    }

    /// The asset index descriptor of this version.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Missing`] when the version JSON has no asset index.
    pub fn require_asset_index(&self) -> AssetsResult<&AssetIndexDescriptor> {
        self.asset_index
            .as_ref()
            .ok_or_else(|| AssetsError::Missing {
                what: format!("asset index of version {}", self.id),
                path: self.id.clone(),
            })
    }

    /// The client jar descriptor of this version.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Missing`] when the version JSON has no client download.
    pub fn require_client_jar(&self) -> AssetsResult<&ClientJarDescriptor> {
        self.downloads
            .client
            .as_ref()
            .ok_or_else(|| AssetsError::Missing {
                what: format!("client jar of version {}", self.id),
                path: self.id.clone(),
            })
    }
}

/// The `downloads` block of a version JSON.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Downloads {
    /// The client jar, when the version ships one.
    #[serde(default)]
    pub client: Option<ClientJarDescriptor>,
}

/// The asset index descriptor: where the index lives and how big it is.
#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexDescriptor {
    /// The asset index id, e.g. `"1.8"`.
    pub id: String,
    /// SHA-1 of the index document.
    pub sha1: String,
    /// Size of the index document in bytes.
    pub size: u64,
    /// Combined size of every object the index references.
    #[serde(default, rename = "totalSize")]
    pub total_size: Option<u64>,
    /// Absolute URL of the index document.
    pub url: String,
}

/// The client jar descriptor: where the jar lives and how big it is.
#[derive(Debug, Clone, Deserialize)]
pub struct ClientJarDescriptor {
    /// SHA-1 of the jar.
    pub sha1: String,
    /// Size of the jar in bytes.
    pub size: u64,
    /// Absolute URL of the jar.
    pub url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
        "latest": {"release": "26.3", "snapshot": "26.4-snapshot-5"},
        "versions": [
            {"id": "1.8.9", "type": "release", "url": "https://piston-meta.mojang.com/v1/packages/aa/1.8.9.json",
             "time": "2021-12-15T15:44:12+00:00", "releaseTime": "2015-07-27T10:31:28+00:00"},
            {"id": "1.8.8", "type": "release", "url": "https://piston-meta.mojang.com/v1/packages/bb/1.8.8.json",
             "time": "2021-12-15T15:44:12+00:00", "releaseTime": "2015-07-27T10:31:28+00:00"}
        ]
    }"#;

    #[test]
    fn manifest_is_parsed_and_searched() {
        let manifest = VersionManifest::parse(MANIFEST.as_bytes()).expect("manifest parses");
        assert_eq!(manifest.latest.release, "26.3");
        assert_eq!(manifest.versions.len(), 2);
        assert_eq!(manifest.find("1.8.8").expect("1.8.8 is listed").id, "1.8.8");
        assert!(manifest.find("1.7.10").is_err());
    }

    #[test]
    fn a_broken_manifest_is_a_typed_error() {
        let err = VersionManifest::parse(b"{not json").expect_err("broken json must fail");
        assert!(matches!(err, AssetsError::Json { .. }));
    }

    #[test]
    fn version_json_exposes_the_asset_index_and_client_descriptors() {
        let json = r#"{
            "id": "1.8.8",
            "assets": "1.8",
            "assetIndex": {
                "id": "1.8",
                "sha1": "f6ad102bcaa53b1a58358f16e376d548d44933ec",
                "size": 78494,
                "totalSize": 114885064,
                "url": "https://launchermeta.mojang.com/v1/packages/f6ad102bcaa53b1a58358f16e376d548d44933ec/1.8.json"
            },
            "downloads": {
                "client": {
                    "sha1": "0983f08be6a4e624f5d85689d1aca869ed99c738",
                    "size": 8465313,
                    "url": "https://launcher.mojang.com/v1/objects/0983f08be6a4e624f5d85689d1aca869ed99c738/client.jar"
                }
            }
        }"#;

        let version = VersionJson::parse(json.as_bytes()).expect("version json parses");
        assert_eq!(version.id, "1.8.8");
        assert_eq!(version.assets.as_deref(), Some("1.8"));

        let index = version.require_asset_index().expect("asset index present");
        assert_eq!(index.id, "1.8");
        assert_eq!(index.size, 78_494);
        assert_eq!(index.total_size, Some(114_885_064));

        let client = version.require_client_jar().expect("client jar present");
        assert_eq!(client.sha1, "0983f08be6a4e624f5d85689d1aca869ed99c738");
        assert_eq!(client.size, 8_465_313);
    }

    #[test]
    fn a_version_without_assets_reports_a_missing_descriptor() {
        let version = VersionJson::parse(b"{\"id\":\"1.8.8\"}").expect("version json parses");
        assert!(version.require_asset_index().is_err());
        assert!(version.require_client_jar().is_err());
    }
}
