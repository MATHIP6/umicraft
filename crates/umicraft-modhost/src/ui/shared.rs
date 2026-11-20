// SPDX-License-Identifier: MIT OR Apache-2.0

//! State shared by every mod of a host: the command table, the chat outbox, the
//! packet outbox and the server-compat switch.
//!
//! [`ModHost::load`](crate::ModHost::load) creates one [`HostShared`] and gives it to
//! every [`ModContext`](crate::ModContext). The client takes the other ends: the
//! [`CommandTable`] for the game thread and the receiver of the chat outbox.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};

use umicraft_protocol::PlayServerbound;

use super::commands::{CommandEntry, CommandTable};
use super::{OUTBOX_CAPACITY, PACKET_OUTBOX_CAPACITY};
use crate::error::DeclareError;

/// The command table, the chat outbox, the packet outbox and the server-compat
/// switch of one host.
///
/// `Default` builds an empty one, which is what a context created without a host
/// (tests, benchmarks) gets.
#[derive(Debug)]
pub struct HostShared {
    commands: Mutex<BTreeMap<String, CommandEntry>>,
    outbox: Sender<String>,
    inbox: Receiver<String>,
    dropped: AtomicU64,
    packets: Sender<PlayServerbound>,
    packet_inbox: Receiver<PlayServerbound>,
    server_compat: AtomicBool,
}

impl Default for HostShared {
    fn default() -> Self {
        let (outbox, inbox) = bounded(OUTBOX_CAPACITY);
        let (packets, packet_inbox) = bounded(PACKET_OUTBOX_CAPACITY);
        Self {
            commands: Mutex::default(),
            outbox,
            inbox,
            dropped: AtomicU64::new(0),
            packets,
            packet_inbox,
            server_compat: AtomicBool::new(false),
        }
    }
}

impl HostShared {
    /// A handle to run and complete the registered commands; cheap to clone and
    /// `Send`, so the client can move it to the game thread.
    pub fn commands(self: &Arc<Self>) -> CommandTable {
        CommandTable::new(Arc::clone(self))
    }

    /// The receiving end of the local chat lines mods print with `chat_print`.
    ///
    /// All receivers share one queue: a line goes to exactly one of them.
    pub fn chat_outbox(&self) -> Receiver<String> {
        self.inbox.clone()
    }

    /// How many printed lines were dropped because the outbox was full.
    pub fn dropped_lines(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Queues `line` without blocking. When the outbox is full the line is dropped
    /// and the running total of drops is returned.
    pub(crate) fn print(&self, line: String) -> Option<u64> {
        match self.outbox.try_send(line) {
            Ok(()) => None,
            // Disconnected cannot happen while `inbox` lives in this struct.
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                Some(self.dropped.fetch_add(1, Ordering::Relaxed) + 1)
            }
        }
    }

    /// Whether server-compat mode is on: mods that alter packets or movement are held off.
    pub fn server_compat(&self) -> bool {
        self.server_compat.load(Ordering::Acquire)
    }

    /// Turns server-compat mode on or off; takes effect at once on every thread.
    pub fn set_server_compat(&self, on: bool) {
        self.server_compat.store(on, Ordering::Release);
    }

    /// The receiving end of the packets mods inject; all receivers share one queue.
    pub(crate) fn packet_inbox(&self) -> Receiver<PlayServerbound> {
        self.packet_inbox.clone()
    }

    /// Queues `packet` without blocking; hands it back when the queue is full.
    pub(crate) fn queue_packet(&self, packet: PlayServerbound) -> Result<(), PlayServerbound> {
        self.packets.try_send(packet).map_err(|error| match error {
            // Disconnected cannot happen while `packet_inbox` lives in this struct.
            TrySendError::Full(packet) | TrySendError::Disconnected(packet) => packet,
        })
    }

    pub(crate) fn table(&self) -> MutexGuard<'_, BTreeMap<String, CommandEntry>> {
        // A panic while holding the lock cannot leave the map inconsistent.
        self.commands.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds `/name`; a name another mod (or this one) already took is a conflict.
    pub(crate) fn add_command(&self, name: &str, entry: CommandEntry) -> Result<(), DeclareError> {
        let mut table = self.table();
        if table.contains_key(name) {
            return Err(DeclareError::Duplicate {
                what: "command name",
                value: name.to_owned(),
            });
        }
        table.insert(name.to_owned(), entry);
        Ok(())
    }

    /// Removes the commands of a mod whose initialisation failed.
    pub(crate) fn remove_commands(&self, names: &[String]) {
        let mut table = self.table();
        for name in names {
            table.remove(name);
        }
    }
}
