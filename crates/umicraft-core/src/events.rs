// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed, ordered, cancellable event bus (spec §5.4).
//!
//! Handlers are per-event-type, carry a `priority` (lower runs first, stable
//! within a priority), and receive the event mutable so they can rewrite values.
//! The list is kept sorted at registration, never per dispatch (ADR-0086), so a
//! dispatch with no handler costs one map lookup and emitters can skip building
//! a payload with [`EventBus::has_subscribers`].
//! A handler may cancel the dispatch: later-priority handlers are skipped and
//! the emitter sees `dispatch` return `true`.
//!
//! Storage is type-erased per event type but downcasts are checked against the
//! dispatching type, so a handler registered for the wrong event type is simply
//! never called — not a panic.

use std::any::Any;
use std::any::TypeId;
use std::collections::HashMap;

use crate::module::ModuleId;

/// Handler erased over the event type it accepted at subscription time.
type DispatchFn = Box<dyn FnMut(&mut dyn Any, &mut EventCx) + Send>;

/// Marker for event payload types that can be dispatched through [`EventBus`].
///
/// Not `Send`: a bus belongs to the thread that dispatches on it, so payloads
/// may hold raw pointers (the modding ABI does). The bus itself stays `Send`
/// because its handlers are, so it can be built on one thread and moved.
pub trait Event: 'static {}

impl<T: 'static> Event for T {}

/// Shared state across one dispatch; handlers cancel by calling [`EventCx::cancel`].
#[derive(Debug, Default)]
pub struct EventCx {
    cancelled: bool,
}

impl EventCx {
    /// Stops propagation to lower-priority (later) handlers for this dispatch.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Whether a previous handler cancelled this dispatch.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

struct Subscription {
    priority: i32,
    subscriber: ModuleId,
    // Checked downcast at call time; a mismatch is impossible because this
    // subscription is keyed by the event's TypeId, so the handler skips
    // silently rather than panicking on the (wrong-type) fallback arm.
    run: DispatchFn,
}

impl std::fmt::Debug for Subscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscription")
            .field("priority", &self.priority)
            .field("subscriber", &self.subscriber)
            .finish_non_exhaustive()
    }
}

/// Registry of event handlers keyed by event type.
#[derive(Default)]
pub struct EventBus {
    subscriptions: HashMap<TypeId, Vec<Subscription>>,
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("subscribed_event_types", &self.subscriptions.len())
            .finish()
    }
}

impl EventBus {
    /// Subscribes `handler` to events of type `E`.
    ///
    /// Lower `priority` runs first; equal priorities run in registration order
    /// (the handler is inserted after the last one of priority <= its own).
    /// `subscriber` tags the handler (per-module profiling/debugging in UI-4).
    pub fn on<E, F>(&mut self, subscriber: ModuleId, priority: i32, mut handler: F)
    where
        E: Event,
        F: FnMut(&mut E, &mut EventCx) + Send + 'static,
    {
        let list = self.subscriptions.entry(TypeId::of::<E>()).or_default();
        let at = list.partition_point(|existing| existing.priority <= priority);
        list.insert(
            at,
            Subscription {
                priority,
                subscriber,
                run: Box::new(move |raw: &mut dyn Any, cx: &mut EventCx| {
                    if let Some(event) = raw.downcast_mut::<E>() {
                        handler(event, cx);
                    }
                }),
            },
        );
    }

    /// Whether at least one handler listens to `E`.
    ///
    /// Emitters call this before building a payload that allocates, so an
    /// event nobody listens to costs nothing (ADR-0086).
    #[must_use]
    pub fn has_subscribers<E: Event>(&self) -> bool {
        self.subscriptions
            .get(&TypeId::of::<E>())
            .is_some_and(|list| !list.is_empty())
    }

    /// Dispatches `event` to its handlers in priority order.
    ///
    /// Returns `true` when a handler cancelled the dispatch (the event may
    /// have been mutated by handlers that already ran).
    pub fn dispatch<E: Event>(&mut self, event: &mut E) -> bool {
        let Some(subscriptions) = self.subscriptions.get_mut(&TypeId::of::<E>()) else {
            return false;
        };
        let mut cx = EventCx::default();
        for subscription in subscriptions {
            if cx.is_cancelled() {
                break;
            }
            (subscription.run)(event, &mut cx);
        }
        cx.is_cancelled()
    }

    /// Ids of modules subscribed to `E`, for diagnostics (UI-4 per-mod views).
    #[must_use]
    pub fn subscribers<E: Event>(&self) -> Vec<ModuleId> {
        self.subscriptions
            .get(&TypeId::of::<E>())
            .map(|subscriptions| {
                subscriptions
                    .iter()
                    .map(|subscription| subscription.subscriber.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    struct Probe;

    fn log(
        order: &Arc<Mutex<Vec<&'static str>>>,
        label: &'static str,
    ) -> impl FnMut(&mut Probe, &mut EventCx) {
        let order = order.clone();
        move |_probe: &mut Probe, _cx: &mut EventCx| {
            order.lock().unwrap().push(label);
        }
    }

    #[test]
    fn dispatch_runs_lower_priority_first() {
        let mut bus = EventBus::default();
        let order = Arc::new(Mutex::new(Vec::new()));
        let late = log(&order, "late");
        bus.on::<Probe, _>(ModuleId::new("late"), 10, late);
        let early = log(&order, "early");
        bus.on::<Probe, _>(ModuleId::new("early"), -5, early);

        let mut probe = Probe;
        bus.dispatch(&mut probe);

        assert_eq!(*order.lock().unwrap(), ["early", "late"]);
    }

    #[test]
    fn cancellation_skips_later_handlers() {
        let mut bus = EventBus::default();
        let order = Arc::new(Mutex::new(Vec::new()));
        let after = log(&order, "after");
        bus.on::<Probe, _>(ModuleId::new("after"), 10, after);
        bus.on::<Probe, _>(
            ModuleId::new("cancel"),
            -5,
            |_: &mut Probe, cx: &mut EventCx| {
                cx.cancel();
            },
        );

        let mut probe = Probe;
        assert!(bus.dispatch(&mut probe));
        assert_eq!(*order.lock().unwrap(), Vec::<&'static str>::new());
    }

    #[test]
    fn events_can_be_mutated_by_handlers() {
        struct Counter(u64);
        let mut bus = EventBus::default();
        bus.on::<Counter, _>(ModuleId::new("bump"), 0, |counter: &mut Counter, _cx| {
            counter.0 += 1
        });

        let mut counter = Counter(41);
        bus.dispatch(&mut counter);
        assert_eq!(counter.0, 42);
    }

    #[test]
    fn equal_priorities_keep_registration_order_among_others() {
        let mut bus = EventBus::default();
        let order = Arc::new(Mutex::new(Vec::new()));
        for (label, priority) in [("b1", 5), ("a", -1), ("b2", 5), ("c", 9), ("a2", -1)] {
            let handler = log(&order, label);
            bus.on::<Probe, _>(ModuleId::new(label), priority, handler);
        }

        bus.dispatch(&mut Probe);

        assert_eq!(*order.lock().unwrap(), ["a", "a2", "b1", "b2", "c"]);
        let tags: Vec<_> = bus
            .subscribers::<Probe>()
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        assert_eq!(tags, ["a", "a2", "b1", "b2", "c"]);
    }

    #[test]
    fn has_subscribers_tracks_the_event_type() {
        struct Other;
        let mut bus = EventBus::default();
        assert!(!bus.has_subscribers::<Probe>());
        bus.on::<Probe, _>(ModuleId::new("p"), 0, |_: &mut Probe, _cx| {});
        assert!(bus.has_subscribers::<Probe>());
        assert!(!bus.has_subscribers::<Other>());
    }

    #[test]
    fn bus_is_send_and_events_need_not_be() {
        fn assert_send<T: Send>() {}
        assert_send::<EventBus>();
        // A payload with a raw pointer is a valid event.
        struct Raw(#[allow(dead_code)] *const u8);
        let mut bus = EventBus::default();
        bus.on::<Raw, _>(ModuleId::new("raw"), 0, |_: &mut Raw, _cx| {});
        assert!(!bus.dispatch(&mut Raw(std::ptr::null())));
    }

    #[test]
    fn no_subscribers_is_a_silent_no() {
        let mut bus = EventBus::default();
        let mut probe = Probe;
        assert!(!bus.dispatch(&mut probe));
        assert!(bus.subscribers::<Probe>().is_empty());
    }
}
