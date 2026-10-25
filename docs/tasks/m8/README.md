# M8 Workstream cards and launch guide

M8 is the entity system, models, skins and rendering milestone (spec §12.2, ADR-0015).
This directory holds the launch cards for the workstreams. The contract they
compile against is
[`docs/decisions/0015-m8-entity-system-and-rendering.md`](../../decisions/0015-m8-entity-system-and-rendering.md).

## Workstreams

| Card | Workstream | Responsibility | Depends on |
|---|---|---|---|
| `00-lead.md` | W0 — Lead & integration | ADR-0015, interface freeze, workspace integration | — |
| `01-protocol-metadata.md` | W1 — Entity protocol & typed metadata | `SpawnMob`, `SpawnObject`, entity movements, `TypedMetadata` | ADR-0015 |
| `02-entity-store.md` | W2 — Entity state & interpolation | `EntityStore`, 20 Hz simulation, movement lerp, snapshot | W1 |
| `03-skin-assets.md` | W3 — Skins & mob textures | Mojang session fetch, skin cache, default Steve/Alex, mob textures | ADR-0015 |
| `04-models-animation.md` | W4 — Entity models & animation | Box models, 1.8 humanoid dual-layer, common mobs, walking math | ADR-0015 |
| `05-entity-renderer.md` | W5 — Entity GPU renderer | `entity.wgsl`, `EntityPass`, instance/vertex buffers, lighting/fog | W3, W4 |
| `06-client-acceptance.md` | W6 — Client integration & acceptance | `--entity-demo`, screenshot, offscreen tests, perf baseline | W2, W5 |

## Wave Plan

1. **Wave 1 — W1, W2, W3, W4 in parallel**:
   - W1 implements packet decoders and typed metadata in `umicraft-protocol` and `umicraft-net`.
   - W2 creates the game-thread `EntityStore` in `umicraft-game`.
   - W3 implements skin fetching, cache and conversion in `umicraft-assets`.
   - W4 implements box models, humanoid skeleton and animation math in `umicraft-model`.
2. **Wave 2 — W5 (GPU Renderer)**:
   - W5 implements the entity GPU pipeline in `umicraft-render`, binding models (W4), skins (W3) and lighting.
3. **Wave 3 — W6 (Acceptance & Tooling)**:
   - W6 implements `--entity-demo`, tests on live Paper 1.8 server, offscreen screenshots, and Criterion benchmarks.

## Quality Gates

Before any workstream handoff:
```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check licenses bans
```

## Milestone Status: Completed

All waves and workstreams W0 through W6 are completed and verified:
- **W0 (Lead & Architecture)**: ADR-0015 adopted, Clean-room policy enforced.
- **W1 (Protocol & Metadata)**: `SpawnMob`, `SpawnObject`, `EntityVelocity`, `EntityHeadLook`, `EntityMetadata`, `AttachEntity` packets decoded, `TypedMetadata` accessors tested.
- **W2 (Game State & Interpolation)**: `EntityStore` managing non-local entities, 20 Hz tick, limb swing animation, partial-tick snapshot interpolation.
- **W3 (Skins & Textures)**: Skin model derivation (Classic/Slim from UUID), 64×32 to 64×64 converter, clean-room procedural fallback skin (64×64 RGBA), Mojang sessionserver fetch.
- **W4 (Entity Models & Animation)**: Articulated `ModelBox` and `ModelPart`, dual-layer `HumanoidModel` (Steve/Alex/Zombie), `QuadrupedModel` (Cow).
- **W5 (Entity GPU Renderer)**: `entity.wgsl` shader with face shading, block/sky light, alpha cutout, and fog integration; `EntityRenderer` and batch drawing in `Renderer`.
- **W6 (Client Integration & Acceptance)**: `--entity-demo` synthetic scene with 5 entities (Steve, Alex, Zombie, Cow, Dropped Item), multiple camera views (`--demo-view {overview, back, closeup, walk-cycle}`), offscreen 1280×720 PNG rendering (`--screenshot target/m8-entities.png`), and live windowed rendering integration in `window.rs` via `EntityTextures`. All workspace tests green.
