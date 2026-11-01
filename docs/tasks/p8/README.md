# P8 — Particles & visual effects: plan and architecture

Status: **P8 is implemented except the pieces listed under "Not done"**, and nothing has been compared with vanilla on a screen yet (unit tests, ignored GPU tests and a headless build only). Decision:
`docs/decisions/0035-p8-particles.md`. Roadmap entry: `ROADMAP.md` §P8.

Rule (as in P5 to P7): a step adds a module, existing big files only get
wiring; modules stay under about 400 lines and test their logic against the MCP
rules.

## Layout

```
umicraft-game/src/particles/   kind, particle, engine, behaviours/<family>.rs
umicraft-render/src/particles/ ParticleRenderer: billboards, layers, shader
umicraft-render/src/overlays/  underwater, in-wall, fire, pumpkin, portal, vignette
umicraft-client/src/particle_view.rs  setting, packets, spawn sources, Frame
```

## Steps

| Step | Content | State |
|---|---|---|
| P8-0 | `ParticleKind`, `Particle`, `ParticleEngine`, `Behaviour`, `create` | done |
| P8-a | `ParticleRenderer`: billboards, flat footprints, `particles.png` sheet or block atlas layer, tint x light x fog | done |
| P8-b | water, drips, explosion and large explosion (simulation) | simulation done |
| P8-b2 | block break and hit fragments, `blockcrack`/`blockdust`, `Effect` 2001 | done (blocks); `iconcrack`, sprint and rain splash remain (iconcrack) |
| P8-e | `HugeExplosion` emitter, fireworks, `MobAppearance`, `ItemTake`, `Snowball`/`Slime` (breaking), full `Effect` packet | simulation done for `HugeExplosion`, `explode`, `Effect` 2000/2002-2005 and `FireworksSpark`; `FireworksSpark` `StarterFX`/overlay, `ItemTake`, `MobAppearance`, breaking (item sprites) and `iconcrack` remain; wired: `Effect` and `Explosion` packets |
| P8-c | flame, smoke, lava, portal, redstone, footstep (drips: with P8-b water) | simulation done |
| P8-c2 | ambient block particles (`randomDisplayTick`), 1000-block sampling, rain splashes, sprint dust (`particles::ambient`) | done, wired in `particle_ambient` |
| P8-d | spells and potions, crit, hearts, notes, enchantment | simulation done |
| P8-f | screen overlays: underwater, in a wall, fire, pumpkin, portal, vignette | done (see below) |
| P8-g | `particles` setting, criterion benchmark, parity sheet | done: option in the video screen, benchmark in `docs/perf-log.md`, rows in `docs/parity.md` |

## Ambient particles (P8-c2)

`particles::ambient` queues `SpawnRequest`s (it never touches the engine): the
display tick of 17 block families, `sample_around` (1000 blocks within 16 of the
player, barrier markers in creative), `rain_splashes` and `running_dust`. The
caller owns the `JavaRandom`, feeds the requests to `ParticleEngine::spawn` and
supplies a `Weather` (rain at a block, rain-bearing biome). Sounds are not played
and the draws that only pick them are not made. Not done: the sound-bearing
branches, stairs (their model blocks never emit), biome-dependent leaf and rain
checks beyond what `Weather` answers.

## Approximations

- Block fragments are tinted with the plains colours (leaves, plants), not the
  biome at the block: the game thread has no biome tint table. Packet-made
  fragments (`blockcrack`, `blockdust`) use white like `getRenderColor` would for
  untinted blocks.
- Opaque particles do not write depth (see `umicraft_render::particles`).
- Item sprites (`iconcrack`), `footprint.png` and the large explosion sheet are
  not drawn; their particles are simulated but skipped by `particle_view`.
- `Effect` 2002 (potion splash) passes no potion colour yet (the item tables of
  `ItemPotion.getColorFromDamage` are not ported), so its spell sparks are black.
- The ambient rain splashes assume fancy graphics (the video options live on
  the render thread). `Weather` approximates the biome with the climate table
  (precipitation and a temperature of at least 0.15).

## Screen overlays (P8-f)

- `umicraft_render::overlays` draws a `ScreenOverlays` (plain data) in one pass
  after the hand and before the HUD: no depth, alpha blending, and the vignette
  multiplies the screen. Sprites of walls, fire and portal are layers of the
  block atlas; `underwater.png`, `pumpkinblur.png` and `vignette.png` come from
  the user's jar through `SkyTextures`, with generated fallbacks.
- The decisions are pure functions: `umicraft-client/src/overlay_state.rs` (game
  thread: block in the way, portal timer, burning flag, pumpkin, border warning)
  and `overlay_view.rs` (render thread: gating by perspective and gamemode,
  interpolation, vignette easing, sprite layers).
- Approximations: the burning flag comes from the metadata of the local entity
  (flag 0), the portal and in-wall tests read the client world, and blocks count
  as visually opaque by `BlockState::is_opaque` (leaves, glass and ice do not).
  Fire and the portal veil need the sprite in the block atlas; the procedural
  terrain has none, so they are skipped there. Sleeping and `hideGUI` (F1, not
  implemented) do not hide the overlays.

## Not done

- Item sprites: `iconcrack`, `Snowball`, `Slime` and the 8 flakes of `Effect`
  2002/2003 (needs an item variant of `Texture` and the item layers).
- `footprint.png` and the large-explosion sheet (their particles are
  simulated but not drawn).
- `ItemTake`, `MobAppearance`, the firework shell and flash.
- Potion swirls from entity metadata and critical-hit stars from `Animation`
  packets (the particles exist; no code reads the metadata or animation to
  spawn them yet).
- Comparison with vanilla on a screen (checklist in `docs/parity.md`).
