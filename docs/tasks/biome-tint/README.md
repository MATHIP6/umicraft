# Biome tints — post-M6 polish task

M6 is accepted, but the terrain still draws grass and leaves with the raw
grayscale textures from the 1.8 client jar: they look grey instead of green.
`umicraft-model` already parses the model `tintindex`, but the mesher ignores
it, `Vertex` has no colour payload and the mesher never reads the biome bytes
stored in `ChunkColumn`.

This task adds the missing tint stage to the M5/M6 pipeline:

```text
biome byte in ChunkColumn
        │
        ▼
BiomeTints (40 base biomes, vanilla climate + overrides)
        │  sampled once per block from a 3×3 biome neighbourhood
        ▼
Vertex.tint (0xRRGGBB) ──▶ terrain.wgsl multiplies the texel
```

The contract is frozen in
[ADR-0009](../../decisions/0009-biome-tint.md); ADR-0008 §2 is amended by it.

## Why this is not M7

The spec roadmap reserves M7 for first-run asset setup and resource packs.
Biome tinting is a correction to the M5/M6 visual result, so it is tracked as
a separate post-M6 task instead of renumbering the roadmap.

## Deliverables

| Area | File / crate | Change |
|---|---|---|
| world | `umicraft-world` | `ChunkColumn::set_biome`/`fill_biomes`, `World::biome` |
| mesh | `umicraft-mesh/src/tint.rs` | biome table, tint classes, 3×3 colour average |
| mesh | `umicraft-mesh/src/lib.rs` | `Vertex.tint`, `BlockSource::biome`, legacy tinting |
| mesh | `umicraft-mesh/src/models.rs` | `ModelMeshContext.tints`, use `ModelQuad.tint` |
| assets | `umicraft-assets/src/colormap.rs` | 256×256 colormap decode and vanilla sampling |
| assets | `umicraft-assets/src/client_resources.rs` | optional runtime load of `colormap/grass.png` and `foliage.png` |
| render | `umicraft-render` | sixth vertex attribute, tint multiply in `terrain.wgsl` |
| client | `umicraft-client` | wire colormaps into the mesh context, plains demo biomes, report line |
| docs | `docs/` | this task, ADR-0009, sources, threading, perf log, README |

## Acceptance

1. Grass blocks show a green top and side overlay; oak/jungle/acacia leaves
   and vines show the foliage colour; water keeps its texture colour except
   in swamps. Stone and other untinted faces stay exactly as before.
2. The tint comes from the biome byte at the block column, averaged over the
   3×3 columns like vanilla `BiomeColorHelper`, so a biome border blends.
3. Without the two colormaps the mesher falls back to white tint and the
   previous no-colour result; no panic and no missing-texture checker.
4. The client report prints whether colormaps were loaded, and the demo scene
   uses plains (id 1) biomes.
5. `--demo-view` / `--block-demo` screenshots show green grass and leaves on
   llvmpipe.
6. Workspace gates and the opt-in GPU tests pass; the mesher benchmark records
   the tint cost in `docs/perf-log.md`.

## Non-goals

- The 1.8 altitude term in `getFloatTemperature` (Perlin noise) is not
  reproduced; the biome base temperature is used. Recorded in
  `docs/sources.md`.
- The swamp grass noise split (`5011004` vs `6975545`) is collapsed to
  `6975545`; the vanilla noise is not part of the world model.
- Resource-pack colour maps, custom biome registries, grass/foliage dye and
  the swamp water blend of other versions.
- No Mojang asset is versioned. `grass.png` and `foliage.png` are read from
  the verified user jar at runtime; tests use synthetic 256×256 PNGs.

## Gates

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check licenses bans
cargo test -p umicraft-render --test offscreen_terrain -- --ignored
cargo test -p umicraft-client block_demo -- --ignored
```
