// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cost of dispatching an event (F3.3, ADR-0086): with no subscriber, with one Rust
//! handler, and with one foreign intercept handler through the mod adapter.

use core::ffi::c_void;
use std::hint::black_box;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use umicraft_core::module::ModuleId;
use umicraft_core::EventBus;
use umicraft_modapi_sys::{UmiEventHeader, UmiEventId, UmiMode, UmiStatus};
use umicraft_modhost::{install_mod, ChatSend, EventThread, ModContext, PreTick, UserPtr};

extern "C" fn noop(_: *mut UmiEventHeader, _: *mut c_void) -> UmiStatus {
    UmiStatus::OK
}

fn foreign_bus() -> EventBus {
    let context = Arc::new(ModContext::new("bench".to_owned().try_into().unwrap()));
    let user = UserPtr::new(std::ptr::null_mut());
    context.subscribe(
        UmiEventId::PRE_TICK,
        UmiMode::INTERCEPT,
        0,
        Some(noop),
        user,
    );
    context.finish_init(true);
    let mut bus = EventBus::default();
    install_mod(&mut bus, EventThread::Game, &context);
    bus
}

fn dispatch(c: &mut Criterion) {
    let mut group = c.benchmark_group("modhost_dispatch");

    let mut empty = EventBus::default();
    group.bench_function("pre_tick_no_subscriber", |b| {
        b.iter(|| empty.dispatch(black_box(&mut PreTick::new(1))));
    });

    let mut rust = EventBus::default();
    rust.on::<PreTick, _>(ModuleId::new("rust"), 0, |event, _| {
        black_box(event.tick());
    });
    group.bench_function("pre_tick_one_rust_handler", |b| {
        b.iter(|| rust.dispatch(black_box(&mut PreTick::new(1))));
    });

    let mut foreign = foreign_bus();
    group.bench_function("pre_tick_one_foreign_intercept", |b| {
        b.iter(|| foreign.dispatch(black_box(&mut PreTick::new(1))));
    });

    group.bench_function("chat_send_no_subscriber", |b| {
        b.iter(|| {
            let mut event = ChatSend::new(black_box("/home"));
            empty.dispatch(&mut event)
        });
    });
    group.finish();
}

criterion_group!(benches, dispatch);
criterion_main!(benches);
