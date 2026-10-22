# M5-W1 — model core

Owner worktree: `.worktrees/m5-model-core`
Branch: `feat/m5-model-core`
Depends on: M5 setup skeleton (committed on `main`)

## Goal

Implement the pure 1.8 blockstate/model parser and face baker in
`umicraft-model`, as frozen by ADR-0007.

## Owned files

- `crates/umicraft-model/src/error.rs`
- `crates/umicraft-model/src/blockstate.rs`
- `crates/umicraft-model/src/model.rs`
- `crates/umicraft-model/src/geometry.rs`
- `crates/umicraft-model/src/lib.rs` (exports only; do not move public items)
- `crates/umicraft-model/tests/**` except `registry.rs`
- `crates/umicraft-model/Cargo.toml` only if a dependency is genuinely needed
  (ask the lead first)

Do **not** edit `registry.rs` (W2), `atlas.rs` (lead), or any other crate.

## Required behaviour

1. Parse blockstate JSON: `variants` map; each value is one object or an array;
   `model`, `x`, `y`, `uvlock`, `weight` (default 1). Reject rotations that are
   not multiples of 90 and weights of zero. Canonicalize keys by sorting their
   comma-separated `name=value` pairs. `normal` is the empty key.
2. Parse model JSON: `parent`, `textures`, `elements`, `ambientocclusion`.
   Resolve parents recursively with cycle detection and a depth limit. Child
   textures override parent textures. A child with `elements` replaces the
   parent elements; without elements it inherits them. Merge `particle`.
   `#` references resolve transitively and must end at a concrete texture key.
3. Parse elements: `from`/`to` (three numbers, allowed `-16..32`), `faces`
   (at least one), `rotation` (`origin`, `axis`, `angle` in
   `-45/-22.5/0/22.5/45`, `rescale`) and `shade` (default `true`).
4. Parse faces: `texture`, optional `uv` (four numbers), optional `cullface`,
   `rotation` (0/90/180/270), `tintindex`. Ignore the extra `cull` key.
5. Bake quads exactly as ADR-0007 describes:
   - vanilla `EnumFaceDirection` corner order and auto UVs;
   - explicit UV mapping with face rotation;
   - element rotation around `origin/16`, with the 1/cos(angle) rescale on the
     two perpendicular axes;
   - blockstate `x`/`y` rotation around `(0.5, 0.5, 0.5)` (x first, then y,
     angles negated like `ModelRotation`);
   - `uvlock` UVs derived from the rotated position with the 1.8 `FaceBakery`
     formula;
   - cullface rotated with the variant;
   - computed normalized normal from the final corners.
6. `BlockModelSet::build`:
   - parse every `assets/minecraft/models/block/` file reported by
     `source.list(...)` for validation and `source_model_count`;
   - parse every unique blockstate name in `StateDef`s and every
     `assets/minecraft/blockstates/` file for `blockstate_count`;
   - map all 16 metadata values of every definition that has a matching
     `StateDef`; absent keys fall back to the document's first variant;
   - cache resolved models and variants; choose the declared weight;
   - collect concrete texture keys from all quads.
7. Keep `AtlasLayout` untouched.

## Tests (synthetic JSON only)

- parent texture merge and child override;
- parent elements inherited, child elements replace;
- parent cycle -> typed error;
- unresolved `#texture` -> typed error;
- cube: 6 quads, outward normals, UV top convention;
- cross: 4 quads, `shade == false`, rotated 45°;
- bottom slab: every `y <= 0.5`; top slab: every `y >= 0.5`;
- stairs: > 6 quads and full height;
- element rotation and `rescale` change the expected bounds;
- blockstate y rotation rotates cullface and positions;
- weighted variants preserve order/weights;
- face UV rotation 90° swaps the expected corner UVs;
- `build` counts synthetic models/blockstates and resolves a metadata mapping.

## Gates

```sh
cargo fmt -p umicraft-model -- --check
cargo clippy -p umicraft-model --all-targets -- -D warnings
cargo test -p umicraft-model
RUSTDOCFLAGS="-D warnings" cargo doc -p umicraft-model --no-deps
```

Report the commit SHA, test count and any frozen-API concern. Do not merge.
