# P8 — Particles & visual effects: plan and architecture

Status: **P8 is code complete (2026-11-26)**; what is left is the comparison with vanilla on a screen (unit tests, ignored GPU tests and a headless build only). Decision:
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
| P8-b2 | block break and hit fragments, `blockcrack`/`blockdust`, `Effect` 2001, `iconcrack` | done |
| P8-e | `HugeExplosion` emitter, fireworks, `MobAppearance`, `ItemTake`, `Snowball`/`Slime` (breaking), full `Effect` packet | done: the shell and flash of a firework (`firework_show`), item-sprite pieces, the large explosion and the footprint with their own pictures; `MobAppearance` is the entity of `scene/entity/appearance.rs`; `ItemTake` is the P5 pickup flight |
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

## Own pictures and fireworks (P8 close-out)

- `footprint.png` and `entity/explosion.png` are two more textures of the
  particle pass (`ParticleSheet`, bind group 1 bindings 2 and 3, negative vertex
  layers -2 and -3). The client resolves `Texture::Sprite` names in
  `scene::particles::named_sprite`: the footprint is a flat quad (half side
  0.125, opacity from `EntityFootStepFX`), the large explosion walks the 4 x 4
  frames of `large_explode_quad` with `lightmap(0, 240)`. Missing pictures fall
  back to original generated ones (spec section 3).
- `particles::firework_show` ports `EntityFirework.StarterFX`: the engine keeps
  the shells (`launch_firework`), every second tick a shell throws the sparks of
  the next explosion (ball, large ball, star, creeper, burst) and a flash
  (`Behaviour::FireworkFlash`, `Texture::Region` of `particles.png`). The client
  launches one on `Entity Status` 17 of a rocket, reading `Fireworks` from the
  rocket item (watcher 8, now stored on the entity). The blast and twinkle
  sounds are `SoundCue`s drained by the client every tick.
- Deviations: the sounds have no `distanceDelay` (the cue has no delay field),
  the far/near variant uses the viewer at launch, and the shell's random draws
  come from the engine generator rather than its own.

## Not done

- Comparison with vanilla on a screen (checklist in `docs/parity.md`).
