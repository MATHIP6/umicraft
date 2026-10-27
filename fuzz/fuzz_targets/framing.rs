// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzzes the frame reader (`Framing::read_frame`, `unwrap_body`) in raw mode
//! and with compression on. Byte 0 picks the threshold; the rest is the wire
//! stream. Corrupt lengths and zlib streams must give `Err`, and the
//! decompressed size must stay bounded.
#![no_main]

use libfuzzer_sys::fuzz_target;
use umicraft_protocol::Framing;

fuzz_target!(|data: &[u8]| {
    let Some((&mode, mut stream)) = data.split_first() else {
        return;
    };
    let framing = match mode {
        0 => Framing::Raw,
        threshold => Framing::Compressed {
            threshold: u32::from(threshold),
        },
    };
    // Read frames until the stream is exhausted or malformed.
    while !stream.is_empty() {
        if framing.read_frame(&mut stream).is_err() {
            break;
        }
    }
    let _ = framing.unwrap_body(stream);
});
