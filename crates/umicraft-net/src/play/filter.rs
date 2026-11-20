// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hook through which mods observe, drop and add play packets.
//!
//! [`drive`](super::drive) asks a [`PacketFilter`] about every packet that
//! crosses the socket. The trait lives in this crate so `umicraft-net` needs
//! no knowledge of the mod host; the client implements it on top of the host.

use umicraft_protocol::packets::play::clientbound::{Disconnect, SetCompression};
use umicraft_protocol::PlayServerbound;

/// Observes and alters the packets of a play session.
///
/// Every method defaults to "let it through", so an implementation overrides
/// only what it needs. The filter runs on the loop's task: it must not block.
pub trait PacketFilter {
    /// Called for each clientbound packet before the session handles it.
    ///
    /// Returning `false` drops the packet: the session never sees it, so it
    /// produces no reply and no event. The packet still counts as received.
    /// `drive` ignores the verdict for `Set Compression` and `Disconnect`:
    /// dropping the first would desynchronise the framing from the server,
    /// dropping the second would hide the kick reason from the player.
    fn receive(&mut self, id: u8, body: &[u8]) -> bool {
        let _ = (id, body);
        true
    }

    /// Called for each serverbound packet about to be written: commands,
    /// session replies and injected packets alike.
    ///
    /// Returning `false` drops the packet; it is then not counted as sent.
    fn send(&mut self, packet: &PlayServerbound) -> bool {
        let _ = packet;
        true
    }

    /// Called after every loop turn to append packets to write.
    ///
    /// The packets go through [`send`](Self::send) like any other. `out` is
    /// empty on entry and reused between turns.
    fn inject(&mut self, out: &mut Vec<PlayServerbound>) {
        let _ = out;
    }
}

/// The filter that changes nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoFilter;

impl PacketFilter for NoFilter {}

/// Whether the packet `id` must reach the session whatever the filter says.
pub(super) fn is_protected(id: u8) -> bool {
    id == SetCompression::ID || id == Disconnect::ID
}
