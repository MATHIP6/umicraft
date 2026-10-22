// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shared in-memory tracing subscriber for client unit tests.
//!
//! Several loaders (font, block resources) deliberately degrade with one
//! warning instead of failing. The tests need to observe that warning without
//! writing to the process-wide subscriber, so they run the loader inside
//! [`capture`].

use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span;
use tracing::{Event, Metadata, Subscriber};

/// A subscriber that records every event message.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<String>>>);

impl Subscriber for Capture {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut message = Message(String::new());
        event.record(&mut message);
        if let Ok(mut messages) = self.0.lock() {
            messages.push(message.0);
        }
    }

    fn enter(&self, _span: &span::Id) {}

    fn exit(&self, _span: &span::Id) {}
}

/// The message field of one event.
struct Message(String);

impl Visit for Message {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_string();
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

/// Runs `operation` with a subscriber that records every event message.
pub(crate) fn capture<T>(operation: impl FnOnce() -> T) -> (T, Vec<String>) {
    let subscriber = Capture::default();
    let value = tracing::subscriber::with_default(subscriber.clone(), operation);
    let messages = subscriber
        .0
        .lock()
        .map(|messages| messages.clone())
        .unwrap_or_default();
    (value, messages)
}
