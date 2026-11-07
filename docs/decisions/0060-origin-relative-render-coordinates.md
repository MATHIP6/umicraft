# 0060. Origin-relative render coordinates (F2-1)

Status: accepted. Implements decision 3 of ADR-0058.

## Context

Far from the world origin (|x| around 1 000 000) an f32 has a 6 to 12 cm grain.
The eye, the entities, the particles and the weather were cast to f32 in world
coordinates before the renderer subtracted its origin, so they moved in jumps.

## Decision

- The eye is an f64 end to end: `RenderState` (`position`, `eye`), `View::sample`,
  `View::retarget` and `Camera::eye()`. `Camera::position()` is only a rounded
  copy for sound and rough distances.
- The **render origin** is `Camera::render_origin()` = `floor(eye)` in f64: a
  whole number, hence exact in f32. `Camera::relative_eye()` is the eye
  relative to it (0..1 per axis). The matrix, the frustum boxes, the chunk
  sort and the fog distance (`camera_pos` and `world_position` in the shaders)
  use numbers relative to it.
- Two conventions for vertices, one per kind of data:
  - **terrain, block outline, cracks, moving blocks**: absolute positions
    (exact for block corners and 1/16 details up to 2^19), the shader
    subtracts `camera.origin`;
  - **entities, block entities, particles, dropped items, weather**: already
    relative to the render origin, built in f64 before the cast. They are drawn
    with the *local* camera uniform (same matrix, `origin` = 0), through
    `local_camera_bind_group` and `local_terrain_bind_group`.
- `EntityFrameData` positions, the cape, the bobber tip and the leash holder are
  f64. `EntityFrameData::interpolate(alpha, origin)` interpolates in f64 and
  subtracts `origin`; the snapshot carries `origin`. `ShadowBlock` holds the
  integer block coordinate, made relative by the shadow builder.
- Block entities: `BlockEntityScene::origin` (whole blocks). The renderers get
  an entry whose `position` is relative; lookups by world position and
  animation seeds use `BlockEntityScene::world_position`.
- Scenes built in world coordinates (demos, GPU tests) call
  `Camera::pin_render_origin(DVec3::ZERO)`; the game never pins.

## Not covered

- Terrain vertices stay absolute f32: model details at 1/16 are exact up to
  2^19 = 524 288 and snap to 1/8 above 2^20. Section-relative vertices
  (phase 3, F2-3c) remove this.
- Clouds animate from the absolute f64 eye; the selection outline and cracks
  keep absolute f32 vertices.
- The CPU name tag projection assumes the snapshots use the camera's origin.

## Tests

`camera.rs` (view changes for a 1 cm step at 1 000 000.5, relative eye),
`entity/snapshot.rs` (smooth lerp at 1M, origin subtracted), `particle_view.rs`,
`weather/geometry.rs`, and the GPU tests `far_origin_tests` (same picture at 0
and at 1 000 000, a 5 cm step moves the pig).
