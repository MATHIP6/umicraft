# ADR 0031: P7f weather has no client ramp and is sampled on the game thread

Status: accepted (2026-10-31)

## Context

The task brief expected a client-side `updateWeatherBody` ramp (0.01 per tick). In
the 1.8 reference `World.updateWeather` only runs when `!isRemote` and
`WorldClient.updateWeather` is empty: the client shows the strengths the
server sends with `Change Game State` 7 and 8 (which the server sends whenever
its own ramp changes them), and `setRainStrength` sets the previous and the
current value alike. `renderRainSnow` also asks the world for the precipitation
height, the biome and the light of up to 441 columns on every frame.

## Decision

- `Weather` mirrors the reference exactly: no ramp, no interpolation, thunder
  weighted by rain, a lightning flash counter driven by the live bolt entities.
- The game thread samples a 25 x 25 `PrecipitationField` once per tick, and only
  while it rains, and publishes it behind an `Arc` in `Frame`. The render thread
  builds the quads from it and never reads the world.
- The biome table (`umicraft_mesh::climate`) reuses the tint temperature table;
  precipitation height is a scan (`ChunkColumn::precipitation_height`), not a
  cached height map, so block changes cannot leave it stale.

## Consequences

The weather moves at 20 Hz with the server, like the reference. The light of a
rain quad is the one at the precipitation height, and the temperature noise is
left out (see `docs/tasks/p7/README.md`).
