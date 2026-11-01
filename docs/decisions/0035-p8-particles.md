# ADR-0035 — Particles: simulation in `umicraft-game`, drawing in `umicraft-render`

**Status:** accepted
**Date:** 2026-11-01
**Milestone:** P8

## Context

The 1.8 `EffectRenderer` mixes the simulation of about 30 `Entity*FX` classes
with their drawing. Umicraft already splits game logic (no GPU) from the render
thread, which never reads the world (ADR-0031).

## Decision

- `umicraft_game::particles` holds `ParticleKind` (42 wire ids), `Particle`
  (the `EntityFX` state and tick, using the entity collision solver),
  `Behaviour` (a closed enum, one module per family, no trait objects) and
  `ParticleEngine` (buckets per layer and blend pass, 4000 cap, seeded
  `JavaRandom` from `umicraft-core`).
- `umicraft-render` draws billboards from the particles the game thread
  publishes in `Frame`; light, block sprites and colours are resolved by
  `umicraft-client`, which knows the model and mesh data.
- Randomness is seeded, not `Math.random()`, so the simulation is repeatable
  in tests.

## Consequences

Each family is a small module with its own tests against the MCP rules, and the
core stays free of the renderer. `umicraft-game` now depends on
`umicraft-core` for `JavaRandom`.
