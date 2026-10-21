// SPDX-License-Identifier: MIT OR Apache-2.0

//! End-to-end tests of the asset transport layer.
//!
//! Every test runs against a local fake HTTP server (see [`common`]); the
//! suite never touches the network and never writes inside the repository.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use umicraft_assets::cache::AssetCache;
use umicraft_assets::error::AssetsError;
use umicraft_assets::hash::sha1_hex;
use umicraft_assets::http::{Endpoints, HttpClient};
use umicraft_assets::install::OfficialInstall;
use umicraft_assets::manifest::VersionManifest;
use umicraft_assets::objects::{AssetIndex, Downloader, FetchMode};
use umicraft_assets::progress::Progress;

mod common;

use common::Route;

/// The payload of the fake asset object.
const OBJECT: &[u8] = b"the real asset object";

/// A temp directory that is removed when the test ends.
fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "umicraft-assets-test-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// The path the fake server serves an object under.
fn object_route(hash: &str) -> String {
    format!("/objects/{}/{}", &hash[..2], hash)
}

/// Builds the asset index document of the fake object.
fn index_document(hash: &str, size: u64) -> String {
    let mut document =
        String::from("{\"objects\":{\"minecraft/sounds/step/grass1.ogg\":{\"hash\":\"");
    document.push_str(hash);
    document.push_str("\",\"size\":");
    document.push_str(&size.to_string());
    document.push_str("}}}");
    document
}

/// Counts the files below `root`.
fn count_files(root: &std::path::Path) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            count += count_files(&path);
        } else {
            count += 1;
        }
    }
    count
}

#[tokio::test]
async fn the_manifest_and_the_version_json_are_parsed() {
    let index_bytes = index_document(&sha1_hex(OBJECT), OBJECT.len() as u64).into_bytes();
    let index_sha1 = sha1_hex(&index_bytes);
    let jar_sha1 = sha1_hex(b"client.jar");

    let server = common::start(|base| {
        let manifest = format!(
            r#"{{"latest":{{"release":"1.8.8","snapshot":"1.8.8"}},
                 "versions":[{{"id":"1.8.8","type":"release","url":"{base}/1.8.8.json",
                 "time":"2021-12-15T15:44:12+00:00","releaseTime":"2015-07-27T10:31:28+00:00"}}]}}"#
        );
        let version = format!(
            r#"{{"id":"1.8.8","assets":"1.8",
                 "assetIndex":{{"id":"1.8","sha1":"{index_sha1}","size":{index_size},
                 "totalSize":{index_size},"url":"{base}/1.8.json"}},
                 "downloads":{{"client":{{"sha1":"{jar_sha1}","size":9,"url":"{base}/client.jar"}}}}}}"#,
            index_size = index_bytes.len()
        );
        let mut routes = HashMap::new();
        routes.insert(
            "/version_manifest.json".to_string(),
            Route::Body(manifest.into_bytes()),
        );
        routes.insert("/1.8.8.json".to_string(), Route::Body(version.into_bytes()));
        routes.insert("/1.8.json".to_string(), Route::Body(index_bytes.clone()));
        routes
    })
    .await;

    let client = HttpClient::new(Endpoints::rooted_at(&server.base_url())).expect("client builds");
    let manifest_bytes = client
        .get_bytes(client.endpoints().manifest_url())
        .await
        .expect("manifest downloads");
    let manifest = VersionManifest::parse(&manifest_bytes).expect("manifest parses");
    assert_eq!(manifest.latest.release, "1.8.8");

    let summary = manifest.find("1.8.8").expect("1.8.8 is listed");
    assert_eq!(summary.kind, "release");
    assert!(summary.url.ends_with("/1.8.8.json"));

    let version = manifest
        .resolve(&client, "1.8.8")
        .await
        .expect("version json resolves");
    assert_eq!(version.id, "1.8.8");
    assert_eq!(version.assets.as_deref(), Some("1.8"));

    let index_descriptor = version
        .require_asset_index()
        .expect("the version has an asset index");
    assert_eq!(index_descriptor.id, "1.8");
    assert_eq!(index_descriptor.size, index_bytes.len() as u64);
    assert_eq!(index_descriptor.sha1, index_sha1);

    let client_descriptor = version
        .require_client_jar()
        .expect("the version has a client jar");
    assert_eq!(client_descriptor.sha1, jar_sha1);
    assert_eq!(client_descriptor.size, 9);

    // The index document is fetched and verified against its digest.
    let index = AssetIndex::fetch(&client, index_descriptor)
        .await
        .expect("the asset index downloads");
    assert_eq!(index.len(), 1);
    assert!(index.get("minecraft/sounds/step/grass1.ogg").is_some());
}

#[tokio::test]
async fn an_object_is_downloaded_verified_and_cached() {
    let hash = sha1_hex(OBJECT);
    let index_bytes = index_document(&hash, OBJECT.len() as u64).into_bytes();
    let server = common::start(|_| {
        let mut routes = HashMap::new();
        routes.insert(object_route(&hash), Route::Body(OBJECT.to_vec()));
        routes
    })
    .await;

    let cache = AssetCache::new(temp_root("download"));
    let index = AssetIndex::parse(&index_bytes).expect("index parses");
    let client = HttpClient::new(Endpoints::rooted_at(&server.base_url())).expect("client builds");
    let downloader = Downloader::new(client, 4);
    let progress = Progress::new();

    let report = downloader
        .fetch(
            &index,
            Arc::new(cache.clone()),
            FetchMode::Download,
            &progress,
        )
        .await
        .expect("the object downloads");

    assert_eq!(report.objects_total, 1);
    assert_eq!(report.downloaded, 1);
    assert_eq!(report.cached, 0);
    assert_eq!(report.bytes_downloaded, OBJECT.len() as u64);

    // The verified bytes are in the cache.
    let stored = cache.read_object(&hash).expect("the object is cached");
    assert_eq!(stored, OBJECT);
    assert!(cache.object_path(&hash).is_file());

    // The progress is reported to the UI.
    let snapshot = progress.snapshot();
    assert_eq!(snapshot.objects_done, 1);
    assert_eq!(snapshot.objects_total, 1);
    assert!(snapshot.is_complete());

    let _ = std::fs::remove_dir_all(cache.root());
}

#[tokio::test]
async fn a_corrupted_object_is_rejected_and_the_cache_stays_clean() {
    let hash = sha1_hex(OBJECT);
    let index_bytes = index_document(&hash, OBJECT.len() as u64).into_bytes();
    // Same length, different content: only the SHA-1 can catch it.
    let corrupted = b"THE REAL ASSET OBJECT";
    assert_eq!(corrupted.len(), OBJECT.len());

    let server = common::start(|_| {
        let mut routes = HashMap::new();
        routes.insert(object_route(&hash), Route::Body(corrupted.to_vec()));
        routes
    })
    .await;

    let cache = AssetCache::new(temp_root("corrupt"));
    let index = AssetIndex::parse(&index_bytes).expect("index parses");
    let client = HttpClient::new(Endpoints::rooted_at(&server.base_url())).expect("client builds");
    let downloader = Downloader::new(client, 4);

    let err = downloader
        .fetch(
            &index,
            Arc::new(cache.clone()),
            FetchMode::Download,
            &Progress::new(),
        )
        .await
        .expect_err("a corrupted object must be rejected");
    assert!(
        matches!(err, AssetsError::HashMismatch { .. }),
        "unexpected error: {err}"
    );

    // Nothing was written to the cache.
    assert!(!cache.object_path(&hash).exists());
    assert_eq!(count_files(&cache.objects_dir()), 0);

    let _ = std::fs::remove_dir_all(cache.root());
}

#[tokio::test]
async fn a_cached_object_is_not_downloaded_again() {
    let hash = sha1_hex(OBJECT);
    let index_bytes = index_document(&hash, OBJECT.len() as u64).into_bytes();
    let server = common::start(|_| {
        let mut routes = HashMap::new();
        routes.insert(object_route(&hash), Route::Body(OBJECT.to_vec()));
        routes
    })
    .await;

    let cache = AssetCache::new(temp_root("resume"));
    let index = AssetIndex::parse(&index_bytes).expect("index parses");
    let client = HttpClient::new(Endpoints::rooted_at(&server.base_url())).expect("client builds");
    let downloader = Downloader::new(client, 4);

    let first = downloader
        .fetch(
            &index,
            Arc::new(cache.clone()),
            FetchMode::Download,
            &Progress::new(),
        )
        .await
        .expect("the first run downloads");
    assert_eq!(first.downloaded, 1);
    assert_eq!(server.request_count(&object_route(&hash)), 1);

    // The second run finds the object in the cache and skips the transfer.
    let second = downloader
        .fetch(
            &index,
            Arc::new(cache.clone()),
            FetchMode::Download,
            &Progress::new(),
        )
        .await
        .expect("the second run resumes");
    assert_eq!(second.downloaded, 0);
    assert_eq!(second.cached, 1);
    assert_eq!(server.request_count(&object_route(&hash)), 1);

    let _ = std::fs::remove_dir_all(cache.root());
}

#[tokio::test]
async fn a_truncated_response_is_an_error_and_not_a_panic() {
    let hash = sha1_hex(OBJECT);
    let index_bytes = index_document(&hash, OBJECT.len() as u64).into_bytes();
    let server = common::start(|_| {
        let mut routes = HashMap::new();
        routes.insert(
            object_route(&hash),
            Route::Truncated {
                body: OBJECT[..4].to_vec(),
                declared_len: OBJECT.len(),
            },
        );
        routes
    })
    .await;

    let cache = AssetCache::new(temp_root("truncated"));
    let index = AssetIndex::parse(&index_bytes).expect("index parses");
    let client = HttpClient::new(Endpoints::rooted_at(&server.base_url())).expect("client builds");
    let downloader = Downloader::new(client, 4);

    let err = downloader
        .fetch(
            &index,
            Arc::new(cache.clone()),
            FetchMode::Download,
            &Progress::new(),
        )
        .await
        .expect_err("a truncated response must be an error");
    assert!(
        matches!(err, AssetsError::Http { .. }),
        "unexpected error: {err}"
    );

    // The truncated transfer never reached the cache.
    assert!(!cache.object_path(&hash).exists());
    assert_eq!(count_files(&cache.objects_dir()), 0);

    let _ = std::fs::remove_dir_all(cache.root());
}

#[tokio::test]
async fn the_pointer_mode_finds_the_assets_without_copying_them() {
    let hash = sha1_hex(OBJECT);
    let index_bytes = index_document(&hash, OBJECT.len() as u64).into_bytes();
    let server = common::start(|_| HashMap::new()).await;

    // A fake `.minecraft` directory that already holds the object.
    let install_root = temp_root("install");
    let install = OfficialInstall::new(&install_root);
    let object_dir = install.objects_dir().join(&hash[..2]);
    std::fs::create_dir_all(&object_dir).expect("the object dir is created");
    std::fs::write(object_dir.join(&hash), OBJECT).expect("the object is written");

    let index = AssetIndex::parse(&index_bytes).expect("index parses");
    let client = HttpClient::new(Endpoints::rooted_at(&server.base_url())).expect("client builds");
    let downloader = Downloader::new(client, 4);

    let report = downloader
        .fetch(
            &index,
            Arc::new(install.clone()),
            FetchMode::ReadOnly,
            &Progress::new(),
        )
        .await
        .expect("the installation provides the object");
    assert_eq!(report.cached, 1);
    assert_eq!(report.downloaded, 0);

    // Nothing was downloaded and nothing was copied.
    assert!(server.requests().is_empty());
    assert_eq!(count_files(&install_root), 1);
    assert!(!install_root
        .join("assets/objects")
        .join(&hash[..2])
        .join(&hash)
        .is_symlink());

    // A missing object is an error in pointer mode: the installation is
    // never written to.
    let missing_hash = sha1_hex(b"another object");
    let missing_index = index_document(&missing_hash, 13);
    let missing_index = AssetIndex::parse(missing_index.as_bytes()).expect("index parses");
    let err = downloader
        .fetch(
            &missing_index,
            Arc::new(install.clone()),
            FetchMode::ReadOnly,
            &Progress::new(),
        )
        .await
        .expect_err("a missing object must be reported");
    assert!(
        matches!(err, AssetsError::Missing { .. }),
        "unexpected error: {err}"
    );
    assert_eq!(count_files(&install_root), 1);

    let _ = std::fs::remove_dir_all(&install_root);
}
