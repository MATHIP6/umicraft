// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzzes `PlayClientbound::decode`: the first input byte is the packet id,
//! the rest is the body. A malformed body must yield `Err`, never a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use umicraft_protocol::PlayClientbound;

fuzz_target!(|data: &[u8]| {
    if let Some((&id, mut body)) = data.split_first() {
        let _ = PlayClientbound::decode(id, &mut body);
    }
});
