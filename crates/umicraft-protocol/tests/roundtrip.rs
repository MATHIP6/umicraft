//! Property-based round-trip tests for the protocol-47 packets (ADR-0020 §6).
//!
//! For every packet of the handshake, status, login and play states (all 74
//! clientbound play ids `0x00..=0x49` and all 26 serverbound play ids
//! `0x00..=0x19`) a `proptest` strategy produces valid values and checks that
//! `Dispatch::decode(id, encode(x)) == x` *through the state's dispatch enum*
//! and that the decoder consumes every byte of the body.
//!
//! Decoders that are intentionally lossy are only fed canonical values: an
//! unknown particle id decodes as the barrier particle (35), unknown objective
//! render types as `Integer`, serverbound `PlayerAbilities` masks its flags
//! to four bits, `MapChunkBulk` payloads are sized by their bit masks, and
//! metadata whose header equals the `0x7F` terminator cannot exist. Empty
//! item stacks (negative id) decode with zeroed fields.
//!
//! Two additional properties feed arbitrary bytes to every play decoder id and
//! assert that nothing panics. A final test fails when a play packet exists
//! without a strategy.

#[macro_use]
#[path = "roundtrip/harness.rs"]
mod harness;
#[path = "roundtrip/clientbound.rs"]
mod clientbound;
#[path = "roundtrip/common.rs"]
mod common;
#[path = "roundtrip/login_status.rs"]
mod login_status;
#[path = "roundtrip/serverbound.rs"]
mod serverbound;

use proptest::prelude::*;
use umicraft_protocol::{PlayClientbound, PlayServerbound};

use harness::decode_never_panics;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn clientbound_decoders_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        decode_never_panics::<PlayClientbound>(0..=0x49, &bytes);
    }

    #[test]
    fn serverbound_decoders_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        decode_never_panics::<PlayServerbound>(0..=0x49, &bytes);
    }
}

fn sorted(ids: &[u8]) -> Vec<u8> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids
}

#[test]
fn every_play_packet_has_a_round_trip_strategy() {
    assert_eq!(
        sorted(clientbound::CLIENTBOUND_COVERED),
        (0..=0x49).collect::<Vec<u8>>(),
        "clientbound play ids without a round-trip strategy (or duplicated)"
    );
    assert_eq!(
        sorted(serverbound::SERVERBOUND_COVERED),
        (0..=0x19).collect::<Vec<u8>>(),
        "serverbound play ids without a round-trip strategy (or duplicated)"
    );
}
