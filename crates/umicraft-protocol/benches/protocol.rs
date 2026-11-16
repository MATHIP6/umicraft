// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reproducible Criterion benchmarks for network framing, zlib compression,
//! and Minecraft 1.8 packet decoding.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use umicraft_protocol::packets::play::clientbound::ChunkData;
use umicraft_protocol::varint::{read_var_u32, write_var_u32};
use umicraft_protocol::{EncodablePacket, Framing, PlayClientbound};

const FLAT_SECTION: &[u8] =
    include_bytes!("../../umicraft-world/tests/fixtures/flat-chunk-section.bin");

fn encoded_chunk_packet() -> Vec<u8> {
    let packet = ChunkData {
        chunk_x: -107,
        chunk_z: 6,
        ground_up: true,
        primary_bit_mask: 0x0001,
        data: FLAT_SECTION.to_vec(),
    };
    let mut raw = Vec::with_capacity(FLAT_SECTION.len() + 32);
    raw.push(packet.packet_id());
    packet.encode_body(&mut raw).expect("encode chunk data");
    raw
}

fn bench_varint_roundtrip(criterion: &mut Criterion) {
    criterion.bench_function("protocol/varint_10k_roundtrip", |bencher| {
        let mut buf = Vec::with_capacity(50_000);
        bencher.iter(|| {
            buf.clear();
            for i in 0..10_000_u32 {
                write_var_u32(&mut buf, black_box(i.wrapping_mul(131))).expect("write");
            }
            let mut slice = buf.as_slice();
            let mut acc = 0_u64;
            for _ in 0..10_000 {
                acc += u64::from(read_var_u32(&mut slice).expect("read"));
            }
            black_box(acc)
        });
    });
}

fn bench_framing_zlib_chunk(criterion: &mut Criterion) {
    let raw_body = encoded_chunk_packet();
    let framing = Framing::Compressed { threshold: 256 };
    let mut framed = Vec::new();
    framing
        .write_frame(&mut framed, &raw_body)
        .expect("compress chunk frame");
    // Strip the outer packet_len varint to benchmark `unwrap_body` directly as `Connection` does.
    let mut reader = framed.as_slice();
    let _len = umicraft_protocol::varint::read_var_u32(&mut reader).expect("len");
    let inner_frame = reader.to_vec();

    let mut group = criterion.benchmark_group("protocol/framing_12k_chunk");
    group.bench_function("unwrap_zlib_inflate", |bencher| {
        bencher.iter(|| {
            black_box(
                framing
                    .unwrap_body(black_box(&inner_frame))
                    .expect("inflate"),
            )
        });
    });
    group.bench_function("write_zlib_deflate", |bencher| {
        let mut out = Vec::with_capacity(4096);
        bencher.iter(|| {
            out.clear();
            framing
                .write_frame(&mut out, black_box(&raw_body))
                .expect("deflate");
            black_box(out.len())
        });
    });
    group.finish();
}

fn bench_decode_chunk_packet(criterion: &mut Criterion) {
    let raw_body = encoded_chunk_packet();
    let id = raw_body[0];
    let payload = &raw_body[1..];
    criterion.bench_function("protocol/decode_chunk_data_12k", |bencher| {
        bencher.iter(|| {
            let mut slice = black_box(payload);
            black_box(PlayClientbound::decode(id, &mut slice).expect("decode chunk packet"))
        });
    });
}

fn bench_unwrap_packet_chunk(criterion: &mut Criterion) {
    let raw_body = encoded_chunk_packet();
    let framing = Framing::Compressed { threshold: 256 };
    let mut framed = Vec::new();
    framing
        .write_frame(&mut framed, &raw_body)
        .expect("compress chunk frame");
    let mut reader = framed.as_slice();
    let _len = umicraft_protocol::varint::read_var_u32(&mut reader).expect("len");
    let inner_frame = reader.to_vec();

    criterion.bench_function("protocol/unwrap_packet_chunk_12k", |bencher| {
        bencher.iter(|| {
            black_box(
                framing
                    .unwrap_packet(black_box(&inner_frame))
                    .expect("unwrap packet"),
            )
        });
    });
}

fn bench_burst_100_packets_peel(criterion: &mut Criterion) {
    let framing = Framing::Raw;
    let mut stream = Vec::with_capacity(4096);
    for i in 0..100_u8 {
        let packet = [0x15, i, 1, 2, 0];
        framing.write_frame(&mut stream, &packet).unwrap();
    }

    criterion.bench_function("protocol/burst_100_packets_peel", |bencher| {
        bencher.iter(|| {
            let mut cursor = 0usize;
            let slice = black_box(&stream);
            let mut count = 0;
            while cursor < slice.len() {
                let mut consumed = 0;
                let Some(len) =
                    umicraft_protocol::varint::take_var_u32(&slice[cursor..], &mut consumed)
                        .unwrap()
                else {
                    break;
                };
                let total = consumed + len as usize;
                let frame = &slice[cursor + consumed..cursor + total];
                let (id, body) = framing.unwrap_packet(frame).unwrap();
                cursor += total;
                count += id as usize + body.len();
            }
            black_box(count)
        });
    });
}

criterion_group!(
    benches,
    bench_varint_roundtrip,
    bench_framing_zlib_chunk,
    bench_decode_chunk_packet,
    bench_unwrap_packet_chunk,
    bench_burst_100_packets_peel
);
criterion_main!(benches);
