// SPDX-License-Identifier: MIT OR Apache-2.0

//! Calling a mod's foreign function under a time budget (ADR-0086).
//!
//! Event handlers, widget draw functions and commands all share one policy: the
//! call is timed, a non-`OK` status fails the mod at once, overruns are logged
//! sparsely and a streak of them fails the mod. It lives here once so the three
//! kinds of hook cannot drift apart.

use std::fmt::Display;
use std::time::{Duration, Instant};

use umicraft_modapi_sys::UmiStatus;

use crate::budget::{should_log, Stats, Verdict, MAX_CONSECUTIVE_OVERRUNS};
use crate::context::ModContext;

/// Fails the mod and returns `false` unless `status` is `OK`; `what` names the hook
/// for the log line ("its handler for event 3", "its command `/hi`").
pub(crate) fn check_status(context: &ModContext, what: impl Display, status: UmiStatus) -> bool {
    if !status.is_ok() {
        if status == UmiStatus::PANICKED {
            context.fail(&format!("{what} panicked"));
        } else {
            context.fail(&format!("{what} returned status {}", status.0));
        }
    }
    status.is_ok()
}

/// Runs `call`, a foreign function returning a status, and applies the budget policy.
///
/// Returns whether the call succeeded (status `OK`); the mod is already failed when
/// it did not. A call that is slow but succeeds still returns `true`: only a streak
/// of overruns fails the mod, through `stats`' verdict.
pub(crate) fn run_hook(
    context: &ModContext,
    stats: &Stats,
    budget: Duration,
    what: impl Display,
    call: impl FnOnce() -> UmiStatus,
) -> bool {
    let started = Instant::now();
    let status = call();
    let elapsed = started.elapsed();
    if !check_status(context, what, status) {
        return false;
    }
    match stats.record_call(elapsed, budget) {
        Verdict::Within => {}
        Verdict::Overrun { log } => {
            if log {
                warn_overrun(context, elapsed, budget);
            }
        }
        Verdict::Exhausted => context.fail(&format!(
            "{MAX_CONSECUTIVE_OVERRUNS} calls in a row over the {budget:?} budget"
        )),
    }
    true
}

fn warn_overrun(context: &ModContext, elapsed: Duration, budget: Duration) {
    let mod_id = context.id();
    tracing::warn!(%mod_id, ?elapsed, ?budget, "mod call over its time budget");
}

/// Logs a repeated, droppable condition on its first occurrence and then sparsely.
pub(crate) fn warn_sparse(context: &ModContext, count: u64, message: &str) {
    if should_log(count) {
        let mod_id = context.id();
        tracing::warn!(%mod_id, count, "{message}");
    }
}
