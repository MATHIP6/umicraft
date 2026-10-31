# ADR-0030 — P7: neighbour-dependent block states

**Status:** accepted
**Date:** 2026-10-31
**Milestone:** P7 (ROADMAP.md)

## Context

A wire block state is `id << 4 | metadata`. Fences, panes, walls, stairs,
doors, vines, redstone and the snowy grass need properties that 1.8 computes
from the neighbours (`Block.getActualState`), so the wire state cannot pick
their model. Until P7 the mesher looked models up by wire state only, and
`umicraft-model` mapped such blocks to their "nothing connected" variant.

`umicraft-mesh/src/lib.rs` also held 1 100 lines (vertex, mesh, sources,
legacy cubes, lighting), which would have grown with every P7 step.

## Decision

1. **P7-0: split `umicraft-mesh`.** `lib.rs` keeps the crate doc and re-exports;
   the code lives in `vertex`, `mesh`, `face`, `source`, `legacy` and
   `lighting`, each with its tests. No behaviour change.
2. **Models are also reachable by key.** `BlockModelSet::model_by_key(document,
   variant_key)` serves every variant of every document a definition
   references (baked at build time, shared with the wire-state table). The
   atlas therefore contains the textures of all those variants.
3. **`umicraft-mesh/src/actual_state/` computes the key.** One module per block
   family (`fence`, `pane`, `wall`, `gate`, `stairs`, ...), each exposing
   `handles(id)` and `overrides(...)`, plus one entry in the `FAMILIES` table
   of `mod.rs`. `model_for` merges the overrides into the registry key and asks
   the model set. A block that no family owns returns early (one table scan),
   and a key that names no baked variant falls back to the wire-state model,
   so the worst case is the old look, never a hole.
4. **The logic lives in `umicraft-mesh`, not `umicraft-world`.** It needs the
   registry's property names, which `umicraft-world` does not know.
5. **Connection rules are ported from the MCP methods** (`canConnectTo`,
   `canPaneConnectToBlock`, `func_176307_f`...) and cited in the code and in
   `docs/sources.md`. Block-class tests (`isFullCube`, opaque material) use
   `BlockState::is_opaque()` plus an explicit exclusion list (gourds,
   barrier); the approximation is recorded where it is used.

## Consequences

- Adding a family never touches the mesher: one module, one table line.
- The snowy grass deviation of M5 (metadata bit 0 selecting `snowy`) is
  replaced by the vanilla rule (block above is snow).
- Allocation: one `String` per neighbour-dependent block, only for those
  blocks.
