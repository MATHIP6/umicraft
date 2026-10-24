// SPDX-License-Identifier: MIT OR Apache-2.0

//! The world and the protocol each compute the 1.8 chunk payload length: the
//! world to parse a column, the protocol to split a `Map Chunk Bulk`. The two
//! formulas must never drift apart.

use umicraft_protocol::packets::play::clientbound::chunk_data::chunk_payload_len;
use umicraft_world::chunk::payload_len;

#[test]
fn the_world_and_the_protocol_agree_on_every_payload_length() {
    for mask in 0..=u16::MAX {
        for has_sky_light in [false, true] {
            for ground_up in [false, true] {
                assert_eq!(
                    payload_len(mask, has_sky_light, ground_up),
                    chunk_payload_len(mask, has_sky_light, ground_up),
                    "mask {mask:#06x}, sky {has_sky_light}, ground-up {ground_up}"
                );
            }
        }
    }
}
