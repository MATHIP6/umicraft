# P8 — Particles & visual effects: plan and architecture

Status: **P8-0 done** (core in `umicraft-game/src/particles`). Decision:
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
| P8-a | `ParticleRenderer`: billboards, flat footprints, `particles.png` sheet or block atlas layer, tint x light x fog (render side done; client wiring next) | render done |
| P8-b | water, drips, explosion and large explosion (simulation) | simulation done |
| P8-b2 | digging, block break, `blockcrack`/`blockdust`/`iconcrack`, sprint, rain splash (need client sprites) | |
| P8-e | `HugeExplosion` emitter, fireworks, `MobAppearance`, `ItemTake`, `Snowball`/`Slime` (breaking), full `Effect` packet | |
| P8-c | flame, smoke, lava, portal, redstone, footstep (drips: with P8-b water) | simulation done |
| P8-d | spells and potions, crit, hearts, notes, enchantment | simulation done |
| P8-f | screen overlays | |
| P8-g | `particles` setting, criterion benchmark, parity sheet | |
