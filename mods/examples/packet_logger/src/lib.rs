// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example mod: packet logger.
//!
//! Shows the packet API: observing both directions, sharing counters between
//! the network thread and the game and render threads, and injecting a packet.
//! It needs `network_packets = true` in `mod.toml`; without it the host
//! refuses the subscriptions and the injection. See `mods/examples/README.md`
//! for how to build and install it.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, PoisonError};

use umicraft_modapi::{
    umicraft_mod, Anchor, Chat, InitCx, Logger, Mod, ModError, PacketReceived, PacketSend, Packets,
    Placement, PostTick,
};

/// Game ticks per second; the report runs once per this many ticks.
const TICKS_PER_SECOND: u64 = 20;

/// The serverbound `Animation` packet (protocol 47): the player swings their
/// arm. Its body is empty, so the host accepts it with no bytes.
const ANIMATION: u8 = 0x0A;

/// HUD text colour.
const WIDGET_COLOR: u32 = 0x00FF_FFFF;

/// What one packet id did in the current window.
#[derive(Debug, Default, Clone, Copy)]
struct Count {
    packets: u64,
    bytes: u64,
}

/// Counts per packet id since the last report, for one direction.
#[derive(Debug, Default)]
struct Direction {
    window: Mutex<BTreeMap<u8, Count>>,
    /// Packets in the last finished window, for the HUD.
    per_second: AtomicU64,
}

impl Direction {
    /// Called on the network thread for every packet.
    fn record(&self, id: u8, body: &[u8]) {
        // A poisoned lock only means another handler panicked mid-update;
        // the counts are still usable, and a logger must not stop logging.
        let mut window = self.window.lock().unwrap_or_else(PoisonError::into_inner);
        let count = window.entry(id).or_default();
        count.packets += 1;
        count.bytes += body.len() as u64;
    }

    /// Starts a new window and returns the finished one.
    fn rotate(&self) -> BTreeMap<u8, Count> {
        let finished =
            std::mem::take(&mut *self.window.lock().unwrap_or_else(PoisonError::into_inner));
        let total = finished.values().map(|count| count.packets).sum();
        self.per_second.store(total, Relaxed);
        finished
    }

    /// Logs one line per packet id seen in `finished`.
    fn log(&self, log: Logger, label: &str, finished: &BTreeMap<u8, Count>) {
        for (id, count) in finished {
            log.info(&format!(
                "{label} 0x{id:02x} x{} {} bytes",
                count.packets, count.bytes
            ));
        }
    }
}

/// Both directions, shared by the packet handlers, the report and the widget.
#[derive(Debug, Default)]
struct Traffic {
    received: Direction,
    sent: Direction,
}

/// The mod's state, kept alive by the host between `init` and `shutdown`.
struct PacketLogger {
    log: Logger,
}

impl Mod for PacketLogger {
    /// Runs once when the client loads the library: subscribe and register.
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        let log = cx.logger();
        let traffic = Arc::new(Traffic::default());

        // Packet events fire on the network thread, so the handlers do the
        // least possible: bump a counter. Observers get a read-only copy of
        // the packet and never delay the connection. (Interceptors could
        // cancel a packet, which is what makes a mod able to break a session;
        // this one only watches.)
        let counts = Arc::clone(&traffic);
        cx.observe::<PacketReceived>(0, move |event| {
            counts.received.record(event.id(), event.data());
        })?;
        let counts = Arc::clone(&traffic);
        cx.observe::<PacketSend>(0, move |event| {
            counts.sent.record(event.id(), event.data());
        })?;

        // Reporting on the game thread's tick keeps the formatting and the
        // log calls off the network thread. Every 20th tick is one second.
        let report = Arc::clone(&traffic);
        cx.observe::<PostTick>(0, move |event| {
            if event.tick() % TICKS_PER_SECOND == 0 {
                let received = report.received.rotate();
                let sent = report.sent.rotate();
                report.received.log(log, "in ", &received);
                report.sent.log(log, "out", &sent);
            }
        })?;

        let placement = Placement::new(Anchor::TopLeft).offset(4.0, 60.0);
        cx.hud_widget("packets", "Packets", placement, move |hud| {
            let incoming = traffic.received.per_second.load(Relaxed);
            let outgoing = traffic.sent.per_second.load(Relaxed);
            hud.line(&format!("Packets in: {incoming}/s"), WIDGET_COLOR);
            hud.line(&format!("Packets out: {outgoing}/s"), WIDGET_COLOR);
        })?;

        let (chat, packets) = (cx.chat(), cx.packets());
        cx.command("swing", "/swing", move |_| swing(chat, packets))?;
        Ok(Self { log })
    }

    /// Runs once when the mod is disabled or the client exits.
    fn shutdown(&mut self) {
        self.log.info("packet logger stopped");
    }
}

/// Injects an `Animation` packet and tells the player how it went.
///
/// The host checks the body like a packet the client would send and queues it;
/// it is written about one tick later. `Busy` means its queue is full, which
/// a real mod would retry, and `HostRefused` covers a missing permission.
fn swing(chat: Chat, packets: Packets) -> Result<(), ModError> {
    match packets.send(ANIMATION, &[]) {
        Ok(()) => {
            chat.print("swing sent");
            Ok(())
        }
        Err(error) => {
            chat.print(&format!("swing failed: {error}"));
            Err(error)
        }
    }
}

umicraft_mod!(PacketLogger);
