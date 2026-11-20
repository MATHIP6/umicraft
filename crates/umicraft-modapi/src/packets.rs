// SPDX-License-Identifier: MIT OR Apache-2.0

//! Injecting packets into the connection.

use umicraft_modapi_sys::UmiBytes;

use crate::{host, ModError};

/// Sends packets to the server as if the client had written them.
///
/// Cheap to copy and usable from any thread. Get one with
/// [`InitCx::packets`](crate::InitCx::packets).
#[derive(Debug, Clone, Copy)]
pub struct Packets;

impl Packets {
    /// Queues a serverbound play packet: `id` and its `body` (no id, no length
    /// prefix, uncompressed, protocol 47).
    ///
    /// The host validates the body like a packet the client would send itself
    /// and writes it on the network thread, so it arrives about one tick
    /// later. It passes through [`PacketSend`](crate::PacketSend) handlers like
    /// any other outgoing packet, this mod's included.
    ///
    /// Requires `network_packets = true` in `mod.toml`.
    ///
    /// ```ignore
    /// // Animation (0x0A) has an empty body: the player swings their arm.
    /// cx.packets().send(0x0A, &[])?;
    /// ```
    ///
    /// # Errors
    /// - [`ModError::HostRefused`] with `DENIED` without the permission or
    ///   while the mod is disabled, and with `INVALID_ARGUMENT` for a body the
    ///   client would not send (unknown serverbound id, malformed or trailing
    ///   bytes). Before `init` it is `UNSUPPORTED`.
    /// - [`ModError::Busy`] when the host's queue is full; retry later.
    pub fn send(&self, id: u8, body: &[u8]) -> Result<(), ModError> {
        // The ABI wants a null pointer for no bytes, not Rust's dangling one.
        let data = if body.is_empty() {
            UmiBytes::EMPTY
        } else {
            UmiBytes {
                ptr: body.as_ptr(),
                len: body.len(),
            }
        };
        host::call(|host, me| (host.packet_send)(me, u32::from(id), data)).and_then(ModError::check)
    }
}
