# ADR-0003 — Packet module layout: one file per packet, grouped by state and direction

- Status: Accepted
- Date: 2026-10-21
- Deciders: project owner

## Context

M1 shipped as a single `packets.rs` (≈1,150 lines) holding every packet for
the handshake, status, login, and the M1 play subset. Each state was one enum
with flat field variants, and each enum carried the decode/encode logic for
all of its variants. M2 and later add whole families of packets (chunks,
entities, inventory, …), so the flat layout would keep growing in one file and
would duplicate every layout between the enum and any future reusable codec.

The public surface (`umicraft_protocol::{Handshake, StatusClientbound,
LoginServerbound, PlayClientbound, PlayServerbound}`) is consumed by
`umicraft-net`, `umicraft-testkit` (fake server), and the integration tests.
The refactor must not change wire behavior.

## Options

1. Keep flat-field enum variants and split only the file, moving codecs into
   free functions per packet. Rejected: each layout would exist twice (free
   function plus enum arm), inviting drift.
2. One file per packet with a struct, and replace the state enums with a
   trait/registry (static or dynamic dispatch). Rejected for now: much more
   churn in `umicraft-net` and the fake server; the enum dispatch table is
   cheap and will stay small until the full packet set lands.
3. One file per packet with a struct, keeping the state enums as thin
   dispatch tables whose variants wrap the structs. *(chosen)*

## Decision

- Packets live under
  `packets/{handshake,status,login,play}/{clientbound,serverbound}/<packet>.rs`.
  `clientbound` flows server → client (**inbound** on a client); `serverbound`
  flows client → server (**outbound** on a client). The names follow the
  wiki.vg tables and the state enums.
- Each packet file contains:
  - a `//!` module doc with the wire layout and its source (wiki.vg field
    names, MCP class name, or observed traffic per ADR-0001),
  - a `pub struct` with documented fields,
  - `pub const ID: u8`,
  - `pub fn decode`, an `EncodablePacket` impl, and a wire-shape or round-trip
    test that pins the layout.
- The state enums (`StatusClientbound`, `LoginServerbound`, …) stay as
  dispatch tables: variants wrap the packet structs, `decode` dispatches on
  `<Struct>::ID`, and unknown ids still return `ProtocolError::UnknownPacket`
  so callers keep their skip behavior.
- Fixed-width field primitives live in `packets/codec.rs`; VarInt and
  string/array codecs stay in `varint.rs` and `types.rs`.
- The crate-root re-exports are unchanged, so consumers keep matching on the
  enums. Constructing an outbound packet names the direction module, e.g.
  `PlayServerbound::KeepAlive(play::serverbound::KeepAlive { id })`.
- Directions are named `clientbound`/`serverbound` rather than
  `inbound`/`outbound` because the project is a client but the protocol crate
  is direction-agnostic and the fake server consumes the same types; the
  inbound/outbound mapping is documented in every state's module doc.

## Consequences

- Adding a packet is mechanical: one new file, one new variant, one dispatch
  arm, one test in the same file.
- Layouts have a single source of truth; tests live next to the code they pin.
- `packets/mod.rs` shrinks to the module map, re-exports, and the
  `EncodablePacket` trait.
- Import paths for packet structs are longer in consumer code; if that becomes
  noisy, the enums can grow constructor helpers without changing the layout.
- No wire behavior changes: the refactor was done in four mechanically
  verifiable commits (codec extraction, status/handshake, login, play) with
  `fmt`, `clippy -D warnings`, the full test suite, and a live-server smoke
  run at the end.
