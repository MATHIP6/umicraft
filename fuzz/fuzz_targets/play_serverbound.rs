// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzzes `PlayServerbound::decode` (first byte = packet id, rest = body).
//! Serverbound decoding is what a test server or proxy would run on hostile
//! client input; it must return `Err`, never panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use umicraft_protocol::PlayServerbound;

fuzz_target!(|data: &[u8]| {
    if let Some((&id, mut body)) = data.split_first() {
        let _ = PlayServerbound::decode(id, &mut body);
    }
});
