// SPDX-License-Identifier: MIT OR Apache-2.0

//! Module model and the core runtime assembly (spec §5.1, §5.4).
//!
//! Everything Umicraft ships — chunk rendering, physics, chat, inventory UI —
//! is a [`Module`] registered through the same hooks future mods use, so mods
//! (M15) can wrap or replace built-in behavior instead of fighting it.
//!
//! [`Core`] owns the four stores (services, events, scheduler, config) and runs
//! each module's `on_load` once, in registration order, with a [`ModuleCtx`]
//! that already knows which module is loading.

use std::fmt;

use crate::config::ConfigStore;
use crate::error::CoreError;
use crate::events::EventBus;
use crate::scheduler::Scheduler;
use crate::service::ServiceRegistry;

/// Stable module identifier; unique within a running core.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleId(String);

impl ModuleId {
    /// Creates an id from any string-like value.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrowed view of the id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A unit of functionality loaded by the core (built-ins and mods alike).
///
/// Static modules implement this trait directly; dynamic mods get the same
/// surface through the M15 C ABI shim, which is why hooks stay minimal.
pub trait Module: Send {
    /// Stable, unique id — used in diagnostics, conflict errors, and profiling.
    fn id(&self) -> &ModuleId;

    /// Human-readable display name; defaults to the id.
    fn name(&self) -> &str {
        self.id().as_str()
    }

    /// Registers services, event handlers, tick systems, and applies config.
    ///
    /// Called exactly once, during [`Core::start`], in registration order.
    /// Keep work here cheap: the client main loop does not run while loading.
    ///
    /// # Errors
    /// Anything the module can express as [`CoreError`]; failing aborts startup.
    fn on_load(&mut self, ctx: &mut ModuleCtx<'_>) -> Result<(), CoreError>;
}

/// Handles handed to [`Module::on_load`]; `module_id` is always filled in by
/// the core, so hooks do not need to thread their own identity around.
pub struct ModuleCtx<'a> {
    module_id: ModuleId,
    services: &'a mut ServiceRegistry,
    events: &'a mut EventBus,
    scheduler: &'a mut Scheduler,
    config: &'a mut ConfigStore,
}

impl<'a> ModuleCtx<'a> {
    /// Internal: assembled by the core, one per module load.
    pub(crate) fn new(
        module_id: ModuleId,
        services: &'a mut ServiceRegistry,
        events: &'a mut EventBus,
        scheduler: &'a mut Scheduler,
        config: &'a mut ConfigStore,
    ) -> Self {
        Self {
            module_id,
            services,
            events,
            scheduler,
            config,
        }
    }

    /// Id of the module currently being loaded.
    #[must_use]
    pub fn module_id(&self) -> &ModuleId {
        &self.module_id
    }

    /// Service registry for provide/get/replace/wrap.
    pub fn services(&mut self) -> &mut ServiceRegistry {
        self.services
    }

    /// Event bus for subscriptions.
    pub fn events(&mut self) -> &mut EventBus {
        self.events
    }

    /// Scheduler for tick systems.
    pub fn scheduler(&mut self) -> &mut Scheduler {
        self.scheduler
    }

    /// Configuration store; the module's own section is keyed by its id.
    pub fn config(&mut self) -> &mut ConfigStore {
        self.config
    }
}

/// Ordered list of registered modules.
#[derive(Default)]
pub struct ModuleRegistry {
    modules: Vec<Box<dyn Module>>,
}

impl std::fmt::Debug for ModuleRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleRegistry")
            .field("modules", &self.modules.len())
            .finish()
    }
}

impl ModuleRegistry {
    /// Appends a module after checking its id is not already taken.
    ///
    /// # Errors
    /// [`CoreError::DuplicateModule`] when the id is already registered.
    pub fn register(&mut self, module: Box<dyn Module>) -> Result<(), CoreError> {
        if self
            .modules
            .iter()
            .any(|existing| existing.id() == module.id())
        {
            return Err(CoreError::DuplicateModule(module.id().to_string()));
        }
        self.modules.push(module);
        Ok(())
    }

    /// Number of registered modules.
    #[must_use]
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Whether no modules are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    fn modules_mut(&mut self) -> &mut [Box<dyn Module>] {
        &mut self.modules
    }
}

/// The Umicraft core runtime: modules plus their four stores (spec §5.1).
///
/// Lifecycle: build with [`Core::new`]/[`Core::with_config`], add modules via
/// [`Core::add_module`], then call [`Core::start`] exactly once; afterwards the
/// client drives [`Core::run_tick`] in a loop.
#[derive(Debug, Default)]
pub struct Core {
    modules: ModuleRegistry,
    services: ServiceRegistry,
    events: EventBus,
    scheduler: Scheduler,
    config: ConfigStore,
    started: bool,
}

impl Core {
    /// Creates an empty core with an empty configuration (tests, headless tools).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a core around a pre-loaded configuration.
    #[must_use]
    pub fn with_config(config: ConfigStore) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    /// Registers a module for the next [`Core::start`].
    ///
    /// # Errors
    /// [`CoreError::AlreadyStarted`] when the core already started,
    /// [`CoreError::DuplicateModule`] when the id is taken.
    pub fn add_module(&mut self, module: Box<dyn Module>) -> Result<(), CoreError> {
        if self.started {
            return Err(CoreError::AlreadyStarted);
        }
        self.modules.register(module)
    }

    /// Runs every module's `on_load` in registration order.
    ///
    /// Marks the core as started beforehand: on [`CoreError::LoadFailed`] the
    /// core may hold partially initialized stores, so the caller should discard
    /// this [`Core`] rather than retry `start` on it.
    ///
    /// # Errors
    /// [`CoreError::AlreadyStarted`] on a second call, [`CoreError::LoadFailed`]
    /// when any module hook fails.
    pub fn start(&mut self) -> Result<(), CoreError> {
        if self.started {
            return Err(CoreError::AlreadyStarted);
        }
        self.started = true;

        let mut modules = std::mem::take(&mut self.modules);
        let mut failure = None;
        for module in modules.modules_mut() {
            let module_id = module.id().clone();
            let mut ctx = ModuleCtx::new(
                module_id.clone(),
                &mut self.services,
                &mut self.events,
                &mut self.scheduler,
                &mut self.config,
            );
            if let Err(source) = module.on_load(&mut ctx) {
                failure = Some((module_id, source));
                break;
            }
        }
        self.modules = modules;

        match failure {
            Some((module, source)) => Err(CoreError::LoadFailed {
                module: module.to_string(),
                source: Box::new(source),
            }),
            None => Ok(()),
        }
    }

    /// Service registry access, mainly for tests and the client shell.
    pub fn services(&mut self) -> &mut ServiceRegistry {
        &mut self.services
    }

    /// Event bus access, mainly for tests and the client shell.
    pub fn events(&mut self) -> &mut EventBus {
        &mut self.events
    }

    /// Scheduler access, mainly for tests and the client shell.
    pub fn scheduler(&mut self) -> &mut Scheduler {
        &mut self.scheduler
    }

    /// Configuration access, mainly for tests and the client shell.
    pub fn config(&mut self) -> &mut ConfigStore {
        &mut self.config
    }

    /// Advances one game tick through the scheduler and event bus.
    ///
    /// # Errors
    /// Propagates [`CoreError`] from a failing tick system.
    pub fn run_tick(&mut self) -> Result<(), CoreError> {
        self.scheduler.run_tick(&mut self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Noop {
        id: ModuleId,
    }

    impl Noop {
        fn new(id: &str) -> Self {
            Self {
                id: ModuleId::new(id),
            }
        }
    }

    impl Module for Noop {
        fn id(&self) -> &ModuleId {
            &self.id
        }

        fn on_load(&mut self, _ctx: &mut ModuleCtx<'_>) -> Result<(), CoreError> {
            Ok(())
        }
    }

    #[test]
    fn duplicate_module_ids_are_rejected() {
        let mut core = Core::new();
        core.add_module(Box::new(Noop::new("dup"))).unwrap();
        let second = core.add_module(Box::new(Noop::new("dup")));
        assert!(matches!(&second, Err(CoreError::DuplicateModule(id)) if id == "dup"));
    }

    #[test]
    fn add_module_after_start_is_blocked() {
        let mut core = Core::new();
        core.add_module(Box::new(Noop::new("first"))).unwrap();
        core.start().unwrap();
        let late = core.add_module(Box::new(Noop::new("second")));
        assert!(matches!(late, Err(CoreError::AlreadyStarted)));
    }
}
