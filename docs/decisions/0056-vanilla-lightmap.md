# 0056. Lighting: the vanilla lightmap replaces the scalar light

Status: accepted

## Context

ADR-0008 §3 modelled light as one scalar per vertex,
`min(1, b(sky) * sun + b(block))`, then the gamma curve. The reference does not
(`EntityRenderer.updateLightmap`, MCP 1.8.8). It builds a 16 x 16 colour table,
indexed by the sky and block level, once per frame, and every lit surface samples
it bilinearly with the light levels of its vertices:

- block light is `table[block] * (flicker * 0.1 + 1.5)`, brighter than its table
  value, and warm: green and blue are shaped by
  `f * ((f * 0.6 + 0.4) * 0.6 + 0.4)` and `f * (f * f * 0.6 + 0.4)`;
- sky light is `table[sky] * (sun * 0.95 + 0.05)`, and the red and green channels
  get a further `sun * 0.65 + 0.35`, so the night sky is bluish; a lightning flash
  drops the sun factor;
- every channel goes through `* 0.96 + 0.03`, so nothing is darker than 0.03;
- the Nether raises the brightness table floor to 0.1, the End replaces the
  formula, night vision lifts the weakest channel, then the gamma option applies;
- torch flicker is a damped random walk stepped once per client tick.

The scalar model made torches grey and dim, the night sky neutral, the Nether and
End too dark and flat, and ignored flicker and lightning on terrain.

## Decision

- `umicraft-render/src/lightmap.rs` ports `updateLightmap` and
  `updateTorchFlicker`, with unit tests per rule.
- The 256 colours ride in the camera uniform (`lightmap`, after `origin`); the
  hand copy and the GUI icon camera carry it too, the icons with every entry
  white because the GUI draws items without the lightmap.
- `lightmap.wgsl` samples it bilinearly. Terrain does it per fragment from the
  interpolated levels, as the reference samples the texture; entities, particles
  and rain do it per vertex.
- The entity floor of 0.05 is gone: the lightmap floor is 0.03.

## Consequences

- Every pass that shows world light reads the same colours. Torches are warm and
  saturate above level 12, night light is blue, lightning lights the terrain.
- Terrain mesh data is unchanged. Smooth lighting still bakes integer levels per
  vertex where the reference keeps fractions (quarter levels); a finer
  `VertexLight` would remove that last difference.
- Boss-bar darkening is not ported.
- The camera uniform grows from 128 bytes to 4224. It is written twice a frame.
- The ignored GPU tests of `offscreen_terrain` and `offscreen_fog` that place a
  wall in a culled chunk already failed before this change; the new
  `the_shader_samples_the_lightmap_the_cpu_built` test covers the lightmap.
