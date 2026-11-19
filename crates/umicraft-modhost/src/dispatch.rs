// SPDX-License-Identifier: MIT OR Apache-2.0

//! Adapters that turn a mod's foreign handler into a handler of the core
//! `EventBus` (ADR-0086).
//!
//! The core bus stays ABI-agnostic; this module adds what a foreign callback
//! needs around the call:
//!
//! - **intercept**: runs synchronously on the owning thread, with the `MUTABLE` and
//!   `CANCELLABLE` flags set for the call only, timed against the budget; a
//!   cancelled header becomes `EventCx::cancel`.
//! - **observe**: hands a copy of the event to the mod's worker thread through a
//!   bounded queue, never blocking the owning thread.
//!
//! A mod that panics, returns an error status or blows its budget is disabled (a
//! flag; `umi_mod_shutdown` runs at exit) and logged once.

use std::sync::Arc;

use umicraft_core::module::ModuleId;
use umicraft_core::EventBus;
use umicraft_modapi_sys::{
    UmiEventId, UmiHandlerFn, UmiMode, UMI_EVENT_CANCELLABLE, UMI_EVENT_MUTABLE,
};

use crate::budget::BUDGET;
use crate::context::{ModContext, Subscription};
use crate::events::{
    ChatReceived, ChatSend, Connected, Disconnected, EventThread, HostEvent, KeyPressed, PostTick,
    PreTick, ScreenClosed, ScreenOpened,
};
use crate::ffi::UserPtr;
use crate::hook::{check_status, run_hook, warn_sparse};
use crate::worker::{Job, Submit};

/// Registers every subscription of `context` that belongs to `thread` on `bus`.
///
/// A disabled mod registers nothing. Handlers keep the mod's `Arc`, so the context
/// outlives every closure that can reach it.
pub fn install_mod(bus: &mut EventBus, thread: EventThread, context: &Arc<ModContext>) {
    if !context.is_enabled() {
        return;
    }
    for subscription in context.all_subscriptions() {
        register(bus, thread, context, subscription);
    }
}

/// The one place that maps an event id to its Rust type.
fn register(
    bus: &mut EventBus,
    thread: EventThread,
    context: &Arc<ModContext>,
    subscription: Subscription,
) {
    match subscription.event {
        UmiEventId::PRE_TICK => adapt::<PreTick>(bus, thread, context, subscription),
        UmiEventId::POST_TICK => adapt::<PostTick>(bus, thread, context, subscription),
        UmiEventId::CHAT_RECEIVED => adapt::<ChatReceived>(bus, thread, context, subscription),
        UmiEventId::CHAT_SEND => adapt::<ChatSend>(bus, thread, context, subscription),
        UmiEventId::CONNECTED => adapt::<Connected>(bus, thread, context, subscription),
        UmiEventId::DISCONNECTED => adapt::<Disconnected>(bus, thread, context, subscription),
        UmiEventId::SCREEN_OPENED => adapt::<ScreenOpened>(bus, thread, context, subscription),
        UmiEventId::SCREEN_CLOSED => adapt::<ScreenClosed>(bus, thread, context, subscription),
        UmiEventId::KEY_PRESSED => adapt::<KeyPressed>(bus, thread, context, subscription),
        // `subscribe` refuses unknown ids, so nothing else can be recorded.
        _ => {}
    }
}

fn adapt<E: HostEvent>(
    bus: &mut EventBus,
    thread: EventThread,
    context: &Arc<ModContext>,
    subscription: Subscription,
) {
    if E::THREAD != thread {
        return;
    }
    match subscription.mode {
        UmiMode::INTERCEPT => intercept::<E>(bus, context, subscription),
        // `subscribe` refuses unknown modes, so the rest is `OBSERVE`.
        _ => observe::<E>(bus, context, subscription),
    }
}

/// Tags a bus handler with its mod, for the bus's per-module diagnostics.
fn tag(context: &ModContext) -> ModuleId {
    ModuleId::new(context.id().as_str())
}

fn intercept<E: HostEvent>(
    bus: &mut EventBus,
    context: &Arc<ModContext>,
    subscription: Subscription,
) {
    let context = Arc::clone(context);
    let owner = tag(&context);
    bus.on::<E, _>(owner, subscription.priority, move |event, cx| {
        if context.is_enabled()
            && addressed_to(event, &context)
            && call_intercept(&context, &subscription, event)
        {
            cx.cancel();
        }
    });
}

/// The flags an intercepting handler sees for `E`.
fn call_flags<E: HostEvent>() -> u32 {
    let mutable = if E::MUTABLE { UMI_EVENT_MUTABLE } else { 0 };
    let cancellable = if E::CANCELLABLE {
        UMI_EVENT_CANCELLABLE
    } else {
        0
    };
    mutable | cancellable
}

/// Calls one intercept handler; returns whether it cancelled the event.
fn call_intercept<E: HostEvent>(
    context: &ModContext,
    subscription: &Subscription,
    event: &mut E,
) -> bool {
    event.header_mut().flags |= call_flags::<E>();
    let header = event.header_ptr();
    let ok = run_hook(
        context,
        context.counters(),
        BUDGET,
        format_args!("its handler for event {}", subscription.event.0),
        || (subscription.handler)(header, subscription.user.as_ptr()),
    );
    // Only the capabilities are lent for the call; the cancel mark stays readable.
    event.header_mut().flags &= !call_flags::<E>();
    // A non-cancellable event ignores the mark: honouring it would let a
    // non-conforming mod skip the other handlers of a tick event.
    ok && E::CANCELLABLE && event.is_cancelled()
}

/// Whether `event` is meant for the mod owning `context`: an event aimed at one mod
/// (`KeyPressed`) must not reach the others (ADR-0086, F3.4 amendment).
fn addressed_to<E: HostEvent>(event: &E, context: &ModContext) -> bool {
    event.target().is_none_or(|target| target == context.id())
}

fn observe<E: HostEvent>(
    bus: &mut EventBus,
    context: &Arc<ModContext>,
    subscription: Subscription,
) {
    if let Err(error) = context.worker().ensure_started(context.id()) {
        let mod_id = context.id();
        tracing::error!(%mod_id, %error, "cannot start the mod worker, its observers will not run");
        return;
    }
    let context = Arc::clone(context);
    let owner = tag(&context);
    let target = Target {
        event: subscription.event,
        handler: subscription.handler,
        user: subscription.user,
    };
    bus.on::<E, _>(owner, subscription.priority, move |event, _cx| {
        if !context.is_enabled() || !addressed_to(event, &context) {
            return;
        }
        let job = observation(&context, target, event.clone());
        if context.worker().submit(job) == Submit::Dropped {
            note_drop(&context);
        }
    });
}

/// Where an observation goes: the copyable part of a subscription.
#[derive(Clone, Copy)]
struct Target {
    event: UmiEventId,
    handler: UmiHandlerFn,
    user: UserPtr,
}

/// The job that gives `copy` to the handler on the worker thread.
///
/// The copy's flags carry neither `MUTABLE` nor `CANCELLABLE`: changing or
/// cancelling a copy would have no effect.
fn observation<E: HostEvent>(context: &Arc<ModContext>, target: Target, mut copy: E) -> Job {
    let context = Arc::clone(context);
    Box::new(move || {
        if !context.is_enabled() {
            return;
        }
        let status = (target.handler)(copy.header_ptr(), target.user.as_ptr());
        check_status(
            &context,
            format_args!("its handler for event {}", target.event.0),
            status,
        );
    })
}

fn note_drop(context: &ModContext) {
    let dropped = context.counters().record_drop();
    warn_sparse(context, dropped, "observer queue full, event dropped");
}

#[cfg(test)]
mod tests;
