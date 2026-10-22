# M5-W3 — client resources and texture atlas

Owner worktree: `.worktrees/m5-atlas`
Branch: `feat/m5-atlas`
Depends on: M5 setup skeleton (committed on `main`)

## Goal

Read the verified client jar once, expose it as a `ResourceSource`, and build
the deterministic block/item texture array.

## Owned files

- `crates/umicraft-assets/src/atlas.rs`
- `crates/umicraft-assets/src/client_resources.rs`
- `crates/umicraft-assets/src/client_jar.rs`
- `crates/umicraft-assets/src/error.rs` (add variants only)
- `crates/umicraft-assets/src/lib.rs` (module exports only)
- `crates/umicraft-assets/tests/**`

Do not edit `umicraft-model`, mesh, render or client.

## Required behaviour

1. `ClientJar` reads its archive once for a bulk operation. Add a typed method
   that returns all `(name, bytes)` entries whose name starts with any requested
   prefix, using one `ZipArchive::open` and one slab read. Do not call
   `read_entry` in a loop (it would re-read the 100 MB jar per entry).
2. `ClientResources::load(jar)`:
   - loads all `assets/minecraft/blockstates/` `*.json`;
   - loads all `assets/minecraft/models/block/` `*.json`;
   - loads all `assets/minecraft/textures/blocks/` and
     `assets/minecraft/textures/items/` `*.png`;
   - ignores `.mcmeta` and any non-requested entry;
   - keeps the full virtual path as the key.
3. `BlockAtlas`:
   - layout keys are texture keys relative to `textures/`, e.g.
     `blocks/stone`, `items/apple`;
   - uses `AtlasLayout::from_keys` so missing is layer 0 and the rest sort;
   - layer 0 is an original procedural missing checker (no Mojang art);
   - each layer is RGBA8; decode PNG with the `png` workspace dependency;
   - all textures must be the same size (1.8 is 16×16); reject otherwise with
     `AssetsError::Image` naming the entry;
   - layer-major contiguous pixels, length = width*height*4*layers.
4. `ClientResources::get`, `file_count`, `atlas` accessors and
   `ResourceSource` (`read` returns a typed resource error with the virtual
   path; `list` filters keys by prefix).
5. Tests use tiny synthetic zip fixtures/PNGs only:
   - bulk read returns only the requested prefixes;
   - two synthetic 2×2 PNGs decode into the expected sorted layers;
   - missing layer is opaque/checker-like and not zero length;
   - wrong image size and truncated PNG -> typed `AssetsError::Image`;
   - `ResourceSource::read/list` work for loaded and missing paths.

## Optional official smoke (outside the repo)

If `/home/mathip/.minecraft/versions/1.8.8/1.8.8.json` and the verified jar are
available, write a temporary example/test outside the tracked tree or use an
`#[ignore]` test. It must report exactly 340 blockstate files, 1080 block model
files, 382 block PNGs, 229 item PNGs and 612 layers. Do not commit the jar or
any extracted PNG.

## Gates

```sh
cargo fmt -p umicraft-assets -- --check
cargo clippy -p umicraft-assets --all-targets -- -D warnings
cargo test -p umicraft-assets
RUSTDOCFLAGS="-D warnings" cargo doc -p umicraft-assets --no-deps
```

Report the commit SHA, test count and the official counts if observed. Do not
merge.
