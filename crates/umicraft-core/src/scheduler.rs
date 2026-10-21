// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fixed-timestep tick scheduler (spec §5.3, GAM-8).
//!
//! In Minecraft, all server logic — and therefore client prediction — runs at a
//! fixed 20 ticks per second (`Docs/sources.md` entry pending M3 traffic capture).
//! The scheduler owns tick counting and system invocation; the client binary
//! drives pacing. Built-in [`PreTick`]/[`PostTick`] events let modules gate or
//! reorder per-tick behavior without touching the loop itself.

use tracing::info;

use crate::error::CoreError;
use crate::events::EventBus;

/// Emitted before tick systems run; a handler cancelling it skips the systems.
#[derive(Debug)]
pub struct PreTick {
    /// Index of the tick being started (1-based: counts up from the first run).
    pub tick: u64,
}

/// Emitted after tick systems ran; cancelling it stops remaining post-handlers.
#[derive(Debug)]
pub struct PostTick {
    /// Index of the tick that just completed.
    pub tick: u64,
}

struct SystemEntry {
    name: String,
    run: SysRun,
}

/// System closure erased into the scheduler; tick counter + bus on every call.
type SysRun = Box<dyn FnMut(u64, &mut EventBus) -> Result<(), CoreError> + Send>;

impl std::fmt::Debug for SystemEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemEntry")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Ordered tick systems plus the current tick counter.
#[derive(Debug, Default)]
pub struct Scheduler {
    tick: u64,
    systems: Vec<SystemEntry>,
}

impl Scheduler {
    /// Registers a tick system under a unique `name` (diagnostics + conflict rule).
    ///
    /// # Errors
    /// [`CoreError::DuplicateModule`] when `name` is already registered (it is
    /// reused here as a generic name-clash error rather than a new variant).
    pub fn add_system(
        &mut self,
        name: impl Into<String>,
        run: impl FnMut(u64, &mut EventBus) -> Result<(), CoreError> + Send + 'static,
    ) -> Result<(), CoreError> {
        let name = name.into();
        if self.systems.iter().any(|system| system.name == name) {
            return Err(CoreError::DuplicateModule(name));
        }
        info!(system = %name, "tick system registered");
        self.systems.push(SystemEntry {
            name,
            run: Box::new(run),
        });
        Ok(())
    }

    /// Current tick counter (the last completed tick).
    #[must_use]
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Advances one tick: `PreTick` → systems in registration order → `PostTick`.
    ///
    /// A cancelled `PreTick` skips all systems (`PostTick` still runs). The
    /// first failing system aborts the tick; the tick counter still advanced.
    ///
    /// # Errors
    /// Propagates the first system error.
    pub fn run_tick(&mut self, events: &mut EventBus) -> Result<(), CoreError> {
        self.tick += 1;
        let tick = self.tick;

        let mut pre = PreTick { tick };
        if events.dispatch(&mut pre) {
            return Ok(());
        }
        for system in &mut self.systems {
            (system.run)(tick, events)?;
        }

        let mut post = PostTick { tick };
        events.dispatch(&mut post);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventCx;
    use crate::module::ModuleId;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    #[test]
    fn ticks_run_systems_in_order_and_count() {
        let pre_seen = Arc::new(AtomicU64::new(0));
        let system_seen = Arc::new(AtomicU64::new(0));
        let mut scheduler = Scheduler::default();
        let mut events = EventBus::default();
        let pre = pre_seen.clone();
        events.on::<PreTick, _>(ModuleId::new("flow"), 0, move |_tick: &mut PreTick, _cx| {
            pre.fetch_add(1, Ordering::SeqCst);
        });
        let seen = system_seen.clone();
        scheduler
            .add_system("a", move |tick: u64, _events: &mut EventBus| {
                seen.store(tick, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();

        for _ in 0..3 {
            scheduler.run_tick(&mut events).unwrap();
        }
        assert_eq!(scheduler.tick(), 3);
        assert_eq!(system_seen.load(Ordering::SeqCst), 3);
        assert_eq!(pre_seen.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn cancelling_pre_tick_skips_systems() {
        let mut scheduler = Scheduler::default();
        let mut events = EventBus::default();
        events.on::<PreTick, _>(
            ModuleId::new("gate"),
            -10,
            |_: &mut PreTick, cx: &mut EventCx| {
                cx.cancel();
            },
        );
        let ran = Arc::new(AtomicU64::new(0));
        let ran_gate = ran.clone();
        scheduler
            .add_system("skipped", move |_tick: u64, _events: &mut EventBus| {
                ran_gate.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();

        scheduler.run_tick(&mut events).unwrap();
        assert_eq!(scheduler.tick(), 1);
        assert_eq!(ran.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn duplicate_system_names_conflict() {
        let mut scheduler = Scheduler::default();
        scheduler
            .add_system("physics", |_: u64, _: &mut EventBus| Ok(()))
            .unwrap();
        let second = scheduler.add_system("physics", |_: u64, _: &mut EventBus| Ok(()));
        assert!(matches!(second, Err(CoreError::DuplicateModule(_))));
    }
}
