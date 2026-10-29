// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzzes `PlaySession::handle` with a packet sequence. The input is split
//! into chunks of `[len, id, body...]`, so state carried between packets
//! (join, keepalive, teleport bookkeeping) is exercised too. Errors are fine
//! (they end a real session); panics are not.
#![no_main]

use libfuzzer_sys::fuzz_target;
use umicraft_net::play::{PlayOptions, PlaySession};

/// Splits `input` into `(id, body)` pairs; a leading length byte frames each
/// pair (id included) and a short tail is dropped.
fn packets(mut input: &[u8]) -> impl Iterator<Item = (u8, &[u8])> {
    std::iter::from_fn(move || {
        let (&len, rest) = input.split_first()?;
        let len = usize::from(len);
        if len == 0 || rest.len() < len {
            return None;
        }
        let (chunk, tail) = rest.split_at(len);
        input = tail;
        let (&id, body) = chunk.split_first()?;
        Some((id, body))
    })
}

fuzz_target!(|data: &[u8]| {
    let mut session = PlaySession::new(PlayOptions::default());
    for (id, body) in packets(data) {
        if session.handle(id, body).is_err() {
            break;
        }
    }
});
