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
| P8-a | `ParticleRenderer`: billboards, 3 layers, `particles.png`, tint x light x fog | |
| P8-b | digging, block break, `blockcrack`/`blockdust`/`iconcrack`, sprint, rain splash | |
| P8-c | flame, smoke, lava, portal, redstone, footstep (drips: later) | simulation done |
| P8-d | spells and potions, crit, hearts, notes, enchantment | |
| P8-e | explosions, fireworks, full `Effect` packet | |
| P8-f | screen overlays | |
| P8-g | `particles` setting, criterion benchmark, parity sheet | |
