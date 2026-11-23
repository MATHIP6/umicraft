// SPDX-License-Identifier: MIT OR Apache-2.0

//! The profiler's and the mod manager's view of one mod: state and counters.

use crate::budget::ModStats;
use crate::context::ModContext;
use crate::manifest::{Manifest, ModId};

/// Where a mod stands, most specific cause first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModState {
    /// Running.
    Enabled,
    /// The player switched it off.
    SwitchedOff,
    /// Server-compat mode holds it off because it may alter packets or movement.
    HeldByCompat,
    /// It misbehaved and stays off until restart; carries the first reason, naming
    /// the hook ("panicked in its handler for event 3").
    Failed(String),
}

/// Cumulative state and counters of one loaded mod.
///
/// The counters only grow; the client turns two snapshots into rates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModProfile {
    /// The mod's id.
    pub id: ModId,
    /// Display name from `mod.toml`.
    pub name: String,
    /// The mod's version as written in `mod.toml`.
    pub version: String,
    /// Whether it runs, and if not why.
    pub state: ModState,
    /// Event handlers (intercept and observe), plus dropped and injected packets.
    pub events: ModStats,
    /// HUD widget draw functions.
    pub widgets: ModStats,
    /// Chat commands.
    pub commands: ModStats,
    /// The `player_physics` layer.
    pub physics: ModStats,
    /// The `render_camera` layer (API 1.1).
    pub camera: ModStats,
}

impl ModProfile {
    pub(crate) fn of(manifest: &Manifest, context: &ModContext) -> Self {
        Self {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.to_string(),
            state: state_of(context),
            events: context.stats(),
            widgets: context.hud_stats(),
            commands: context.command_counters().snapshot(),
            physics: context.physics_stats(),
            camera: context.camera_stats(),
        }
    }

    /// All five categories added up: counters summed, `max_ns` the largest.
    pub fn total(&self) -> ModStats {
        [
            self.events,
            self.widgets,
            self.commands,
            self.physics,
            self.camera,
        ]
        .into_iter()
        .fold(ModStats::default(), |sum, part| ModStats {
            calls: sum.calls.saturating_add(part.calls),
            total_ns: sum.total_ns.saturating_add(part.total_ns),
            max_ns: sum.max_ns.max(part.max_ns),
            overruns: sum.overruns.saturating_add(part.overruns),
            dropped: sum.dropped.saturating_add(part.dropped),
            injected: sum.injected.saturating_add(part.injected),
        })
    }
}

fn state_of(context: &ModContext) -> ModState {
    if context.is_failed() {
        // A mod retired at exit has no reason; it is not a misbehaviour.
        ModState::Failed(context.failure().unwrap_or_else(|| "shut down".to_owned()))
    } else if !context.is_wanted() {
        ModState::SwitchedOff
    } else if context.is_held_by_compat() {
        ModState::HeldByCompat
    } else {
        ModState::Enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(events: ModStats, widgets: ModStats) -> ModProfile {
        ModProfile {
            id: "p".to_owned().try_into().unwrap(),
            name: "P".to_owned(),
            version: "1.0.0".to_owned(),
            state: ModState::Enabled,
            events,
            widgets,
            commands: ModStats::default(),
            physics: ModStats::default(),
            camera: ModStats::default(),
        }
    }

    fn stats(calls: u64, max_ns: u64) -> ModStats {
        ModStats {
            calls,
            total_ns: calls * 10,
            max_ns,
            overruns: 1,
            dropped: 2,
            injected: 3,
        }
    }

    #[test]
    fn total_sums_counters_and_takes_the_largest_max() {
        let total = profile(stats(4, 90), stats(6, 50)).total();
        assert_eq!(
            total,
            ModStats {
                calls: 10,
                total_ns: 100,
                max_ns: 90,
                overruns: 2,
                dropped: 4,
                injected: 6,
            }
        );
    }

    #[test]
    fn state_follows_the_most_specific_cause() {
        let cx = ModContext::new("s".to_owned().try_into().unwrap());
        assert_eq!(state_of(&cx), ModState::SwitchedOff);
        cx.finish_init(true);
        assert_eq!(state_of(&cx), ModState::Enabled);
        cx.set_wanted(false);
        assert_eq!(state_of(&cx), ModState::SwitchedOff);
        cx.fail("first");
        cx.fail("second");
        assert_eq!(state_of(&cx), ModState::Failed("first".to_owned()));
    }
}
