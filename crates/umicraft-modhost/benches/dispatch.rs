// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cost of dispatching an event (F3.3, ADR-0086): with no subscriber, with one Rust
//! handler, and with one foreign intercept handler through the mod adapter; and (F3.4)
//! of drawing one foreign HUD widget that writes a line.

use core::ffi::c_void;
use std::hint::black_box;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use umicraft_builtin_mods::HudPlacement;
use umicraft_core::module::ModuleId;
use umicraft_core::EventBus;
use umicraft_modapi_sys::{UmiEventHeader, UmiEventId, UmiHudCx, UmiMode, UmiStatus, UmiStr};
use umicraft_modhost::{
    host_table, install_mod, ChatSend, EventThread, ModContext, PacketReceived, Permissions,
    PreTick, UserPtr,
};

extern "C" fn noop(_: *mut UmiEventHeader, _: *mut c_void) -> UmiStatus {
    UmiStatus::OK
}

/// A widget that writes one line, the way a typical mod does.
extern "C" fn draw_one_line(hud: *mut UmiHudCx, _: *mut c_void) -> UmiStatus {
    (host_table().hud_line)(hud, UmiStr::from_static("fps 60"), 0x00ff_ffff)
}

fn widget_context() -> Arc<ModContext> {
    let context = ModContext::new("bench".to_owned().try_into().unwrap());
    let user = UserPtr::new(std::ptr::null_mut());
    let placement = HudPlacement::default();
    let registered = context.register_widget("main", "Main", placement, Some(draw_one_line), user);
    assert!(registered.is_ok());
    context.finish_init(true);
    context
}

fn foreign_bus() -> EventBus {
    let context = ModContext::new("bench".to_owned().try_into().unwrap());
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

/// A bus with one foreign observer of `PACKET_RECEIVED` on the network thread.
fn packet_observer_bus() -> (EventBus, Arc<ModContext>) {
    let permissions = Permissions {
        network_packets: true,
        ..Permissions::default()
    };
    let context = ModContext::with_shared(
        "bench".to_owned().try_into().unwrap(),
        permissions,
        Arc::default(),
    );
    let user = UserPtr::new(std::ptr::null_mut());
    let status = context.subscribe(
        UmiEventId::PACKET_RECEIVED,
        UmiMode::OBSERVE,
        0,
        Some(noop),
        user,
    );
    assert_eq!(status, UmiStatus::OK);
    context.finish_init(true);
    let mut bus = EventBus::default();
    install_mod(&mut bus, EventThread::Network, &context);
    (bus, context)
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

    // A typical chunk-sized body, to make the copy for the observer visible.
    let body = vec![0u8; 256];
    group.bench_function("packet_received_no_subscriber", |b| {
        b.iter(|| {
            let mut event = PacketReceived::new(black_box(0x21), black_box(&body));
            empty.dispatch(&mut event)
        });
    });
    let (mut observed, context) = packet_observer_bus();
    group.bench_function("packet_received_one_foreign_observer", |b| {
        b.iter(|| {
            let mut event = PacketReceived::new(black_box(0x21), black_box(&body));
            observed.dispatch(&mut event)
        });
    });
    group.finish();
    drop(context);
}

fn draw_widgets(c: &mut Criterion) {
    let context = widget_context();
    c.bench_function("modhost_hud/draw_one_foreign_widget", |b| {
        b.iter(|| context.draw_widgets());
    });
    c.bench_function("modhost_hud/draw_and_visit_one_foreign_widget", |b| {
        b.iter(|| {
            context.draw_widgets();
            context.visit_widgets(|w| {
                black_box(w.content.lines().len());
            });
        });
    });
}

criterion_group!(benches, dispatch, draw_widgets);
criterion_main!(benches);
