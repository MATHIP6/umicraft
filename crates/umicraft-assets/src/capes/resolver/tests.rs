// SPDX-License-Identifier: MIT OR Apache-2.0

//! Resolver tests: the redirect rule, and whole lookups against a local
//! server standing in for every provider.

use std::time::{Duration, Instant};

use super::*;
use crate::capes::test_server::{Reply, TestServer};
use crate::capes::test_support::{base64_encode, palette_png_22x17, rgba_png, solid};

/// A version 4 UUID: laby path `00000000-0000-4000-8000-000000000001`.
const UUID: u128 = (4 << 76) | (0x8 << 60) | 1;
const LABY_PATH: &str = "/capes/00000000-0000-4000-8000-000000000001";
const OPTI_PATH: &str = "/capes/Steve.png";
const MC_PATH: &str = "/profile/00000000000040008000000000000001";
const COSMETICA_PATH: &str = "/users/00000000-0000-4000-8000-000000000001/cape";

fn query(uuid: u128, name: &str, providers: &[CapeProvider]) -> CapeQuery {
    CapeQuery {
        owner: CapeOwner {
            uuid,
            name: name.to_owned(),
        },
        providers: providers.to_vec(),
    }
}

/// Runs one query on a fresh resolver aimed at `server`.
fn resolve(server: &TestServer, query: CapeQuery) -> CapeResult {
    let resolver = CapeResolver::spawn_with(CapeEndpoints::local(&server.origin));
    assert!(resolver.request(query));
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(result) = resolver.poll().next() {
            return result;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no result within 10 s");
}

fn route(path: &str, reply: Reply) -> (String, Reply) {
    (path.to_owned(), reply)
}

#[test]
fn redirects_stay_on_allowed_hosts_and_are_capped() {
    let good = Url::parse("https://items.laby.net/a.png").unwrap();
    let bad = Url::parse("https://evil.example/a.png").unwrap();
    assert!(follows_redirect(1, &good, is_allowed_cape_url));
    assert!(follows_redirect(MAX_REDIRECTS, &good, is_allowed_cape_url));
    assert!(!follows_redirect(
        MAX_REDIRECTS + 1,
        &good,
        is_allowed_cape_url
    ));
    assert!(!follows_redirect(1, &bad, is_allowed_cape_url));
}

#[test]
fn the_endpoints_move_only_the_origin() {
    let local = CapeEndpoints::local("http://127.0.0.1:9");
    assert_eq!(
        local.rewrite("http://s.optifine.net/capes/Steve.png".to_owned()),
        "http://127.0.0.1:9/capes/Steve.png"
    );
    let official = CapeEndpoints::official();
    assert_eq!(
        official.rewrite("http://s.optifine.net/capes/Steve.png".to_owned()),
        "http://s.optifine.net/capes/Steve.png"
    );
}

#[test]
fn the_first_provider_with_a_cape_wins_and_later_ones_are_not_asked() {
    let server = TestServer::start(|_| {
        vec![
            route(LABY_PATH, Reply::ok(palette_png_22x17())),
            route(OPTI_PATH, Reply::ok(palette_png_22x17())),
        ]
    });
    let result = resolve(&server, query(UUID, "Steve", &CapeProvider::ALL));
    let found = result.cape.expect("a cape");
    assert_eq!(result.uuid, UUID);
    assert_eq!(found.provider, CapeProvider::LabyMod);
    assert_eq!((found.image.width, found.image.height), (64, 32));
    assert_eq!(server.hits(), [LABY_PATH]);
}

#[test]
fn optifine_is_asked_last() {
    let server = TestServer::start(|_| {
        vec![
            route(MC_PATH, Reply::ok("{}")),
            route(OPTI_PATH, Reply::ok(palette_png_22x17())),
        ]
    });
    let result = resolve(&server, query(UUID, "Steve", &CapeProvider::ALL));
    assert_eq!(
        result.cape.map(|c| c.provider),
        Some(CapeProvider::OptiFine)
    );
    assert_eq!(
        server.hits(),
        [LABY_PATH, MC_PATH, COSMETICA_PATH, OPTI_PATH]
    );
}

#[test]
fn no_provider_having_a_cape_gives_none() {
    let server = TestServer::start(|_| Vec::new());
    let result = resolve(&server, query(UUID, "Steve", &CapeProvider::ALL));
    assert_eq!(
        result,
        CapeResult {
            uuid: UUID,
            cape: None
        }
    );
    assert_eq!(server.hits().len(), 4);
}

#[test]
fn an_owner_that_may_not_be_looked_up_sends_nothing() {
    let server = TestServer::start(|_| Vec::new());
    let offline = (3 << 76) | (0x8 << 60);
    let result = resolve(&server, query(offline, "Steve", &CapeProvider::ALL));
    assert_eq!(result.cape, None);
    assert!(server.hits().is_empty());
}

#[test]
fn only_the_asked_providers_are_used_in_the_given_order() {
    let server = TestServer::start(|_| vec![route(LABY_PATH, Reply::ok(palette_png_22x17()))]);
    let providers = [CapeProvider::Cosmetica, CapeProvider::LabyMod];
    let result = resolve(&server, query(UUID, "Steve", &providers));
    assert_eq!(result.cape.map(|c| c.provider), Some(CapeProvider::LabyMod));
    assert_eq!(server.hits(), [COSMETICA_PATH, LABY_PATH]);
}

#[test]
fn optifine_is_skipped_for_a_name_that_is_not_url_safe() {
    let server = TestServer::start(|_| vec![route(OPTI_PATH, Reply::ok(palette_png_22x17()))]);
    let result = resolve(&server, query(UUID, "a/../b", &CapeProvider::ALL));
    assert_eq!(result.cape, None);
    assert_eq!(server.hits(), [LABY_PATH, MC_PATH, COSMETICA_PATH]);
}

#[test]
fn a_failing_provider_does_not_stop_the_next_one() {
    let oversized = vec![0; MAX_IMAGE_BYTES + 1];
    let server = TestServer::start(|_| {
        vec![
            route(OPTI_PATH, Reply::ok(oversized)),
            route(LABY_PATH, Reply::ok("not a png")),
            route(MC_PATH, Reply::ok("not json")),
            route(
                COSMETICA_PATH,
                Reply::ok(r#"{"cloak":{"texture":"data:image/png;base64,AAAA"}}"#),
            ),
        ]
    });
    let result = resolve(&server, query(UUID, "Steve", &CapeProvider::ALL));
    assert_eq!(result.cape, None);
    assert_eq!(server.hits().len(), 4);
}

#[test]
fn a_json_provider_downloads_the_image_it_names() {
    let strip = rgba_png(16, 24, &solid(16, 24, [7, 7, 7, 255]));
    let server = TestServer::start(|origin| {
        let json = format!(r#"{{"animated_cape_url":"{origin}/img/strip.png","capeGlint":false}}"#);
        vec![
            route(MC_PATH, Reply::ok(json)),
            route("/img/strip.png", Reply::ok(strip)),
        ]
    });
    let providers = [CapeProvider::MinecraftCapes];
    let found = resolve(&server, query(UUID, "Steve", &providers))
        .cape
        .expect("a cape");
    assert_eq!(found.provider, CapeProvider::MinecraftCapes);
    assert_eq!((found.image.width, found.image.height), (16, 8));
    assert_eq!(found.image.frames.len(), 3);
}

#[test]
fn a_json_provider_can_carry_the_image_inline() {
    let inline = base64_encode(&rgba_png(8, 4, &solid(8, 4, [1, 2, 3, 255])));
    let server = TestServer::start(|_| {
        let json =
            format!(r#"{{"cloak":{{"texture":"data:image/png;base64,{inline}","frames":1}}}}"#);
        vec![route(COSMETICA_PATH, Reply::ok(json))]
    });
    let providers = [CapeProvider::Cosmetica];
    let found = resolve(&server, query(UUID, "Steve", &providers))
        .cape
        .expect("a cape");
    assert_eq!((found.image.width, found.image.height), (64, 32));
    assert_eq!(server.hits(), [COSMETICA_PATH]);
}

#[test]
fn a_full_queue_refuses_more_requests() {
    // A resolver whose worker is gone has a closed queue: `request` says so.
    let (requests, queue) = async_channel(1);
    let (_sender, results) = mpsc::sync_channel(1);
    let resolver = CapeResolver { requests, results };
    assert!(resolver.request(query(UUID, "a", &[])));
    assert!(
        !resolver.request(query(UUID, "a", &[])),
        "queue of one is full"
    );
    drop(queue);
    assert!(!resolver.request(query(UUID, "a", &[])), "worker gone");
}

/// Asks the real services about a well-known account, one provider at a
/// time, and prints what each answers. Needs the network, so it only runs
/// with `cargo test -p umicraft-assets live_services -- --ignored --nocapture`.
#[test]
#[ignore = "talks to the real cape services"]
fn live_services() {
    /// The public account `Notch`.
    const NOTCH: u128 = 0x069a_79f4_44e9_4726_a5be_fca9_0e38_aaf5;
    let resolver = CapeResolver::spawn();
    for provider in CapeProvider::ALL {
        assert!(resolver.request(query(NOTCH, "Notch", &[provider])));
        let deadline = Instant::now() + Duration::from_secs(20);
        let result = loop {
            if let Some(result) = resolver.poll().next() {
                break result;
            }
            assert!(
                Instant::now() < deadline,
                "{} did not answer",
                provider.key()
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        let found = result.cape.map(|found| {
            let image = found.image;
            format!(
                "{}x{}, {} frame(s)",
                image.width,
                image.height,
                image.frames.len()
            )
        });
        println!("{}: {found:?}", provider.key());
    }
}
