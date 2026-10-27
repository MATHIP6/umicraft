# Fuzzing

Umicraft fuzzes its untrusted-input parsers with [`cargo fuzz`](https://rust-fuzz.github.io/book/)
(libFuzzer). The crate lives in `fuzz/`, outside the main workspace
(`exclude = ["fuzz"]`), because it needs the nightly toolchain. Rationale:
`docs/decisions/0020-p1-protocol-completeness.md` section 6.

## Install

```sh
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --locked
```

## Targets

| Target | Covers |
|---|---|
| `play_clientbound` | `PlayClientbound::decode(id, body)`; byte 0 = packet id, rest = body. |
| `play_serverbound` | `PlayServerbound::decode(id, body)`, same input shape. |
| `play_session` | `PlaySession::handle` over a packet sequence, framed as `[len, id, body...]` chunks, to exercise session state. |
| `chunk_parser` | Byte 0 selects the mode: `ChunkData::decode` + `World::apply_chunk`, `MapChunkBulk::decode` + `World::apply_chunk_bulk`, or a raw `ChunkPayload` (arbitrary mask/flags, data as given or fitted to the exact announced length so the section parser is reached). Bit 7 picks overworld vs. nether (sky light on/off). |
| `framing` | `Framing::read_frame` / `unwrap_body`, raw and compressed (byte 0 = threshold, 0 = raw). |

A decoder must return `Err` on bad input and never panic; the targets ignore
`Err` results.

## Run

```sh
cd fuzz
cargo +nightly fuzz run <target> -- -max_total_time=600
```

Hand-written seeds are in `fuzz/seeds/<target>/`; copy them into the
(gitignored) corpus before the first run:

```sh
for t in seeds/*; do mkdir -p corpus/$(basename $t); cp $t/* corpus/$(basename $t)/; done
```

Crashing inputs are written to `fuzz/artifacts/<target>/crash-<hash>`. Replay
with `cargo +nightly fuzz run <target> artifacts/<target>/crash-<hash>`,
shrink with `cargo +nightly fuzz tmin`. `corpus/`, `artifacts/`, `target/` and
`coverage/` are gitignored. Once a bug is fixed, add a regression unit test in
the affected crate with the minimised input.

## Runs

| Date | Target | Duration | Result |
|---|---|---|---|
| 2026-10-26 | play_clientbound | 60 s | no crash (~152k runs) |
| 2026-10-26 | play_serverbound | 60 s | no crash (~148k runs) |
| 2026-10-26 | play_session | 60 s | no crash (~2.1M runs) |
| 2026-10-26 | framing | 60 s | no crash (~537k runs) |
| 2026-10-26 | chunk_parser | 60 s | crash: `World::apply_chunk` with chunk coordinate `i32::MIN`/`MAX` panics with `attempt to subtract with overflow` in `ChunkPos::neighbours`. **Fixed** (wrapping neighbours, regression test in `pos.rs`). |
| 2026-10-26 | all five | 600 s each | no crash; slow units in `play_clientbound` (0x2F, 0x30), `play_serverbound` (0x0E) and `play_session`: item-stack NBT with a huge list of `TAG_End` or nested lists took up to 426 s per input. **Fixed** with the reference client's NBT limits (`packets/play/nbt.rs`, regression tests). |
| 2026-10-26 | play_clientbound | 600 s | no crash, no slow unit (10.7M runs) — after the NBT fix |
| 2026-10-26 | play_serverbound | 600 s | no crash, no slow unit (8.9M runs) |
| 2026-10-26 | play_session | 600 s | no crash, no slow unit (14.9M runs) |
| 2026-10-26 | chunk_parser | 600 s | no crash, no slow unit (46.5k runs; each input applies whole columns) |
| 2026-10-26 | framing | 600 s | no crash, no slow unit (1.2M runs) |

P1's "fuzzers run 10 minutes without a panic" criterion is met by the
last five rows (ROADMAP.md P1).
