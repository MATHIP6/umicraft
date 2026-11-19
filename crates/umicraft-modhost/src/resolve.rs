// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deciding which mods load and in which order.
//!
//! Every way a mod can fail to load (missing or rejected dependency, cycle, service
//! conflict) can invalidate other mods, so all of them feed one loop in [`resolve`]:
//! cascade the rejections, order the survivors, look for a cycle or a conflict, reject
//! it and start over. The loop ends because every pass rejects at least one mod. The
//! result depends only on the set of candidates, never on their input order.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap};

use semver::{Version, VersionReq};
use thiserror::Error;

use crate::manifest::{ApiVersion, Manifest, ModId, ServiceId};

/// A mod found on disk and whether the user switched it on.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// The mod's manifest.
    pub manifest: Manifest,
    /// Whether the user enabled it.
    pub enabled: bool,
}

/// Why a mod will not load. The `Display` text is addressed to the player.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RejectReason {
    /// The user disabled it.
    #[error("disabled by the user")]
    Disabled,
    /// Several installed mods share the id, so none of them is used.
    #[error("several installed mods share this id")]
    DuplicateId,
    /// Built for an API version the game does not offer.
    #[error("built for API {required}, but this game provides API {host}")]
    IncompatibleApi {
        /// The version the mod asks for.
        required: ApiVersion,
        /// The version the host provides.
        host: ApiVersion,
    },
    /// A required mod is not installed.
    #[error("requires `{dependency}`, which is not installed")]
    MissingDependency {
        /// The absent mod.
        dependency: ModId,
    },
    /// A required mod is installed in an unsuitable version.
    #[error("requires `{dependency}` {required}, but version {found} is installed")]
    DependencyVersion {
        /// The mod whose version does not fit.
        dependency: ModId,
        /// The accepted versions.
        required: VersionReq,
        /// The installed version.
        found: Version,
    },
    /// A required mod itself cannot load.
    #[error("requires `{dependency}`, which cannot be loaded")]
    DependencyRejected {
        /// The mod that cannot load.
        dependency: ModId,
    },
    /// The mod is part of a circular dependency.
    #[error("takes part in a dependency cycle ({})", join(members))]
    DependencyCycle {
        /// Every mod of the cycle, sorted by id.
        members: Vec<ModId>,
    },
    /// An earlier mod already replaces a service this one replaces.
    #[error("replaces service `{service}`, which `{with}` already replaces")]
    ServiceConflict {
        /// The contested service.
        service: ServiceId,
        /// The mod that replaces it first in load order.
        with: ModId,
    },
}

fn join(ids: &[ModId]) -> String {
    ids.iter().map(ModId::as_str).collect::<Vec<_>>().join(", ")
}

/// A mod that will not load, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    /// The rejected mod.
    pub id: ModId,
    /// The reason.
    pub reason: RejectReason,
}

/// The outcome of [`resolve`].
#[derive(Debug, Clone)]
pub struct Resolution {
    /// The mods to load: dependencies first, then ascending priority, then id.
    pub order: Vec<Manifest>,
    /// One entry per mod that will not load, sorted by id.
    pub rejected: Vec<Rejection>,
}

impl Resolution {
    /// The mods wrapping `service`, in load order (the first is the innermost wrapper).
    pub fn wrappers<'a>(&'a self, service: &'a ServiceId) -> impl Iterator<Item = &'a ModId> {
        self.order
            .iter()
            .filter(|m| m.load.wraps.contains(service))
            .map(|m| &m.id)
    }

    /// The mod replacing `service`, if any. Resolution guarantees at most one.
    pub fn replacer(&self, service: &ServiceId) -> Option<&ModId> {
        self.order
            .iter()
            .find(|m| m.load.replaces.contains(service))
            .map(|m| &m.id)
    }
}

/// Chooses which candidates load and in which order.
///
/// `host_api` is the API version the running game provides. A mod that cannot load is
/// reported in [`Resolution::rejected`] rather than failing the whole resolution, so
/// one broken mod never keeps the others from starting.
pub fn resolve(
    candidates: impl IntoIterator<Item = Candidate>,
    host_api: ApiVersion,
) -> Resolution {
    let mut pool = Pool::new(candidates, host_api);
    let order = loop {
        pool.cascade();
        let (order, stuck) = pool.schedule();
        if pool.reject_cycles(&stuck) {
            continue;
        }
        match pool.first_conflict(&order) {
            Some((id, reason)) => pool.reject(&id, reason),
            None => break order,
        }
    };
    let order = order.iter().filter_map(|id| pool.live.remove(id)).collect();
    let rejected = pool
        .rejected
        .into_iter()
        .map(|(id, reason)| Rejection { id, reason })
        .collect();
    Resolution { order, rejected }
}

/// The mods still in the running and those already rejected.
struct Pool {
    live: BTreeMap<ModId, Manifest>,
    rejected: BTreeMap<ModId, RejectReason>,
}

impl Pool {
    /// Applies the checks that look at a mod alone: duplicate, disabled, API.
    fn new(candidates: impl IntoIterator<Item = Candidate>, host_api: ApiVersion) -> Self {
        let mut by_id: BTreeMap<ModId, Vec<Candidate>> = BTreeMap::new();
        for candidate in candidates {
            by_id
                .entry(candidate.manifest.id.clone())
                .or_default()
                .push(candidate);
        }
        let mut pool = Self {
            live: BTreeMap::new(),
            rejected: BTreeMap::new(),
        };
        for (id, mut copies) in by_id {
            let single = if copies.len() == 1 {
                copies.pop()
            } else {
                None
            };
            match single {
                None => pool.reject_new(id, RejectReason::DuplicateId),
                Some(c) if !c.enabled => pool.reject_new(id, RejectReason::Disabled),
                Some(c) if !host_api.accepts(c.manifest.api_version) => {
                    let reason = RejectReason::IncompatibleApi {
                        required: c.manifest.api_version,
                        host: host_api,
                    };
                    pool.reject_new(id, reason);
                }
                Some(c) => {
                    pool.live.insert(id, c.manifest);
                }
            }
        }
        pool
    }

    fn reject_new(&mut self, id: ModId, reason: RejectReason) {
        self.rejected.insert(id, reason);
    }

    fn reject(&mut self, id: &ModId, reason: RejectReason) {
        self.live.remove(id);
        self.rejected.insert(id.clone(), reason);
    }

    /// The first unmet dependency of `manifest`, in dependency id order.
    fn dependency_failure(&self, manifest: &Manifest) -> Option<RejectReason> {
        manifest
            .dependencies
            .iter()
            .find_map(|(dependency, required)| match self.live.get(dependency) {
                Some(found) if required.matches(&found.version) => None,
                Some(found) => Some(RejectReason::DependencyVersion {
                    dependency: dependency.clone(),
                    required: required.clone(),
                    found: found.version.clone(),
                }),
                None if self.rejected.contains_key(dependency) => {
                    Some(RejectReason::DependencyRejected {
                        dependency: dependency.clone(),
                    })
                }
                None => Some(RejectReason::MissingDependency {
                    dependency: dependency.clone(),
                }),
            })
    }

    /// Rejects mods with an unmet dependency until none is left; one rejection can
    /// break the mods that depend on it.
    fn cascade(&mut self) {
        loop {
            let failing: Vec<(ModId, RejectReason)> = self
                .live
                .values()
                .filter_map(|m| Some((m.id.clone(), self.dependency_failure(m)?)))
                .collect();
            if failing.is_empty() {
                return;
            }
            for (id, reason) in failing {
                self.reject(&id, reason);
            }
        }
    }

    /// Kahn's algorithm on the live mods, all of whose dependencies are live.
    ///
    /// Among the mods that are ready, the lowest `(priority, id)` goes first. Returns
    /// the order and the mods that could not be scheduled because of a cycle.
    fn schedule(&self) -> (Vec<ModId>, BTreeSet<ModId>) {
        let mut waiting: HashMap<&ModId, usize> = HashMap::new();
        let mut dependents: HashMap<&ModId, Vec<&ModId>> = HashMap::new();
        let mut ready = BinaryHeap::new();
        for manifest in self.live.values() {
            waiting.insert(&manifest.id, manifest.dependencies.len());
            for dependency in manifest.dependencies.keys() {
                dependents.entry(dependency).or_default().push(&manifest.id);
            }
            if manifest.dependencies.is_empty() {
                ready.push(Reverse((manifest.load.priority, &manifest.id)));
            }
        }
        let mut order = Vec::with_capacity(self.live.len());
        while let Some(Reverse((_, id))) = ready.pop() {
            order.push(id.clone());
            for &dependent in dependents.get(id).into_iter().flatten() {
                if let Some(count) = waiting.get_mut(dependent) {
                    *count -= 1;
                    if *count == 0 {
                        ready.push(Reverse((self.live[dependent].load.priority, dependent)));
                    }
                }
            }
        }
        let scheduled: BTreeSet<&ModId> = order.iter().collect();
        let stuck = self
            .live
            .keys()
            .filter(|id| !scheduled.contains(id))
            .cloned()
            .collect();
        (order, stuck)
    }

    /// Rejects the mods of `stuck` that lie on a cycle, naming the whole cycle.
    ///
    /// The other stuck mods only depend on a cycle and are left for [`Self::cascade`].
    /// Returns whether anything was rejected.
    fn reject_cycles(&mut self, stuck: &BTreeSet<ModId>) -> bool {
        let reach: BTreeMap<&ModId, BTreeSet<&ModId>> = stuck
            .iter()
            .map(|id| (id, self.reachable(id, stuck)))
            .collect();
        let cyclic: Vec<(ModId, Vec<ModId>)> = stuck
            .iter()
            .filter(|id| reach[id].contains(id))
            .map(|id| {
                let members = reach[id]
                    .iter()
                    .filter(|other| reach[*other].contains(id))
                    .map(|other| (*other).clone())
                    .collect();
                (id.clone(), members)
            })
            .collect();
        for (id, members) in &cyclic {
            self.reject(
                id,
                RejectReason::DependencyCycle {
                    members: members.clone(),
                },
            );
        }
        !cyclic.is_empty()
    }

    /// The mods of `within` that `start` depends on, directly or not.
    fn reachable<'a>(&'a self, start: &ModId, within: &'a BTreeSet<ModId>) -> BTreeSet<&'a ModId> {
        let mut seen = BTreeSet::new();
        let mut todo = vec![start];
        while let Some(id) = todo.pop() {
            for dependency in self.live[id].dependencies.keys() {
                if let Some(dependency) = within.get(dependency) {
                    if seen.insert(dependency) {
                        todo.push(dependency);
                    }
                }
            }
        }
        seen
    }

    /// The first mod in load order that replaces a service an earlier mod replaces.
    fn first_conflict(&self, order: &[ModId]) -> Option<(ModId, RejectReason)> {
        let mut owners: HashMap<&ServiceId, &ModId> = HashMap::new();
        for id in order {
            for service in &self.live[id].load.replaces {
                if let Some(&with) = owners.get(service) {
                    let reason = RejectReason::ServiceConflict {
                        service: service.clone(),
                        with: with.clone(),
                    };
                    return Some((id.clone(), reason));
                }
                owners.insert(service, id);
            }
        }
        None
    }
}
