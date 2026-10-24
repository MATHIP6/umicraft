# ADR-0011 — The world takes world types, not packets

- Status: Implemented
- Date: 2026-10-24
- Deciders: project owner
- Supersedes: none (completes the seam of ADR-0010)
- Related: ADR-0005 (game thread), ADR-0010 (play session)

## Context

`umicraft-world` applied the protocol's packet structs directly
(`apply_chunk_data(&ChunkData)`, `apply_map_chunk_bulk(&MapChunkBulk)`,
`apply_block_change(&BlockChange)`, `apply_multi_block_change(&MultiBlockChange)`),
borrowed the protocol's `chunk_payload_len` and kept two unused 1.8
packed-position helpers. Every crate above the world — `umicraft-game`,
`-mesh`, `-model`, `-render` — therefore depended on the network format
without using it, and the world could not be fed without building packets.

## Decision

1. **The world's update interface uses world types only:**
   `apply_chunk(ChunkPos, ChunkPayload)`,
   `apply_chunk_bulk(has_sky_light, &[(ChunkPos, ChunkPayload)])` (validates
   every payload before mutating anything),
   `apply_block_change(BlockPos, BlockState)` and
   `apply_block_changes(ChunkPos, changes)`. `ChunkPayload` borrows the raw
   1.8 column bytes with their section mask and ground-up flag.
2. **The translation lives with the world's owner.** The client's game
   thread (and the movement probe) map the packets onto these calls;
   `umicraft-net` stays independent of the world.
3. **Each side keeps its payload-length formula.** The world derives it from
   its own section constants because it parses the layout; the protocol keeps
   its own to split `Map Chunk Bulk`. `tests/payload_layout.rs` checks they
   agree for every mask and flag.
4. **The protocol is a dev-dependency of the world only**, for that test.

## Consequences

- No crate below the client depends on the protocol.
- World tests build payloads, not packets.
- The bulk and multi-change translations are written twice (client game
  thread and movement probe), a few lines of field mapping each.
