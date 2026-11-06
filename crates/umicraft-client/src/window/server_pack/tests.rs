// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

fn request(url: &str) -> ServerPackRequest {
    ServerPackRequest {
        url: url.to_string(),
        hash: HASH.to_string(),
    }
}

fn reply(result: ResourcePackResult) -> Effect {
    Effect::Reply {
        hash: HASH.to_string(),
        result,
    }
}

fn flow(mode: ResourcePackMode) -> ServerPackFlow {
    let mut flow = ServerPackFlow::default();
    flow.set_mode(mode);
    flow
}

fn download_generation(effects: &[Effect]) -> u64 {
    effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Download { generation, .. } => Some(*generation),
            _ => None,
        })
        .expect("a download was started")
}

#[test]
fn an_enabled_server_is_accepted_and_downloaded_at_once() {
    let mut flow = flow(ResourcePackMode::Enabled);
    let effects = flow.offer(request("https://example.net/p.zip"));
    assert_eq!(effects[0], reply(ResourcePackResult::Accepted));
    assert!(matches!(effects[1], Effect::Download { .. }));
    assert!(!flow.is_prompting());
}

#[test]
fn a_prompting_server_waits_for_the_answer() {
    let mut flow = flow(ResourcePackMode::Prompt);
    assert!(flow.offer(request("http://example.net/p.zip")).is_empty());
    assert!(flow.is_prompting());

    let effects = flow.answer(true);
    assert_eq!(effects[0], reply(ResourcePackResult::Accepted));
    assert!(matches!(effects[1], Effect::Download { .. }));
    assert!(!flow.is_prompting());
}

#[test]
fn refusing_the_question_declines_and_downloads_nothing() {
    let mut flow = flow(ResourcePackMode::Prompt);
    flow.offer(request("http://example.net/p.zip"));
    assert_eq!(
        flow.answer(false),
        vec![reply(ResourcePackResult::Declined)]
    );
    assert!(flow.answer(true).is_empty(), "the question is over");
}

#[test]
fn a_disabled_server_is_declined_without_a_question() {
    let mut flow = flow(ResourcePackMode::Disabled);
    assert_eq!(
        flow.offer(request("http://example.net/p.zip")),
        vec![reply(ResourcePackResult::Declined)]
    );
    assert!(!flow.is_prompting());
}

#[test]
fn an_invalid_or_local_offer_fails_in_every_mode() {
    for mode in [ResourcePackMode::Enabled, ResourcePackMode::Prompt] {
        for url in [
            "ftp://example.net/p.zip",
            "level://world.zip",
            "level://../x",
        ] {
            assert_eq!(
                flow(mode).offer(request(url)),
                vec![reply(ResourcePackResult::FailedDownload)],
                "{url} in {mode:?}"
            );
        }
    }
}

#[test]
fn a_second_offer_while_busy_is_declined() {
    let mut flow = flow(ResourcePackMode::Prompt);
    flow.offer(request("http://example.net/a.zip"));
    assert_eq!(
        flow.offer(request("http://example.net/b.zip")),
        vec![reply(ResourcePackResult::Declined)]
    );
    assert!(flow.is_prompting(), "the first question stays");
}

#[test]
fn a_finished_download_loads_the_pack_then_reports_it() {
    let mut flow = flow(ResourcePackMode::Enabled);
    let generation = download_generation(&flow.offer(request("http://example.net/p.zip")));
    let path = PathBuf::from("/cache/server-resourcepacks/pack");

    let effects = flow.downloaded(generation, Some(path.clone()));
    assert_eq!(
        effects,
        vec![
            Effect::Reload,
            reply(ResourcePackResult::SuccessfullyLoaded)
        ]
    );
    assert_eq!(flow.active_path(), Some(path.as_path()));
}

#[test]
fn a_failed_download_reports_the_failure_and_loads_nothing() {
    let mut flow = flow(ResourcePackMode::Enabled);
    let generation = download_generation(&flow.offer(request("http://example.net/p.zip")));
    assert_eq!(
        flow.downloaded(generation, None),
        vec![reply(ResourcePackResult::FailedDownload)]
    );
    assert_eq!(flow.active_path(), None);
}

#[test]
fn ending_the_session_drops_the_pack_and_ignores_a_late_download() {
    let mut flow = flow(ResourcePackMode::Enabled);
    let generation = download_generation(&flow.offer(request("http://example.net/p.zip")));
    flow.downloaded(generation, Some(PathBuf::from("/cache/pack")));

    assert_eq!(flow.end_session(), vec![Effect::Reload]);
    assert_eq!(flow.active_path(), None);
    assert!(flow.end_session().is_empty(), "nothing left to reload");

    let late = download_generation(&flow.offer(request("http://example.net/q.zip")));
    flow.end_session();
    assert!(flow
        .downloaded(late, Some(PathBuf::from("/cache/q")))
        .is_empty());
    assert_eq!(flow.active_path(), None);
}

#[test]
fn a_new_pack_replaces_the_active_one_only_when_it_loads() {
    let mut flow = flow(ResourcePackMode::Enabled);
    let first = download_generation(&flow.offer(request("http://example.net/a.zip")));
    flow.downloaded(first, Some(PathBuf::from("/cache/a")));

    let second = download_generation(&flow.offer(request("http://example.net/b.zip")));
    flow.downloaded(second, None);
    assert_eq!(flow.active_path(), Some(Path::new("/cache/a")));
}
