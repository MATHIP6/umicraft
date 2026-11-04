# ADR-0045 — The title panorama pass

Status: accepted (2026-11-04)

## Context

ADR-0042 and ADR-0044 left the rotating panorama of `GuiMainMenu` to a
renderer pass: the title screen already returned a `PanoramaView` (face ids and
the camera rotation) and the window ignored it.

## Decisions

- **Six quads, no cubemap.** `umicraft-render/src/panorama/` draws a unit cube
  seen from inside as six textured quads (one bind group per face, a static
  vertex buffer, one matrix uniform). The matrix is
  `fit * perspective(120, 1, 0.05, 10) * model_view` with the model-view of
  `renderSkybox` (`rotate 180 x`, `rotate 90 z`, pitch, yaw).
- **Vanilla crop.** Vanilla renders the cube into a 256 x 256 square and
  stretches its central `120 / max(width, height)` over the screen with the
  texture coordinates transposed (screen right is capture down). `fit`
  reproduces that in one matrix, so the larger screen side shows about 117
  degrees and no pixel is stretched differently from the reference. Checked on
  screen with the user's own faces: upright scenery, tilted slightly up.
- **Draw order.** Right after the sky, writing depth 0 so the world passes that
  follow (clouds, particles, weather) fail their depth test and cannot show
  through; the UI has no depth test and draws on top. The faces are opaque.
- **Blur skipped.** The 64 sub-pixel offset overdraws at alpha `255 / (j + 1)`
  and the seven shifted blends of `rotateAndBlurSkybox` are not drawn. Bilinear
  filtering of the strongly magnified 256 px faces already softens the picture;
  an exact blur needs an offscreen 256 x 256 target and extra passes. Revisit
  if the owner finds the picture too sharp next to vanilla.
- **Renderer API.** `Renderer::upload_panorama(&[SkyImage; 6]) ->
  Option<PanoramaTextures>` (the renderer does not depend on the assets crate)
  and `Renderer::set_panorama(Option<PanoramaView>)`. A view whose ids are not
  those of the current upload draws nothing.
- **Window.** The six faces are read once at start from the verified jar
  (`panorama_faces::load_panorama_faces`; absent jar or an undecodable face
  gives `None` and a debug log, so the dirt background stays). Every UI frame
  first sets the panorama to `None`; only the title screen sets it again, so a
  session never has it. The textures are the user's download and are never
  stored (spec §3).
- **Demo.** `--hud-demo --hud-view title` uses the user's faces when
  available; `--synthetic-panorama` draws six generated gradient faces
  instead (no Mojang picture).

## Consequences

Tests: `umicraft-render` unit tests of the matrix and shader, offscreen tests
with six solid colour faces (`tests/offscreen_panorama.rs`, `--ignored`, needs
an adapter) for the centre face at three rotations, the seam, no draw without
a view and no world pass showing through. The owner still has to compare the
title screen with vanilla in a real window.
