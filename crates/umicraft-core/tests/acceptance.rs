// SPDX-License-Identifier: MIT OR Apache-2.0

//! M0 acceptance (spec §12): a dummy module registers a service and an event
//! handler; a second module wraps the service; the core drives a tick.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use umicraft_core::error::{CoreError, CoreResult};
use umicraft_core::{Core, EventCx, Module, ModuleCtx, ModuleId};

/// A test-only event payload dispatched by the shell.
struct Ping(u64);

struct EchoModule {
    id: ModuleId,
    counter: Arc<AtomicU64>,
}

impl EchoModule {
    fn new(id: &str, counter: Arc<AtomicU64>) -> Self {
        Self {
            id: ModuleId::new(id),
            counter,
        }
    }
}

impl Module for EchoModule {
    fn id(&self) -> &ModuleId {
        &self.id
    }

    fn name(&self) -> &str {
        "Echo"
    }

    fn on_load(&mut self, ctx: &mut ModuleCtx<'_>) -> CoreResult<()> {
        let module_id = ctx.module_id().clone();
        ctx.services()
            .provide("echo.count", module_id.clone(), 1, self.counter.clone())?;
        ctx.services().provide(
            "echo.value",
            module_id.clone(),
            1,
            Arc::new(String::from("hi")),
        )?;

        let counter = self.counter.clone();
        ctx.events()
            .on(module_id, 0, move |ping: &mut Ping, _cx: &mut EventCx| {
                counter.fetch_add(ping.0, Ordering::SeqCst);
            });
        Ok(())
    }
}

struct WrapModule {
    id: ModuleId,
}

impl Module for WrapModule {
    fn id(&self) -> &ModuleId {
        &self.id
    }

    fn on_load(&mut self, ctx: &mut ModuleCtx<'_>) -> CoreResult<()> {
        let module_id = ctx.module_id().clone();
        ctx.services()
            .wrap("echo.value", module_id, |old| Arc::new(format!("{old}!")))?;
        Ok(())
    }
}

struct FailingModule {
    id: ModuleId,
}

impl Module for FailingModule {
    fn id(&self) -> &ModuleId {
        &self.id
    }

    fn on_load(&mut self, ctx: &mut ModuleCtx<'_>) -> CoreResult<()> {
        // Definitively wrong type for an existing name: proves the conflict path.
        let module_id = ctx.module_id().clone();
        ctx.services()
            .provide("echo.value", module_id, 1, Arc::new(0u64))
    }
}

#[test]
fn m0_acceptance_service_and_event_through_modules() {
    let counter = Arc::new(AtomicU64::new(0));
    let mut core = Core::new();
    core.add_module(Box::new(EchoModule::new("echo", Arc::clone(&counter))))
        .unwrap();
    core.add_module(Box::new(WrapModule {
        id: ModuleId::new("wrap"),
    }))
    .unwrap();
    core.start().unwrap();

    // Wrap chain composed onto the provided value.
    let value: Arc<String> = core.services().get("echo.value").unwrap();
    assert_eq!(&*value, "hi!");

    // Driving one tick runs the scheduler; the shell dispatches user events.
    core.run_tick().unwrap();
    core.events().dispatch(&mut Ping(7));
    assert_eq!(counter.load(Ordering::SeqCst), 7);

    // The counter service and the event handler share the same state.
    let stored: Arc<AtomicU64> = core.services().get("echo.count").unwrap();
    assert_eq!(stored.load(Ordering::SeqCst), 7);
}

#[test]
fn failing_module_load_reports_typed_error() {
    let counter = Arc::new(AtomicU64::new(0));
    let mut core = Core::new();
    core.add_module(Box::new(EchoModule::new("echo", counter)))
        .unwrap();
    core.add_module(Box::new(FailingModule {
        id: ModuleId::new("broken"),
    }))
    .unwrap();

    // FailingModule re-provides an existing name, so on_load maps to a
    // conflict; start surfaces it wrapped as LoadFailed with the module id.
    let Err(CoreError::LoadFailed { module, .. }) = core.start() else {
        panic!("expected a load failure, got {:?}", core.scheduler());
    };
    assert_eq!(module, "broken");
}

#[test]
fn second_start_is_rejected() {
    let counter = Arc::new(AtomicU64::new(0));
    let mut core = Core::new();
    core.add_module(Box::new(EchoModule::new("echo", counter)))
        .unwrap();
    core.start().unwrap();
    assert!(matches!(core.start(), Err(CoreError::AlreadyStarted)));
}

#[test]
fn core_with_config_file_starts() {
    let store = "[echo]\nunused = 1\n"
        .parse::<umicraft_core::ConfigStore>()
        .unwrap();
    let counter = Arc::new(AtomicU64::new(0));
    let mut core = Core::with_config(store);
    core.add_module(Box::new(EchoModule::new("echo", counter)))
        .unwrap();
    core.start().unwrap();
}
