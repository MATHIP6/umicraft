# M5-W6 — client integration and acceptance

Owner worktree: `.worktrees/m5-client`
Branch: `feat/m5-client`
Depends on: W1–W5 merged into `main`

## Goal

Load the official resources once, route windowed/screenshot meshing through the
model mesher and atlas renderer, add the server-less `--block-demo`, and prove
M5 acceptance without committing Mojang assets.

## Owned files

- `crates/umicraft-client/**`
- `README.md`
- `docs/threading.md`

Do not edit model/assets/mesh/render.

## Required behaviour

1. Add a `ModelResources` bundle in the client:
   - `ClientResources` (files + atlas);
   - `Arc<BlockModelSet>` built with `definitions()`;
   - `Arc<ModelMeshContext>` (models + atlas layout).
   Loading failures (no installation, corrupt jar, model error, low GPU limit)
   log one warning and leave `None`; the legacy path must still run.
2. Windowed and screenshot paths:
   - load once before creating the mesh pool/renderer;
   - `MeshPool` gets an optional context and workers call
     `mesh_chunk_with_models`; without it they call `mesh_chunk`;
   - after renderer creation, call `set_block_atlas`; on error warn and fall
     back to procedural textures consistently (do not mix real-layer meshes
     with the fallback array).
   - keep the M4 UI/font path and F2 debug screen working.
3. `--block-demo` (server-less; wins over `--screenshot`? No: it uses
   `--screenshot` for the PNG, like `--ui-demo`):
   - load official resources; if unavailable return a clear typed error;
   - build a synthetic `World` with loaded chunks containing the acceptance
     blocks and a platform;
   - mesh with `mesh_chunk_with_models`, upload, render offscreen;
   - print exactly the counts from ADR-0007 (blockstates, source models,
     artifact counts, layers, states, quads, fallbacks);
   - without `--screenshot`, print the report only (no GPU needed);
   - with `--screenshot PATH`, write a 1280×720 PNG with a readable HUD label.
4. `Args`: add `Mode::BlockDemo`, accept `--block-demo`, allow no host/port only
   for `--ui-demo`/`--block-demo`, update `USAGE`, tests and precedence.
5. Update `README.md` status/controls/commands and `docs/threading.md` for the
   one-time asset load, model meshing and block demo. Mention the fallback.
6. No official asset or extracted texture in the repo.

## Acceptance tests

- args parsing/precedence for `--block-demo`, optional endpoint, screenshot;
- `--block-demo` report contains the exact counts and common block names;
- a headless synthetic scene unit test verifies slab/stair/fence/torch quads
  exist and a slab max height is 0.5;
- the offscreen screenshot test (`#[ignore]`) writes a non-empty 1280×720 PNG;
- fallback smoke with `HOME=/tmp/umicraft-empty-home` still renders the legacy
  path and warns;
- `--ui-demo` output remains unchanged.

Run the full workspace gates plus the ignored GPU tests from the task card
`docs/tasks/m5/00-integration.md` if present; otherwise report which commands
ran. Do not merge; the lead runs final acceptance.
