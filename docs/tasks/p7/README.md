# P7 — World rendering: plan and architecture

Status: **P7-0 to P7g are done** (unit tests and ignored GPU tests; partly
checked in game, see below). P7h remains. The decision record is `docs/decisions/0030-p7-actual-state.md`;
MCP sources are in `docs/sources.md` (rows dated 2026-10-31). The roadmap entry
is `ROADMAP.md` §P7.

Rule (as in P5 and P6): a step adds a module, existing big files only get
wiring; modules stay under about 400 lines and test their logic against the
MCP rules.

## Where we stand (end of the 2026-10-31 session)

### What was done

- **P7-0** split `umicraft-mesh/src/lib.rs` (1 112 lines) into `vertex`, `mesh`,
  `face`, `source`, `legacy`, `lighting`.
- **P7a, P7b** neighbour-dependent states (`actual_state/`, one module per
  family): fences, panes, iron bars, walls, gates in walls, stairs, doors, double
  plants, vines, redstone wire, tripwire, snowy grass. `BlockModelSet::model_by_key`
  bakes every variant of every referenced blockstate. ADR-0030.
- **P7c** tile-entity plumbing (the mesher lists chests and signs, no cube),
  chests (single, double, trapped, ender) with the lid animation from
  `Block Action`, signs with text.
- **Fixes found in play**: chests, farmland, ender chests, beacons, dragon eggs
  and extended pistons no longer hide neighbouring faces (they were "opaque
  cubes"); pistons and heads got their collision boxes.
- **P7d-pistons** moving pistons: `BlockPistonStructureHelper` port, 0.5 per
  tick animation drawn smoothly, push of the local player.
- **P7d** skulls (owner skins), banners (patterns composed at runtime),
  beacon beam, end portal, enchanting-table book, spawner mob, flower-pot plants.
- **P7e** geometry sky (dome, stars from `Random(10842)`, sun and moon pictures,
  sunrise fan, End box, no Nether sky), clouds (fast and fancy), pictures read
  from the verified jar with original fallbacks, biome-tinted sky, options
  `clouds` and `fancy_graphics`. `java.util.Random` moved to `umicraft-core`.
- **P7f** rain and snow (`renderRainSnow`), rain/thunder darkening, lightning
  flash, precipitation heights and biome climate. No client ramp: 1.8 shows what
  reasons 7 and 8 send (ADR-0031).
- **P7g** world border (`forcefield.png`, status colours) and the fog (linear
  and exponential, water, lava, blindness, night vision, void, Nether, End,
  sunrise tint) behind one uniform (ADR-0032).
- **Colour** the surface and the textures are no longer sRGB, so texel x tint x
  light x fog is computed on the stored values like the 1.8 client (ADR-0033);
  the local compensations were removed (overlay shaders, tooltip, additive
  entity layers, TNT flash, GUI icon shading, lightning tint).

Everything is covered by unit tests and, where a GPU adapter exists, by ignored
offscreen tests (`offscreen_sky`, `offscreen_weather`, `offscreen_border`,
`offscreen_fog`). The owner checked in game: grass and leaf tints and the GUI
block icons (after the colour fix), pistons and the chest collision fix. The
rest has not been seen on a screen.

### What remains

**P7h (not started)**

- Frame-time benchmark (criterion), numbers in `docs/perf-log.md` (the M6
  criterion was never measured).
- Check the animated textures against vanilla (water, lava, fire, portal;
  `texture_animator`, ADR-0023), including the clock and compass.
- Backlog item of the roadmap: liquid corners on a diagonal chunk boundary.
- Final `docs/parity.md` rows and the reference screenshot set (village, Nether,
  End, rain, night, redstone, fences, panes and walls, chests and signs) compared
  with the agreed threshold. The references stay outside the repository (spec §3).

**Checks in game (nothing below has been seen on a screen)**

- Chests (lid, double chest orientation), signs (text placement and size), skulls
  (wall orientation, owner skin), banners (pattern colours, sway), beacon beam,
  end portal layers, enchanting book, spawner mob, flower-pot plants.
- Sky at noon, dusk, night, in the End and the Nether; clouds in both modes;
  rain, snow and lightning; the world border; water, lava and blindness fog.
- Hand-written tables to confirm: the redstone power-provider ids, the piston
  mobility table, the opaque/flat classification in `umicraft-world/src/block.rs`
  (a block missing there shows through its neighbours, as chests did).
- The `official_1_8_8_counts` test fails with the real jar (2546 files against
  2031 expected); it fails without these changes too and needs its constant
  reviewed.

**Accepted deviations and gaps** (also in `docs/parity.md`)

- Sign text styles and the edit marker; Christmas chests; chest and piston sounds.
- Beam: no fade-in, no glass colours, additive; end portal layers are added over
  the frame instead of replacing it; the book follows the camera only and flips
  pages with a deterministic generator; the spawner draws only the main mob layers.
- Banner sway, cloud drift and the portal scroll use the client clock, not the
  world tick.
- Rain pass fogs linearly even under water; the eye light ignores rain;
  respiration, aqua affinity and the boss-bar fog colour are not modelled.
- Moving blocks and pot plants are drawn without biome tint.
- Sky: horizon fixed at y = 63; anaglyph skipped; fast clouds write no depth.
- Pistons: no sounds, no neighbour updates, the player's prediction history is
  not amended by the push.
- Per-frame allocations remain in the spawner, the pot NBT parse and the book
  facing test (cheap, not allocation-free).

**Follow-ups outside P7**

- P8 uses the typed hooks left here: rain splashes, break and fire particles,
  screen overlays (underwater, fire, portal).
- P11 owns the sounds (chests, pistons, rain, thunder).
- P10 owns the options screen for `clouds` and `fancy_graphics`.
- `renderer.rs` (about 2 400 lines) was not split: the sky, clouds, weather,
  border and fog live in their own modules and `renderer.rs` only wires them, but
  a split of the draw order into passes would help before F2.

## Architecture

### Mesh — `umicraft-mesh/src/`

| Module | Owns |
|---|---|
| `vertex.rs`, `mesh.rs`, `face.rs` | GPU vertex, the two-pass CPU mesh, the face table |
| `source.rs` | `BlockSource`, `ChunkSnapshot` |
| `legacy.rs` | procedural cubes, cull rule, pass choice |
| `lighting.rs` | smooth light and ambient occlusion |
| `models.rs` | the model mesher (asks `actual_state` before the wire-state table) |
| `block_entity.rs` | `BlockEntityRef`: chests and signs found by the chunk scan |
| `actual_state/` | `getActualState`: one module per family, `FAMILIES` table in `mod.rs` |

### Model — `umicraft-model`

`BlockModelSet::model_by_key` (all variants of all referenced blockstate
documents are baked).

## Steps

| Step | Content | State |
|---|---|---|
| P7-0 | split `umicraft-mesh/src/lib.rs` | done |
| P7a | fences, panes, iron bars, walls, fence gates in walls, stair shapes | done |
| P7b | doors, double plants, vines, redstone wire, tripwire, snowy grass | done |
| P7c | block-entity references from the mesher, chests with the lid, signs with text | done in code, not checked in game |
| P7d | skulls, banners, beacon, end portal, enchanting book, spawner, flower pots | done in code, not checked in game |
| P7d-pistons | moving pistons: structure rules, animation, push of the player, drawing | done in code, not checked in game |
| P7e | vanilla sky: sun, moon, stars, Nether and End (geometry pass, `umicraft-render/src/sky/`) and clouds (`umicraft-render/src/clouds/`) | done in code, not checked in game |
| P7f | weather: rain, snow, darkening, lightning flash (`umicraft-render/src/weather/`) | done in code, not checked in game |
| P7g | world border (`umicraft-render/src/border/`), fog parity (`umicraft-render/src/fog/`, `fog.wgsl`) | done in code, checked offscreen, not in game |
| P7-colour | colour arithmetic on the stored values (ADR-0033): grass, leaves, GUI block icons | done, checked by the owner against vanilla |
| P7h | animated texture check, frame-time benchmark, parity rows | not started |

## Sky (P7e)

`umicraft-render/src/sky/` mirrors `RenderGlobal.renderSky` / `renderSkyEnd`:
`state` (pure `SkyState`: biome temperature, rain, thunder and lightning inputs
come from `RenderEnvironment`), `dome`, `stars`, `celestial`, `end`,
`textures` (`SkyTextures` from the user's jar, original fallbacks otherwise),
`plan` (the ordered layers of a frame as data) and `gpu`. The clear colour is
now the fog colour, as in the reference. The client loads the pictures
(`sky_textures.rs`), fills `sky_temperature` from the biome at the camera and,
since P7f, drives `rain_strength` / `thunder_strength` / `lightning_ticks`.

## Clouds (P7e)

`umicraft-render/src/clouds/` is its own pass, not part of the sky plan: the
reference draws clouds in `renderWorldPass` with the depth test, before the
translucent terrain when the eye is below y = 128 and after it above. Meshes
are static (the cloud picture's alpha selects texels, as in the reference); a
frame writes one uniform (shift, texture scroll, tint, fog). `CloudMode`
(`Off`/`Fast`/`Fancy`) comes from `VideoOptions::cloud_mode`
(`clouds`, `fancy_graphics`); the picture from `SkyTextures::clouds`. Drift is
driven by `Renderer::set_cloud_ticks` (wall clock x 20 in the client, not the
reference `cloudTickCounter`). Overworld only. No options UI yet.

## Weather (P7f)

The 1.8 client has no weather ramp: `World.updateWeather` only runs on the
server, and `WorldClient.updateWeather` is empty. The strengths are the values of
`Change Game State` 7 and 8 (reasons 1 and 2 reset the rain to 0 / 1);
`thunder = thunder * rain` (`umicraft_game::server::weather`). A lightning
flash (`lastLightningBolt`) is set to 2 on each tick a bolt entity lives and
decremented before it (`Weather::tick`).

The game thread publishes in `Frame::weather` (`umicraft-client/src/weather_view.rs`):
the strengths, the flash and, only while it rains, a 25 x 25
`PrecipitationField` (precipitation height from
`ChunkColumn::precipitation_height`, biome temperature and wet flag from
`umicraft_mesh::climate`, light at the precipitation height). The render thread
builds at most 441 quads from it each frame (`weather/geometry.rs`, same
formulas and per-column `Random` seeds as `renderRainSnow`), drawn before the
translucent terrain with the camera bind group of the terrain
(depth test, no depth write, alpha blend, fog). `rain.png` and `snow.png` come
from the jar through `SkyTextures`, with generated fallbacks. The terrain
darkening is already part of `SkyState::sun_brightness`; the sky colours, stars
and fog take the strengths from `RenderEnvironment`.

Approximations: no temperature noise term in `getFloatTemperature` (at most
0.007); the rain light is read at the precipitation height even when the camera
is above it; the camera stands for the render view entity; the lightning flash
brightens the sky colour only, not the terrain lightmap
(`EntityRenderer.updateLightmap`); the bolt is considered alive for its whole
client lifetime (`EntityLightningBolt.lightningState` is not modelled). Splash
particles belong to P8, sounds to P11.

## Known gaps

- Pane and wall connections use `BlockState::is_opaque()` as `Block.fullBlock`;
  a block the world crate classes differently from 1.8 connects differently.
- The redstone power-provider list is an id table, not per-state.
- Hook support (`doesBlockHaveSolidTopSurface`) ignores upside-down stairs and top slabs.
- Not yet checked in game against the reference screenshots.

## Block entities (P7c)

The mesher emits no quad for chests and signs; it lists them on `Mesh`
(`block_entities`, with light and same-block neighbours). `WorldView` keeps
the list per chunk. Each frame `umicraft-client/src/block_entity_render/`
meshes the ones within 64 blocks into entity vertices (`chest.rs`, `sign.rs`)
that the entity pass draws. Lid angles come from
`umicraft_game::server::chest_lids` (`Block Action`), sign lines from
`BlockEntities::sign_table()`.

## Pistons

`Block Action` for a piston (33, 29) reaches the client before the block
changes of the tick. `umicraft_game::piston::motion::start` plans the move on
that pre-move world (`structure::plan_move`, the port of
`BlockPistonStructureHelper`), clears the source cells, puts block 36 in the
destination cells and records a `MovingBlock` per moved block. `tick` advances
them by 0.5 per tick and `push::launch` shoves the local player (the physics
step runs first, then the pistons, as `World.updateEntities` does). The render
thread draws each moving block interpolated (`moving_blocks_draw`,
`umicraft_render::dynamic_blocks`). Collision boxes of the base and head are in
`piston_shape.rs`; before this the sticky piston and the head had none.

## The other block entities (P7d)

`BlockEntityKind` now also lists skulls, banners, beacons, end portals,
enchanting tables, spawners and flower pots. `replaces_model()` says whether
the renderer draws the whole block (skull, banner, portal: no model) or adds to
the model (table, spawner, beacon base, pot). Their NBT comes from
`BlockEntities::entity_table()` (an `Arc` snapshot like the signs). A renderer
can write to the `solid` or `additive` batches of `BlockEntityMeshes`; the
pot plants go through the dynamic terrain mesh like the moving blocks.

## Fog (P7g)

`umicraft-render/src/fog.rs` ports `EntityRenderer.updateFogColor` and
`setupFog`: `FogParams { color, world, sky }` with a linear or exponential
`FogMode`. The game thread publishes a `FogContext` (medium at the eye,
Blindness, Night Vision, Water Breathing, eye height, eye light) in `Frame`
(`umicraft-client/src/fog_context.rs`); the render thread eases the eye light
per tick and puts the context in `RenderEnvironment::fog`. `SkyState::fog` holds
the result (`SkyState::at_looking` adds the sunrise tint toward the sun), and
every pass reads it: the camera uniform (terrain, entities), the cloud uniform
and the sky slots. `fog.wgsl` (`fog_amount`) is prepended to those shaders.
Terrain fog is now linear as in the reference instead of a smoothstep.
