//! Strategies for the wire primitives shared by many packets.

use proptest::collection::vec;
use proptest::prelude::*;
use umicraft_protocol::packets::play::metadata::{Metadata, WatchedObject, TERMINATOR};
use umicraft_protocol::packets::play::ItemStack;

/// Cases for ordinary small packets.
pub(crate) const DEFAULT: u32 = 256;
/// Cases for packets that carry kilobytes of payload.
pub(crate) const HEAVY: u32 = 16;

/// Printable non-control text of at most `max_chars` characters.
///
/// Capped at 40 characters regardless of the wire limit: the limit checks are
/// exercised by the packets' unit tests, here the content is what varies.
pub(crate) fn text(max_chars: usize) -> BoxedStrategy<String> {
    let max = max_chars.min(40);
    proptest::string::string_regex(&format!("\\PC{{0,{max}}}"))
        .expect("static regex is valid")
        .boxed()
}

/// Finite `f32` (NaN breaks `PartialEq`, infinities are not interesting).
pub(crate) fn f32s() -> impl Strategy<Value = f32> {
    -1.0e6f32..1.0e6f32
}

/// Finite `f64`.
pub(crate) fn f64s() -> impl Strategy<Value = f64> {
    -1.0e9f64..1.0e9f64
}

/// Block coordinates that survive the packed 26/12/26-bit encoding.
pub(crate) fn block_pos() -> impl Strategy<Value = (i32, i32, i32)> {
    (
        -(1i32 << 25)..(1i32 << 25),
        -(1i32 << 11)..(1i32 << 11),
        -(1i32 << 25)..(1i32 << 25),
    )
}

/// Arbitrary short byte payload.
pub(crate) fn bytes(max_len: usize) -> impl Strategy<Value = Vec<u8>> {
    vec(any::<u8>(), 0..=max_len)
}

fn length_prefixed_i32(data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as i32).to_be_bytes().to_vec();
    out.extend_from_slice(data);
    out
}

fn int_array(ints: &[i32]) -> Vec<u8> {
    let mut out = (ints.len() as i32).to_be_bytes().to_vec();
    ints.iter().for_each(|i| out.extend(i.to_be_bytes()));
    out
}

/// One named NBT entry (tag type, name, payload) with a valid payload.
fn nbt_entry() -> impl Strategy<Value = Vec<u8>> {
    let payload = prop_oneof![
        any::<u8>().prop_map(|v| (1u8, vec![v])),
        any::<i16>().prop_map(|v| (2u8, v.to_be_bytes().to_vec())),
        any::<i32>().prop_map(|v| (3u8, v.to_be_bytes().to_vec())),
        any::<i64>().prop_map(|v| (4u8, v.to_be_bytes().to_vec())),
        any::<u32>().prop_map(|v| (5u8, v.to_be_bytes().to_vec())),
        any::<u64>().prop_map(|v| (6u8, v.to_be_bytes().to_vec())),
        bytes(16).prop_map(|data| (7u8, length_prefixed_i32(&data))),
        "[a-z]{0,12}".prop_map(|s| {
            let mut out = (s.len() as i16).to_be_bytes().to_vec();
            out.extend(s.into_bytes());
            (8u8, out)
        }),
        vec(any::<i32>(), 0..4).prop_map(|ints| {
            // TAG_List whose element type is TAG_Int (3).
            let mut out = vec![3u8];
            out.extend(int_array(&ints));
            (9u8, out)
        }),
        vec(any::<i32>(), 0..4).prop_map(|ints| (11u8, int_array(&ints))),
    ];
    ("[a-zA-Z]{0,8}", payload).prop_map(|(name, (tag, payload))| {
        let mut out = vec![tag];
        out.extend((name.len() as i16).to_be_bytes());
        out.extend(name.into_bytes());
        out.extend(payload);
        out
    })
}

/// A small valid named root compound as raw NBT bytes.
pub(crate) fn nbt_compound() -> impl Strategy<Value = Vec<u8>> {
    ("[a-z]{0,6}", vec(nbt_entry(), 0..5)).prop_map(|(root, entries)| {
        let mut out = vec![10u8];
        out.extend((root.len() as i16).to_be_bytes());
        out.extend(root.into_bytes());
        entries.into_iter().for_each(|entry| out.extend(entry));
        out.push(0);
        out
    })
}

/// `None` (a single `0` byte on the wire) or a small compound.
pub(crate) fn optional_nbt() -> impl Strategy<Value = Option<Vec<u8>>> {
    proptest::option::of(nbt_compound())
}

/// Canonical item stacks: empty slots decode with zeroed fields, so only
/// those are generated for negative ids.
pub(crate) fn item_stack() -> impl Strategy<Value = ItemStack> {
    prop_oneof![
        (i16::MIN..0).prop_map(|item_id| ItemStack {
            item_id,
            stack_size: 0,
            damage: 0,
            nbt: None,
        }),
        (0i16..=i16::MAX, any::<i8>(), any::<i16>(), optional_nbt()).prop_map(
            |(item_id, stack_size, damage, nbt)| ItemStack {
                item_id,
                stack_size,
                damage,
                nbt,
            }
        ),
    ]
}

fn bytes_exact(len: usize) -> BoxedStrategy<Vec<u8>> {
    vec(any::<u8>(), len).boxed()
}

/// Encoded bytes of one watcher value of the given type id.
fn watcher_value(type_id: u8) -> BoxedStrategy<Vec<u8>> {
    match type_id {
        0 => bytes_exact(1),
        1 => bytes_exact(2),
        2 | 3 => bytes_exact(4),
        4 => "\\PC{0,20}"
            .prop_map(|s: String| {
                // At most 80 bytes: the VarInt length fits one byte.
                let mut out = vec![s.len() as u8];
                out.extend(s.into_bytes());
                out
            })
            .boxed(),
        5 => item_stack()
            .prop_map(|stack| {
                let mut out = Vec::new();
                stack
                    .encode(&mut out)
                    .expect("encoding to a Vec cannot fail");
                out
            })
            .boxed(),
        _ => bytes_exact(12),
    }
}

fn watched_object() -> impl Strategy<Value = WatchedObject> {
    (0u8..8, 0u8..32)
        // A float at index 31 has header 0x7F, which is the list terminator
        // on the wire (inherent ambiguity of the 1.8 format).
        .prop_filter("header collides with the terminator", |(type_id, index)| {
            (type_id << 5) | index != TERMINATOR
        })
        .prop_flat_map(|(type_id, index)| {
            watcher_value(type_id).prop_map(move |value| WatchedObject {
                type_id,
                index,
                value,
            })
        })
}

/// Entity metadata with up to six valid watched objects.
pub(crate) fn metadata() -> impl Strategy<Value = Metadata> {
    vec(watched_object(), 0..6).prop_map(|objects| Metadata { objects })
}
