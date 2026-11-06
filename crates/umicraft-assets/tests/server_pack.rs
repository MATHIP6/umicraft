// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of server resource pack validation and fetching (NET-11).
//!
//! Downloads run against the local fake server in [`common`]; no network.

use std::collections::HashMap;
use std::path::PathBuf;

use umicraft_assets::hash::sha1_hex;
use umicraft_assets::server_pack::{
    fetch_server_pack, fetch_server_pack_blocking, server_pack_path, validate_offer,
    ServerPackError, ServerPackOffer, ValidatedOffer,
};

#[allow(dead_code)] // shared helper; this suite uses only some routes
mod common;

use common::Route;

/// The smallest valid zip: just an end-of-central-directory record.
fn empty_zip() -> Vec<u8> {
    let mut zip = vec![0x50, 0x4b, 0x05, 0x06];
    zip.extend_from_slice(&[0; 18]);
    zip
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "umicraft-assets-serverpack-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn offer(url: &str, hash: &str) -> ServerPackOffer {
    ServerPackOffer {
        url: url.to_string(),
        hash: hash.to_string(),
    }
}

fn remote(url: String, sha1: Option<String>) -> ValidatedOffer {
    ValidatedOffer::Remote { url, sha1 }
}

async fn serve(body: Vec<u8>) -> common::FakeServer {
    common::start(|_| HashMap::from([("/pack.zip".to_string(), Route::Body(body))])).await
}

#[test]
fn http_and_https_offers_are_accepted_and_other_schemes_rejected() {
    let hash = "a".repeat(40);
    assert_eq!(
        validate_offer(&offer("https://h/p.zip", &hash)).unwrap(),
        remote("https://h/p.zip".into(), Some(hash))
    );
    assert_eq!(
        validate_offer(&offer("http://h/p.zip", "")).unwrap(),
        remote("http://h/p.zip".into(), None)
    );
    for url in ["ftp://h/p.zip", "file:///etc/passwd", "p.zip", ""] {
        assert!(matches!(
            validate_offer(&offer(url, "")),
            Err(ServerPackError::UnsupportedScheme(_))
        ));
    }
}

#[test]
fn malformed_hash_is_treated_as_absent() {
    let upper = "A".repeat(40);
    let validated = validate_offer(&offer("http://h/p.zip", &upper)).unwrap();
    assert_eq!(validated, remote("http://h/p.zip".into(), None));
}

#[test]
fn level_offers_reject_path_traversal() {
    assert_eq!(
        validate_offer(&offer("level://pack.zip", "")).unwrap(),
        ValidatedOffer::Level("pack.zip".into())
    );
    for url in [
        "level://",
        "level://../x",
        "level://a/b",
        "level://a\\b",
        "level://..",
    ] {
        assert!(matches!(
            validate_offer(&offer(url, "")),
            Err(ServerPackError::InvalidLevelName(_))
        ));
    }
}

#[tokio::test]
async fn download_with_matching_hash_is_cached_and_parsed() {
    let zip = empty_zip();
    let hash = sha1_hex(&zip);
    let server = serve(zip.clone()).await;
    let root = temp_root("ok");
    let url = format!("{}/pack.zip", server.base_url());

    let offer = remote(url, Some(hash.clone()));
    let client = reqwest::Client::new();
    fetch_server_pack(&client, &offer, &root, 1024)
        .await
        .unwrap();

    let cached = root.join("server-resourcepacks").join(&hash);
    assert_eq!(std::fs::read(cached).unwrap(), zip);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn hash_mismatch_fails_and_leaves_no_file() {
    let server = serve(empty_zip()).await;
    let root = temp_root("mismatch");
    let wrong = "0".repeat(40);
    let url = format!("{}/pack.zip", server.base_url());

    let offer = remote(url, Some(wrong.clone()));
    let client = reqwest::Client::new();
    let err = fetch_server_pack(&client, &offer, &root, 1024).await;

    assert!(matches!(err, Err(ServerPackError::HashMismatch { .. })));
    let dir = root.join("server-resourcepacks");
    assert!(!dir.join(&wrong).exists());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn oversized_download_is_refused_and_leaves_no_file() {
    let server = serve(vec![0; 4096]).await;
    let root = temp_root("big");
    let url = format!("{}/pack.zip", server.base_url());

    let offer = remote(url, None);
    let client = reqwest::Client::new();
    let err = fetch_server_pack(&client, &offer, &root, 1000).await;

    assert!(matches!(
        err,
        Err(ServerPackError::TooLarge { max_bytes: 1000 })
    ));
    let dir = root.join("server-resourcepacks");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn cache_hit_makes_no_request() {
    let zip = empty_zip();
    let hash = sha1_hex(&zip);
    let server = serve(zip.clone()).await;
    let root = temp_root("hit");
    let dir = root.join("server-resourcepacks");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(&hash), &zip).unwrap();
    let url = format!("{}/pack.zip", server.base_url());

    let offer = remote(url, Some(hash));
    let client = reqwest::Client::new();
    fetch_server_pack(&client, &offer, &root, 1024)
        .await
        .unwrap();

    assert!(server.requests().is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn corrupt_cache_entry_is_replaced_by_a_fresh_download() {
    let zip = empty_zip();
    let hash = sha1_hex(&zip);
    let server = serve(zip.clone()).await;
    let root = temp_root("corrupt");
    let dir = root.join("server-resourcepacks");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(&hash), b"garbage").unwrap();
    let url = format!("{}/pack.zip", server.base_url());

    let offer = remote(url, Some(hash.clone()));
    let client = reqwest::Client::new();
    fetch_server_pack(&client, &offer, &root, 1024)
        .await
        .unwrap();

    assert_eq!(server.request_count("/pack.zip"), 1);
    assert_eq!(std::fs::read(dir.join(&hash)).unwrap(), zip);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn level_offer_is_not_fetchable() {
    let client = reqwest::Client::new();
    let offer = ValidatedOffer::Level("p.zip".into());
    let err = fetch_server_pack(&client, &offer, &temp_root("lvl"), 1024).await;
    assert!(matches!(err, Err(ServerPackError::LocalOffer(_))));
}

#[tokio::test]
async fn the_blocking_fetch_returns_the_cached_path() {
    let zip = empty_zip();
    let hash = sha1_hex(&zip);
    let server = serve(zip.clone()).await;
    let root = temp_root("blocking");
    let offer = remote(
        format!("{}/pack.zip", server.base_url()),
        Some(hash.clone()),
    );

    let (offer_for_thread, root_for_thread) = (offer.clone(), root.clone());
    let path = tokio::task::spawn_blocking(move || {
        fetch_server_pack_blocking(&offer_for_thread, &root_for_thread, 1024)
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(path, root.join("server-resourcepacks").join(&hash));
    assert_eq!(server_pack_path(&root, &offer), Some(path.clone()));
    assert_eq!(std::fs::read(path).unwrap(), zip);
    assert!(matches!(
        fetch_server_pack_blocking(&ValidatedOffer::Level("a.zip".into()), &root, 1),
        Err(ServerPackError::LocalOffer(_))
    ));
    let _ = std::fs::remove_dir_all(&root);
}
