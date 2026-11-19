// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the resolver: ordering, every rejection reason, cascades, determinism.

use proptest::prelude::*;
use umicraft_modhost::{
    resolve, ApiVersion, Candidate, Manifest, ModId, RejectReason, Resolution, ServiceId,
};

const HOST: ApiVersion = ApiVersion { major: 0, minor: 1 };

/// Builds a manifest from compact parts: dependencies as `(id, requirement)`.
fn manifest(id: &str, deps: &[(&str, &str)], priority: i32) -> Manifest {
    let mut text = format!(
        "[mod]\nid = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\nauthors = []\napi_version = \"0.1\"\n\
         [dependencies]\n"
    );
    for (dep, req) in deps {
        text.push_str(&format!("{dep} = \"{req}\"\n"));
    }
    text.push_str(&format!("[load]\npriority = {priority}\n"));
    Manifest::parse(&text).unwrap()
}

fn with_services(mut m: Manifest, replaces: &[&str], wraps: &[&str]) -> Manifest {
    m.load.replaces = replaces.iter().map(|s| s.parse().unwrap()).collect();
    m.load.wraps = wraps.iter().map(|s| s.parse().unwrap()).collect();
    m
}

fn on(m: Manifest) -> Candidate {
    Candidate {
        manifest: m,
        enabled: true,
    }
}

fn run(manifests: Vec<Manifest>) -> Resolution {
    resolve(manifests.into_iter().map(on), HOST)
}

fn order(r: &Resolution) -> Vec<&str> {
    r.order.iter().map(|m| m.id.as_str()).collect()
}

fn reason<'a>(r: &'a Resolution, id: &str) -> &'a RejectReason {
    &r.rejected
        .iter()
        .find(|x| x.id.as_str() == id)
        .unwrap()
        .reason
}

fn id(text: &str) -> ModId {
    text.parse().unwrap()
}

#[test]
fn orders_by_priority_then_id() {
    let r = run(vec![
        manifest("c", &[], 0),
        manifest("b", &[], 5),
        manifest("a", &[], 5),
        manifest("z", &[], -1),
    ]);
    assert_eq!(order(&r), ["z", "c", "a", "b"]);
    assert!(r.rejected.is_empty());
}

#[test]
fn dependencies_come_before_priority() {
    let r = run(vec![
        manifest("late", &[("base", "*")], -100),
        manifest("base", &[], 100),
        manifest("other", &[], 0),
    ]);
    // `other` is ready first, then `base` (priority 100) unlocks `late`.
    assert_eq!(order(&r), ["other", "base", "late"]);
}

#[test]
fn disabled_and_incompatible_api() {
    let off = Candidate {
        manifest: manifest("off", &[], 0),
        enabled: false,
    };
    let mut old = manifest("old", &[], 0);
    old.api_version = ApiVersion { major: 0, minor: 2 };
    let r = resolve([off, on(old), on(manifest("ok", &[], 0))], HOST);
    assert_eq!(order(&r), ["ok"]);
    assert_eq!(reason(&r, "off"), &RejectReason::Disabled);
    assert_eq!(
        reason(&r, "old"),
        &RejectReason::IncompatibleApi {
            required: ApiVersion { major: 0, minor: 2 },
            host: HOST
        }
    );
}

#[test]
fn duplicate_ids_reject_every_copy_even_disabled_ones() {
    let off = Candidate {
        manifest: manifest("dup", &[], 0),
        enabled: false,
    };
    let r = resolve(
        [
            on(manifest("dup", &[], 0)),
            off,
            on(manifest("user", &[("dup", "*")], 0)),
        ],
        HOST,
    );
    assert!(r.order.is_empty());
    assert_eq!(reason(&r, "dup"), &RejectReason::DuplicateId);
    assert_eq!(
        reason(&r, "user"),
        &RejectReason::DependencyRejected {
            dependency: id("dup")
        }
    );
    assert_eq!(r.rejected.len(), 2);
}

#[test]
fn missing_dependency() {
    let r = run(vec![manifest("a", &[("ghost", "*")], 0)]);
    assert_eq!(
        reason(&r, "a"),
        &RejectReason::MissingDependency {
            dependency: id("ghost")
        }
    );
}

#[test]
fn dependency_version_mismatch() {
    let r = run(vec![
        manifest("a", &[("b", ">=2.0.0")], 0),
        manifest("b", &[], 0),
    ]);
    match reason(&r, "a") {
        RejectReason::DependencyVersion {
            dependency,
            required,
            found,
        } => {
            assert_eq!(dependency.as_str(), "b");
            assert_eq!(required.to_string(), ">=2.0.0");
            assert_eq!(found.to_string(), "1.0.0");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(order(&r), ["b"]);
}

#[test]
fn rejections_cascade_along_chains() {
    let r = run(vec![
        manifest("a", &[("b", "*")], 0),
        manifest("b", &[("c", "*")], 0),
        manifest("c", &[("ghost", "*")], 0),
        manifest("free", &[], 0),
    ]);
    assert_eq!(order(&r), ["free"]);
    assert_eq!(
        reason(&r, "c"),
        &RejectReason::MissingDependency {
            dependency: id("ghost")
        }
    );
    assert_eq!(
        reason(&r, "b"),
        &RejectReason::DependencyRejected {
            dependency: id("c")
        }
    );
    assert_eq!(
        reason(&r, "a"),
        &RejectReason::DependencyRejected {
            dependency: id("b")
        }
    );
}

#[test]
fn cycles_name_all_members_and_dependents_cascade() {
    let r = run(vec![
        manifest("a", &[("b", "*")], 0),
        manifest("b", &[("c", "*")], 0),
        manifest("c", &[("a", "*")], 0),
        manifest("tail", &[("a", "*")], 0),
        manifest("fine", &[], 0),
    ]);
    assert_eq!(order(&r), ["fine"]);
    let members = vec![id("a"), id("b"), id("c")];
    for m in ["a", "b", "c"] {
        assert_eq!(
            reason(&r, m),
            &RejectReason::DependencyCycle {
                members: members.clone()
            }
        );
    }
    assert_eq!(
        reason(&r, "tail"),
        &RejectReason::DependencyRejected {
            dependency: id("a")
        }
    );
    assert!(reason(&r, "a").to_string().contains("a, b, c"));
}

#[test]
fn separate_cycles_are_reported_separately() {
    let r = run(vec![
        manifest("a", &[("b", "*")], 0),
        manifest("b", &[("a", "*")], 0),
        manifest("x", &[("x2", "*")], 0),
        manifest("x2", &[("x", "*"), ("a", "*")], 0),
    ]);
    assert!(r.order.is_empty());
    assert_eq!(
        reason(&r, "a"),
        &RejectReason::DependencyCycle {
            members: vec![id("a"), id("b")]
        }
    );
    assert_eq!(
        reason(&r, "x"),
        &RejectReason::DependencyCycle {
            members: vec![id("x"), id("x2")]
        }
    );
}

#[test]
fn service_conflict_rejects_the_later_mod_and_its_dependents() {
    let r = run(vec![
        with_services(manifest("first", &[], 0), &["render.sky"], &[]),
        with_services(manifest("second", &[], 1), &["render.sky"], &[]),
        manifest("child", &[("second", "*")], 0),
    ]);
    assert_eq!(order(&r), ["first"]);
    let service: ServiceId = "render.sky".parse().unwrap();
    assert_eq!(
        reason(&r, "second"),
        &RejectReason::ServiceConflict {
            service: service.clone(),
            with: id("first")
        }
    );
    assert_eq!(
        reason(&r, "child"),
        &RejectReason::DependencyRejected {
            dependency: id("second")
        }
    );
    assert_eq!(r.replacer(&service), Some(&id("first")));
}

#[test]
fn conflict_loser_no_longer_blocks_a_third_replacer() {
    // a and b conflict, b is rejected; c conflicts with a only.
    let r = run(vec![
        with_services(manifest("a", &[], 0), &["s"], &[]),
        with_services(manifest("b", &[], 1), &["s"], &[]),
        with_services(manifest("c", &[], 2), &["s"], &[]),
    ]);
    assert_eq!(order(&r), ["a"]);
    assert_eq!(r.rejected.len(), 2);
}

#[test]
fn wrappers_follow_load_order() {
    let r = run(vec![
        with_services(manifest("w2", &[], 2), &[], &["render.hud"]),
        with_services(manifest("w1", &[], 1), &[], &["render.hud"]),
        manifest("plain", &[], 0),
    ]);
    let service: ServiceId = "render.hud".parse().unwrap();
    let wrappers: Vec<&str> = r.wrappers(&service).map(ModId::as_str).collect();
    assert_eq!(wrappers, ["w1", "w2"]);
    assert_eq!(r.replacer(&service), None);
}

#[test]
fn rejections_are_sorted_by_id() {
    let off = |id: &str| Candidate {
        manifest: manifest(id, &[], 0),
        enabled: false,
    };
    let r = resolve([off("m"), off("a"), off("z")], HOST);
    let ids: Vec<&str> = r.rejected.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(ids, ["a", "m", "z"]);
}

#[test]
fn reason_messages_are_readable() {
    let text = reason(&run(vec![manifest("a", &[("b", "*")], 0)]), "a").to_string();
    assert_eq!(text, "requires `b`, which is not installed");
}

/// A random DAG: mod `i` depends on a subset of lower indices.
fn dag() -> impl Strategy<Value = Vec<Manifest>> {
    (1usize..12).prop_flat_map(|n| {
        let mods: Vec<_> = (0..n)
            .map(|i| (proptest::collection::vec(any::<bool>(), i), -3i32..3))
            .collect();
        mods.prop_map(|specs| {
            specs
                .into_iter()
                .enumerate()
                .map(|(i, (links, priority))| {
                    let deps: Vec<String> = (0..i)
                        .filter(|j| links[*j])
                        .map(|j| format!("m{j}"))
                        .collect();
                    let deps: Vec<(&str, &str)> = deps.iter().map(|d| (d.as_str(), "*")).collect();
                    manifest(&format!("m{i}"), &deps, priority)
                })
                .collect()
        })
    })
}

proptest! {
    #[test]
    fn dag_loads_fully_in_dependency_order(mods in dag(), seed in any::<u64>()) {
        let r = run(mods.clone());
        prop_assert!(r.rejected.is_empty());
        prop_assert_eq!(r.order.len(), mods.len());
        let position = |id: &ModId| r.order.iter().position(|m| &m.id == id).unwrap();
        for m in &r.order {
            for dep in m.dependencies.keys() {
                prop_assert!(position(dep) < position(&m.id));
            }
        }
        // Determinism: any permutation of the input gives the same resolution.
        let mut shuffled = mods;
        let len = shuffled.len();
        let mut state = seed;
        for i in (1..len).rev() {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            shuffled.swap(i, (state >> 33) as usize % (i + 1));
        }
        let other = run(shuffled);
        prop_assert_eq!(order(&r), order(&other));
    }
}
