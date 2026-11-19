// SPDX-License-Identifier: MIT OR Apache-2.0

//! The per-mod worker thread that runs observe handlers (ADR-0086).
//!
//! Observers get a copy of the event on a thread of their own, through a bounded
//! queue, so a slow observer never delays the thread that owns the event. A full
//! queue drops the event (the caller counts it) instead of blocking.

use std::io;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use crossbeam_channel::{bounded, Sender, TrySendError};

use crate::manifest::ModId;

/// Observations a mod may have waiting before new ones are dropped.
pub const QUEUE_CAPACITY: usize = 256;

/// One observation to run on the worker thread.
pub(crate) type Job = Box<dyn FnOnce() + Send>;

/// What happened to a submitted job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Submit {
    /// Queued.
    Queued,
    /// Not queued: the queue is full, or the worker is not running.
    Dropped,
}

/// A running worker: the sending half of its queue and its thread.
#[derive(Debug)]
struct Running {
    sender: Sender<Job>,
    thread: JoinHandle<()>,
}

#[derive(Debug, Default)]
enum State {
    /// Not started yet.
    #[default]
    Idle,
    Running(Running),
    /// Stopped for good; a mod is never restarted after shutdown.
    Stopped,
}

/// The lazily started worker of one mod.
///
/// Dispatch adapters reach it through the mod's context, so [`WorkerSlot::stop`]
/// can close the queue for all of them at once: sender clones captured elsewhere
/// would keep the thread alive.
#[derive(Debug, Default)]
pub(crate) struct WorkerSlot {
    state: Mutex<State>,
}

impl WorkerSlot {
    fn lock(&self) -> MutexGuard<'_, State> {
        // A panic while holding the lock cannot leave the state inconsistent.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts the worker `umicraft-mod-<id>` unless it already runs.
    ///
    /// # Errors
    ///
    /// The OS refused to spawn the thread. After [`WorkerSlot::stop`] this is a
    /// no-op: a stopped worker stays stopped.
    pub(crate) fn ensure_started(&self, id: &ModId) -> io::Result<()> {
        let mut state = self.lock();
        if !matches!(*state, State::Idle) {
            return Ok(());
        }
        let (sender, receiver) = bounded::<Job>(QUEUE_CAPACITY);
        let thread = std::thread::Builder::new()
            .name(format!("umicraft-mod-{id}"))
            .spawn(move || receiver.iter().for_each(|job| job()))?;
        *state = State::Running(Running { sender, thread });
        Ok(())
    }

    /// Queues `job` without blocking.
    pub(crate) fn submit(&self, job: Job) -> Submit {
        match &*self.lock() {
            State::Running(running) => match running.sender.try_send(job) {
                Ok(()) => Submit::Queued,
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => Submit::Dropped,
            },
            State::Idle | State::Stopped => Submit::Dropped,
        }
    }

    /// Closes the queue, lets the worker finish the jobs already queued and joins it.
    ///
    /// The host calls this before `umi_mod_shutdown`: no handler may run after it
    /// (shutdown-quiescence rule, ADR-0086). Returns once the thread has exited.
    pub(crate) fn stop(&self) {
        let previous = std::mem::replace(&mut *self.lock(), State::Stopped);
        if let State::Running(Running { sender, thread }) = previous {
            drop(sender);
            if thread.join().is_err() {
                tracing::error!("a mod worker thread panicked");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc};

    use super::*;

    fn id() -> ModId {
        "w".to_owned().try_into().unwrap()
    }

    #[test]
    fn jobs_run_on_the_named_thread_and_stop_drains_then_joins() {
        let slot = WorkerSlot::default();
        slot.ensure_started(&id()).unwrap();
        let done = Arc::new(AtomicUsize::new(0));
        let (names_tx, names_rx) = mpsc::channel();
        for _ in 0..3 {
            let (done, names_tx) = (done.clone(), names_tx.clone());
            let job = Box::new(move || {
                names_tx
                    .send(std::thread::current().name().map(str::to_owned))
                    .unwrap();
                done.fetch_add(1, Ordering::SeqCst);
            });
            assert_eq!(slot.submit(job), Submit::Queued);
        }
        slot.stop();
        assert_eq!(done.load(Ordering::SeqCst), 3);
        assert_eq!(
            names_rx.try_iter().next().flatten().as_deref(),
            Some("umicraft-mod-w")
        );
    }

    #[test]
    fn a_full_queue_drops_instead_of_blocking() {
        let slot = WorkerSlot::default();
        slot.ensure_started(&id()).unwrap();
        let (release, gate) = mpsc::channel::<()>();
        // The first job parks the worker so the queue can fill behind it.
        assert_eq!(
            slot.submit(Box::new(move || {
                let _ = gate.recv();
            })),
            Submit::Queued
        );
        let mut queued = 0;
        let mut dropped = 0;
        for _ in 0..QUEUE_CAPACITY + 10 {
            match slot.submit(Box::new(|| {})) {
                Submit::Queued => queued += 1,
                Submit::Dropped => dropped += 1,
            }
        }
        // The parked job may or may not have left the queue yet: one slot of slack.
        assert!((QUEUE_CAPACITY - 1..=QUEUE_CAPACITY).contains(&queued));
        assert_eq!(queued + dropped, QUEUE_CAPACITY + 10);
        release.send(()).unwrap();
        slot.stop();
    }

    #[test]
    fn submit_before_start_and_after_stop_drops_and_stop_is_final() {
        let slot = WorkerSlot::default();
        assert_eq!(slot.submit(Box::new(|| {})), Submit::Dropped);
        slot.ensure_started(&id()).unwrap();
        slot.stop();
        slot.stop();
        slot.ensure_started(&id()).unwrap();
        assert_eq!(slot.submit(Box::new(|| {})), Submit::Dropped);
    }
}
