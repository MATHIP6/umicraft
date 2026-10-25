# ADR-0014 — M7: First-run asset setup and 1.8 resource packs

- Status: Implemented
- Date: 2026-10-25
- Deciders: project owner, Antigravity
- Supersedes: none
- Related: ADR-0001 (reference source policy), ADR-0007 (M5 block models),
  ADR-0012 (terrain textures), ADR-0013 (world view)

## Context

Prior to M7, the client only loaded assets if an official `.minecraft/versions/1.8.8`
installation existed on the machine; otherwise it fell back to procedural textures.
Furthermore, there was no mechanism to load 1.8 resource packs (directories or `.zip`
archives) to override textures, models, blockstates or colormaps.

Spec §12.2 (AST-1, AST-2, AST-3, AST-5) defines the acceptance goals for M7:
1. Provide a first-run asset setup that resolves official Mojang metadata, downloads
   the 1.8.8 assets and client jar with SHA-1 validation and resume support, and caches
   them in the user data directory.
2. Provide full support for 1.8 resource packs with prioritized layering over base
   resources and hot reloading (F3+T / F4) without restarting the game or disconnecting.
3. Maintain clean-room compliance: zero Mojang assets committed to the repository.

## Decision

1. **`ResourcePack` and `ResourceStack` (`umicraft-assets`)**:
   - `ResourcePack` parses directory and `.zip` archives, reading `pack.mcmeta` (format 1),
     `pack.png` preview icon, and all entries under `assets/minecraft/`.
   - `ResourceStack` models an ascending priority stack of active packs.
   - `ClientResources::apply_packs` layers active packs onto base assets, updating the
     virtual file map, rebuilding the deterministic `BlockAtlas` and re-decoding colormaps.

2. **First-Run Setup Flow (`setup.rs` & `setup_cli`)**:
   - `umicraft_assets::setup::find_existing_jar` checks `.minecraft` first, then the local
     Umicraft asset cache (`AssetCache::in_data_dir()`).
   - `umicraft_assets::setup::run_setup` orchestrates the complete download flow with
     step callbacks and resumable transfer tracking.
   - `umicraft-client --setup-assets` provides a CLI entry point with progress feedback.

3. **Hot Reloading & Chunk Re-Meshing (`WorldView` & `GameEvent::ReMeshAll`)**:
   - Pressing `F3 + T` (or `F4`) reloads resource packs from disk.
   - `WorldView::reload_terrain` increments the mesh generation, invalidates pending worker
     jobs, clears current GPU meshes, and initializes a new worker pool.
   - The render thread dispatches `GameEvent::ReMeshAll` to the game thread, which marks
     all loaded chunks as dirty, causing them to be re-meshed and re-drawn immediately.
   - CLI option `--resource-pack <PATH>` is supported across windowed, `--block-demo`, and
     screenshot modes.

## Consequences

- The client can now run with full official textures on machines without an existing
  Minecraft installation after a one-time download into the local cache.
- Resource packs can customize blocks, items, colormaps, and models.
- Hot reload allows authors to test texture modifications in real-time.
- Clean-room rules remain strictly enforced with synthetic test fixtures.
