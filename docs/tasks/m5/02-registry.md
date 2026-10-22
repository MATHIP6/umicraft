# M5-W2 — 1.8 block registry

Owner worktree: `.worktrees/m5-registry`
Branch: `feat/m5-registry`
Depends on: M5 setup skeleton (committed on `main`)

## Goal

Replace the empty `umicraft-model/src/registry.rs` with the complete 1.8
block-id table, derived from the local MCP reference under ADR-0001.

## Owned files

- `crates/umicraft-model/src/registry.rs`
- `crates/umicraft-model/tests/registry.rs`

Do not edit the parser/geometry files, `atlas.rs`, or any other crate. Do not
commit any Mojang asset (`mcp/` is gitignored).

## Required data

For every registered block id `1..=197` that has a blockstate document, add a
`BlockDefinition` with its 1.8 registry name and `StateDef` rules. Use the
reference sources:

- `mcp/temp/src/minecraft/net/minecraft/block/Block.java`
  (`func_176219_a(id, name, ...)` registrations);
- each `Block*.java` class's `getStateFromMeta`/`getMetaFromState` (or the
  equivalent `func_176203_a`/`func_176201_c`);
- the blockstate JSON keys in
  `mcp/temp/src/minecraft/assets/minecraft/blockstates/*.json` to verify the
  canonical variant strings.

Rules:

- first matching `mask/value` wins; metadata bits outside `mask` are ignored;
- separate blockstate documents per wood/colour family are allowed and
  expected (`planks_oak`, `log_spruce`, `wool`, `carpet`, ...);
- keys must be canonical (alphabetical `name=value` pairs, or `normal`);
- omit technical blocks with no blockstate document; the mesher falls back;
- do not silently invent a variant key: tests should fail if a key is absent
  from the real document. Use the fallback document `first()` only at runtime.

## Priority acceptance states

Make these exact:

| id | meta | blockstate | variant |
|---|---|---|---|
| 1 | 0..6 | stone | stone/granite/... as shipped |
| 2 | 0/1 | grass | `snowy=false` / `snowy=true` |
| 3 | 0..2 | dirt | variant as shipped |
| 4 | 0 | cobblestone | normal |
| 5 | 0..5 | planks_oak/... | variant=... |
| 17 | 0/4/8/12 | log_oak/... | axis=y/x/z/none |
| 18 | meta&3 | leaves_oak/... | normal |
| 20 | 0 | glass | normal |
| 35 | 0..15 | wool | color=... |
| 44 | 0/8 | stone_slab | half=bottom/top |
| 45 | 0 | bricks | normal |
| 49 | 0 | obsidian | normal |
| 53 | 0..15 | oak_stairs | facing/half/shape |
| 78 | 0..7 | snow_layer | layers=1..8 |
| 89 | 0 | glowstone | normal |
| 155 | 0..4 | quartz_block | variant=... |
| 171 | 0..15 | carpet | color=... |

## Tests

- `definitions().len()` is plausible (>= 150) and ids are unique;
- every `blockstate` string names an existing file in the reference fixture
  checked manually (test names, not paths);
- the table above resolves to the expected strings;
- metadata masks work for logs, leaves, slabs, stairs, wool, snow and quartz;
- unknown id -> `None`;
- the table is ordered by id.

Add a short module-level comment giving the exact MCP symbols used. The lead
will add the matching row to `docs/sources.md`.

## Gates

```sh
cargo fmt -p umicraft-model -- --check
cargo clippy -p umicraft-model --all-targets -- -D warnings
cargo test -p umicraft-model
RUSTDOCFLAGS="-D warnings" cargo doc -p umicraft-model --no-deps
```

Report the commit SHA and the definition count. Do not merge.
