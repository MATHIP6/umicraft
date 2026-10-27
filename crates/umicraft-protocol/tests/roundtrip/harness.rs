//! Shared round-trip machinery: the [`Dispatch`] abstraction over the
//! per-state packet enums and the macros that stamp out one property test per
//! packet.

use std::fmt::Debug;

use proptest::test_runner::TestCaseError;
use umicraft_protocol::{
    EncodablePacket, Handshake, LoginClientbound, LoginServerbound, PlayClientbound,
    PlayServerbound, ProtocolError, StatusClientbound, StatusServerbound,
};

/// Uniform view of a state/direction packet enum (or the handshake).
pub(crate) trait Dispatch: Sized + PartialEq + Debug {
    /// Wire packet id of this value.
    fn id(&self) -> u8;
    /// Encodes the body (without id or framing).
    fn encode(&self, out: &mut Vec<u8>) -> Result<(), ProtocolError>;
    /// Decodes a body for `id` from `reader`.
    fn decode(id: u8, reader: &mut &[u8]) -> Result<Self, ProtocolError>;
}

macro_rules! impl_dispatch {
    ($($enum:ty),* $(,)?) => {$(
        impl Dispatch for $enum {
            fn id(&self) -> u8 {
                <$enum>::packet_id(self)
            }
            fn encode(&self, out: &mut Vec<u8>) -> Result<(), ProtocolError> {
                <$enum>::encode_body(self, out)
            }
            fn decode(id: u8, reader: &mut &[u8]) -> Result<Self, ProtocolError> {
                <$enum>::decode(id, reader)
            }
        }
    )*};
}

impl_dispatch!(
    PlayClientbound,
    PlayServerbound,
    StatusClientbound,
    StatusServerbound,
    LoginClientbound,
    LoginServerbound,
);

impl Dispatch for Handshake {
    fn id(&self) -> u8 {
        Handshake::ID
    }
    fn encode(&self, out: &mut Vec<u8>) -> Result<(), ProtocolError> {
        self.encode_body(out)
    }
    fn decode(id: u8, reader: &mut &[u8]) -> Result<Self, ProtocolError> {
        if id == Handshake::ID {
            Handshake::decode(reader)
        } else {
            Err(ProtocolError::Invariant(format!(
                "not a handshake id: {id}"
            )))
        }
    }
}

/// Asserts `decode(encode(packet)) == packet` and that nothing is left over.
pub(crate) fn check_round_trip<P: Dispatch>(packet: &P) -> Result<(), TestCaseError> {
    let mut body = Vec::new();
    packet
        .encode(&mut body)
        .map_err(|err| TestCaseError::fail(format!("encode failed for {packet:?}: {err:?}")))?;
    let mut reader = body.as_slice();
    let decoded = P::decode(packet.id(), &mut reader)
        .map_err(|err| TestCaseError::fail(format!("decode failed for {packet:?}: {err:?}")))?;
    proptest::prop_assert_eq!(&decoded, packet);
    proptest::prop_assert!(
        reader.is_empty(),
        "decoder left {} of {} bytes unread for {:?}",
        reader.len(),
        body.len(),
        packet
    );
    Ok(())
}

/// Feeds `bytes` to the decoder of every id in `ids`; only panics are bugs.
pub(crate) fn decode_never_panics<P: Dispatch>(ids: std::ops::RangeInclusive<u8>, bytes: &[u8]) {
    for id in ids {
        let _ = P::decode(id, &mut &bytes[..]);
    }
}

/// One property test inside a module named after the packet.
macro_rules! roundtrip_case {
    ($name:ident, $cases:expr, $ctor:path, $strategy:expr) => {
        #[allow(non_snake_case)]
        mod $name {
            #[allow(unused_imports)]
            use super::*;

            proptest! {
                #![proptest_config(ProptestConfig::with_cases($cases))]
                #[test]
                fn round_trips(value in $strategy) {
                    $crate::harness::check_round_trip(&$ctor(value))?;
                }
            }
        }
    };
}

/// One round-trip test per play packet plus a covered-id list.
///
/// Entries are `Name, cases => strategy;` where `Name` is both the packet type
/// in `$module` and the variant of `$enum`.
macro_rules! play_roundtrips {
    ($enum:ident, $module:ident, $covered:ident, { $($name:ident, $cases:expr => $strategy:expr;)* }) => {
        $( roundtrip_case!($name, $cases, $enum::$name, $strategy); )*

        /// Wire ids that have a round-trip test above.
        pub(crate) const $covered: &[u8] = &[$($module::$name::ID),*];
    };
}
