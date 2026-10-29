// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzzes the chunk pipeline: `ChunkData::decode` / `MapChunkBulk::decode`
//! followed by `World::apply_chunk` / `apply_chunk_bulk`, plus raw
//! `ChunkPayload`s built from an arbitrary mask and flags.
//!
//! Byte 0 selects the mode, the rest is the input. In the raw modes the data
//! is padded or truncated to the exact length the mask announces, because a
//! wrong length is rejected up front and would never reach the section parser.
#![no_main]

use libfuzzer_sys::fuzz_target;
use umicraft_protocol::packets::play::clientbound::{ChunkData, MapChunkBulk};
use umicraft_world::chunk::payload_len;
use umicraft_world::{ChunkPayload, ChunkPos, World};

/// Largest payload the raw modes build (a few full sections).
const MAX_RAW_LEN: usize = 1 << 20;

/// Resizes `bytes` to `len` by cycling its content (zeros when empty).
fn fit(bytes: &[u8], len: usize) -> Vec<u8> {
    if bytes.is_empty() {
        return vec![0; len];
    }
    bytes.iter().copied().cycle().take(len).collect()
}

fn apply_raw(world: &mut World, header: &[u8], rest: &[u8], exact: bool) {
    let [mask_hi, mask_lo, flags, ..] = header else {
        return;
    };
    let mask = u16::from_be_bytes([*mask_hi, *mask_lo]);
    let ground_up = flags & 1 != 0;
    let has_sky = world.has_sky_light();
    let len = payload_len(mask, has_sky, ground_up);
    let data = if exact && len <= MAX_RAW_LEN {
        fit(rest, len)
    } else {
        rest.to_vec()
    };
    let payload = ChunkPayload {
        primary_bit_mask: mask,
        ground_up,
        data: &data,
    };
    let _ = world.apply_chunk(ChunkPos::new(0, 0), payload);
}

fuzz_target!(|data: &[u8]| {
    let Some((&mode, input)) = data.split_first() else {
        return;
    };
    let mut world = World::new(if mode & 0x80 != 0 { 0 } else { -1 });
    match mode & 3 {
        0 => {
            if let Ok(packet) = ChunkData::decode(&mut &input[..]) {
                let payload = ChunkPayload {
                    primary_bit_mask: packet.primary_bit_mask,
                    ground_up: packet.ground_up,
                    data: &packet.data,
                };
                let pos = ChunkPos::new(packet.chunk_x, packet.chunk_z);
                let _ = world.apply_chunk(pos, payload);
            }
        }
        1 => {
            if let Ok(packet) = MapChunkBulk::decode(&mut &input[..]) {
                let chunks: Vec<_> = packet
                    .chunks
                    .iter()
                    .map(|chunk| {
                        let payload = ChunkPayload {
                            primary_bit_mask: chunk.primary_bit_mask,
                            ground_up: true,
                            data: &chunk.data,
                        };
                        (ChunkPos::new(chunk.chunk_x, chunk.chunk_z), payload)
                    })
                    .collect();
                let _ = world.apply_chunk_bulk(packet.sky_light, &chunks);
            }
        }
        exact => {
            let (header, rest) = input.split_at(input.len().min(3));
            apply_raw(&mut world, header, rest, exact == 2);
        }
    }
});
