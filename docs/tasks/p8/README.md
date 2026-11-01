# P8 — Particles & visual effects: plan and architecture

Status: **P8-0, P8-a and the `Particle` packet path are done**: a `Particle` packet now shows its particle for every kind drawn from `particles.png`. Block and item sprites, `footprint.png` and the explosion sheet are not drawn yet (`particle_view::sprite` returns `None` for them). Decision:
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
| P8-a | `ParticleRenderer`: billboards, flat footprints, `particles.png` sheet or block atlas layer, tint x light x fog | done: packet to screen for sheet particles |
| P8-b | water, drips, explosion and large explosion (simulation) | simulation done |
| P8-b2 | block break and hit fragments, `blockcrack`/`blockdust`, `Effect` 2001 | done (blocks); `iconcrack`, sprint and rain splash remain |
| P8-e | `HugeExplosion` emitter, fireworks, `MobAppearance`, `ItemTake`, `Snowball`/`Slime` (breaking), full `Effect` packet | simulation done for `HugeExplosion`, `explode`, `Effect` 2000/2002-2005 and `FireworksSpark`; `FireworksSpark` `StarterFX`/overlay, `ItemTake`, `MobAppearance`, breaking (item sprites) and `iconcrack` remain; wiring in the client pending |
| P8-c | flame, smoke, lava, portal, redstone, footstep (drips: with P8-b water) | simulation done |
| P8-c2 | ambient block particles (`randomDisplayTick`), 1000-block sampling, rain splashes, sprint dust (`particles::ambient`) | simulation done, not wired |
| P8-d | spells and potions, crit, hearts, notes, enchantment | simulation done |
| P8-f | screen overlays | |
| P8-g | `particles` setting, criterion benchmark, parity sheet | |

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
