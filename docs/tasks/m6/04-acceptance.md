# M6-W4 — acceptance harness, screenshots and performance

Owner worktree: `/home/mathip/dev/rust/mc-project/.worktrees/m6-acceptance`
Branch: `feat/m6-acceptance`
Depends on: M6 setup (ADR-0008). The harness can start now; final screenshots
and the perf rows wait for W1–W3 to be merged.

## Goal

Extend the server-less `--block-demo` into the M6 acceptance scene: open ground
with AO corners, a lit cave, water, glass and leaves. Render day, night, cave
and transparency shots, print stable counts, and record the Criterion numbers
against the M2 baselines. No Mojang asset is written to the repository.

## Owned files

- `crates/umicraft-client/src/block_demo.rs`
- `crates/umicraft-client/src/args.rs`
- `crates/umicraft-client/src/screenshot.rs`
- `crates/umicraft-client/tests/**`
- `docs/perf-log.md`

Do **not** edit `game.rs`, `network.rs`, `window.rs`, `main.rs`, render, world,
mesh, ADR/tasks, or root `README.md`. If the live screenshot path needs a new
field from `Frame`, coordinate with the W3 handoff; never edit `game.rs`.

## Part 1 — acceptance scene (`block_demo.rs`)

Keep the existing M5 block rows and the exact ADR-0007 texture/model stats
(the existing fixture tests must keep passing). Add an M6 wing to the same
single demo column without making the M5 report counts unstable:

1. **Ground and AO corner.** Leave a flat sky-lit area at y = 65 with a
   three-block L-shaped stone corner (a wall two blocks high plus a side wall)
   so an inside corner receives AO values 15/10/5/0. Put a second free-standing
   pillar for a silhouette check.
2. **Cave.** Carve a 3×3×3 room below the platform (roof at y = 63, floor at
   y = 61). Set every cave cell to `sky = 0`; set block light 14 at a glowstone
   cell in the wall and a falloff ring 12/10/8/6 around it for the screenshot.
   Use `ChunkColumn::set_sky_light`/`set_block_light`. The cave must be visibly
   dark outside the glowstone falloff.
3. **Transparency.** A 3×3 water pool one block deep, a 2×2 glass window in the
   stone wall, and a 2×2 leaf canopy in front of a sky-lit wall. Water, glass
   and leaves must be meshed into the transparent pass, not the opaque one.
4. **Sky.** Set full sky light 15 on every open-air cell (the mesher's default
   is already full sky, but the demo should exercise real nibbles). The sun,
   moon and clouds come from W3; do not draw them in the demo code.
5. Add a `--time N` option (ticks, signed; default 6000 noon). It is only used
   by `--block-demo` and the offscreen demo scene; live play keeps the server
   clock. The value becomes `RenderEnvironment::world_time`.
6. Add a `--demo-view NAME` option for the acceptance camera:
   - `overview` (default): the existing four-row camera plus the new wing;
   - `day`: ground/AO/water at noon (`--time 6000` must be supplied or default);
   - `night`: same camera at midnight (`--time 18000`);
   - `cave`: inside the cave looking at the glowstone falloff;
   - `transparency`: close-up of water/glass/leaves against stone.
   Keep `--block-demo` as the single mode; do not add a new top-level mode.
7. `render_scene` must call `Renderer::set_environment(RenderEnvironment::new(
   0, args.time, 0.0, args.view_distance))` before `render`. The renderer owns
   the sky/fog pass; the demo never writes a sky texture.
8. Extend `BlockDemoReport` with stable fields:
   - `time: i64` and `view: &'static str`;
   - `opaque_quads` and `transparent_quads` from the mesh counts;
   - `ao_shaded_vertices`: vertices whose unpacked `ao < 15`;
   - `sky_vertices`: vertices with `sky < 15` (cave checks);
   - `block_lit_vertices`: vertices with `block > 0`;
   - existing stats and scene labels.
   Print them in the same aligned report format. The report path must work
   without a GPU and without writing a PNG.
9. Keep the fallback no-assets path usable; this demo still needs the official
   installation for real models/textures, and must fail with the existing
   typed error when it is missing.

## Part 2 — screenshots (`block_demo.rs` / `screenshot.rs`)

`--block-demo --screenshot PATH` writes one PNG for the selected view/time.
The acceptance run writes four files:

```sh
target/debug/umicraft-client --block-demo --demo-view day \
  --time 6000 --screenshot /tmp/m6-day.png
target/debug/umicraft-client --block-demo --demo-view night \
  --time 18000 --screenshot /tmp/m6-night.png
target/debug/umicraft-client --block-demo --demo-view cave \
  --time 6000 --screenshot /tmp/m6-cave.png
target/debug/umicraft-client --block-demo --demo-view transparency \
  --time 6000 --screenshot /tmp/m6-transparency.png
```

Visual checks (record PASS/FAIL in the handoff, do not commit large PNGs):

- day: blue sky, clouds visible, sun/moon not inverted, AO corner darker than
  the open face but not black;
- night: sky is dark, stars/moon visible, terrain darker than day, glowstone
  still readable;
- cave: sky/fog is not bright blue, walls outside the glowstone radius are
  near-black, the falloff is visible;
- transparency: water tints what is behind it, glass is transparent, leaves
  are cut out and do not write opaque black; no obvious missing faces.

For the live `--screenshot PATH` path, after W3 merges, read the newest `Frame`
and call `set_environment` with its `world_time`, `dimension`, interpolated
`partial_tick`, and `args.view_distance`. Do not edit `game.rs`; if the frame
fields are missing, stop and report it to the lead. The screenshot must still
work with the official installation and with the procedural fallback.

## Part 3 — performance (`docs/perf-log.md`)

M2 recorded meshing baselines, not a GPU frame benchmark. M6 therefore uses
`mesh/flat_view_169_chunks` as the frame-budget proxy (one full 13×13 view,
single-threaded) plus the single-chunk and dense-section rows. The acceptance
budget is **≤ 110 % of 114.3 ms = 125.7 ms** for that proxy; record the raw
Criterion time and the percentage. Also record the single-chunk row against
446.7 µs, the dense row against 685.7 µs, and the model-path row separately so
the M5 regression is visible.

Run after W1 is merged (and again after W2/W3 if the renderer changes meshing):

```sh
cargo bench -p umicraft-mesh --bench mesh -- --quick
```

Append rows in the existing format, for example:

```
2026-10-22 | mesh/flat_chunk_with_neighbours | 446.7 µs (M2) | <time> | M6 smooth light + AO; ... |
```

If the proxy exceeds 125.7 ms, profile before claiming acceptance and report
the hot spot. Do not edit the ADR or task cards.

## Tests

- `Args::parse` accepts `--time -6000`, `--time 18000`, `--demo-view cave` and
  rejects an unknown view or a non-integer time. Existing parse tests keep
  passing.
- The report is deterministic for a synthetic fixture and includes the new
  fields. Existing ADR-0007 counts remain unchanged.
- A mesh test with water/glass/leaves asserts `transparent_quads > 0` and
  `opaque_quads > 0`; this may be an ignored test if it needs the official
  installation.
- Add an ignored GPU acceptance test (or a small scripted assertion) that
  renders day and night and checks the mean luminance of the terrain area is
  lower at night, and that a cave frame is darker than the day frame. Use
  `Renderer::read_pixels`; skip gracefully (not fail) when no adapter is
  available, like the existing opt-in tests.

## Gates

```sh
cargo fmt -p umicraft-client -- --check
cargo clippy -p umicraft-client --all-targets -- -D warnings
cargo test -p umicraft-client
RUSTDOCFLAGS="-D warnings" cargo doc -p umicraft-client --no-deps
cargo test -p umicraft-client block_demo -- --ignored
```

The final acceptance run happens after W1/W2/W3 are merged, on the official
installation at `/home/mathip/.minecraft` and with the llvmpipe GPU. Report:
commit SHA, commands, report excerpts, PNG dimensions, visual PASS/FAIL and the
perf table. Do not merge.
