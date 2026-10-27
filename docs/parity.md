# Parity register

This register tracks, feature by feature, how close Umicraft is to the vanilla
Minecraft 1.8.8 client. It implements requirement PAR-5 of `UMICRAFT_SPEC.md`
§6.14 and the parity goal of [`ROADMAP.md`](../ROADMAP.md) §1. The feature list is
the parity inventory of [`roadmap-review.md`](roadmap-review.md) §4; the owner
milestone is the one in `ROADMAP.md` that is responsible for the feature.

## States

| State | Meaning |
|---|---|
| Matches vanilla | Verified equal to the vanilla 1.8.8 client with evidence (see below). |
| Partial | Implemented in part, or implemented with known differences. |
| Missing | Not implemented. |
| Unverified | Behaviour unknown; nobody has checked it against vanilla. |
| Approved deviation | Differs from vanilla on purpose; approved by the project owner (see "Approved deviations"). |

"Partial — not verified against vanilla" marks features the review judged done
that have not yet been compared with vanilla.

## How rows are updated

- A row moves to **Matches vanilla** only with harness evidence: a packet-trace
  diff, a trajectory diff, a screenshot diff or an owner checklist. The
  evidence is linked in the **Evidence** column (a path under `docs/` or a
  harness script and run id). Screenshot references contain Mojang assets and
  are kept outside the repository (spec §3); the register only names them.
- The owner milestone updates its rows in the same change that finishes the
  work.
- Milestone P13 requires every row to be **Matches vanilla** or an **Approved
  deviation**.
- Nothing is "Matches vanilla" yet: the harness (P0) is new.

## Harness tolerances

Trajectory and screenshot tolerances are defined in
[`decisions/0019-parity-harness.md`](decisions/0019-parity-harness.md)
(proposed, pending owner approval). The procedure to record and compare a
session is [`parity-harness.md`](parity-harness.md).


## Connection

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Connection | status ping, offline login, compression, keep-alive, disconnect reason | Partial — not verified against vanilla | P1 | `docs/fuzzing.md`, `umicraft-protocol/tests/roundtrip.rs` | Fuzzed (10 min per target) and property-tested in P1; compression switched in play state too (0x46). |
| Connection | online-mode login (Microsoft auth, session join, encryption) | Missing | P12 | — |  |
| Connection | brand + client settings | Partial — not verified against vanilla | P1 | — | Skin-layer bits move to P10. |
| Connection | plugin channels (MC\|Brand in, REGISTER, BungeeCord) | Missing | P12 | — |  |
| Connection | proxy server switch (dimension-swap respawn trick) | Unverified | P12 | — |  |
| Connection | server resource packs | Missing | P12 | — | P1 declines every offer, as vanilla with server packs disabled (ADR-0020). |

## Movement

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Movement | walk/sprint/sneak/jump/swim/ladder/web/soul sand/ice/slime | Partial | P2 | Vanilla vs Umicraft movement run, 2026-10-26 ([parity-harness.md](parity-harness.md#vanilla-comparison-2026-10-26)) | Ground speeds of walk (0.2159), sneak (0.0648) and sprint (0.2806 block/tick) match vanilla on the live server; swim/ladder/web/soul sand/ice/slime not compared yet. |
| Movement | sneak edge clamp | Partial | P2 | — | Implemented in `umicraft_game::collision` (the review missed it); its Z axis cancelled every sneaking Z move until 2026-10-26 (fixed, regression test). Edge behaviour not yet compared with vanilla. |
| Movement | hunger sprint rule, effects (Speed/Slowness/Jump), Depth Strider | Missing | P2 | — |  |
| Movement | flying, creative, spectator | Missing | P2 | — |  |
| Movement | knockback (velocity, explosion) | Missing | P2 | — |  |
| Movement | riding (boat, minecart, horse, pig) | Missing | P2 | — | Objects and vehicles rendering in P5. |
| Movement | serverbound traffic identical to vanilla (0x03, 0x0B, no 0x0C) | Partial | P1 | P1 run, 2026-10-26 ([parity-harness.md](parity-harness.md#p1-run-2026-10-26)) | Implemented in `umicraft_game::traffic`: 0x03 on idle ticks, 0x0B sprint/sneak transitions before the movement packet, 0x0C only while riding, silence until the player's column loads. The live run had the player blocked by terrain, so the moving and sprint ticks still have to be compared from a clear spot. |

## Interaction

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Interaction | hold-to-break with vanilla progress, cracks, particles | Missing | P2 | — | Today a fixed 12-tick break; particles in P8. |
| Interaction | place blocks | Partial — not verified against vanilla | P2 | — |  |
| Interaction | use items (food, potions, bow, sword block, rod, throwables, buckets) | Missing | P2 | — | Combat items finish in P6. |
| Interaction | pick block, drop item, swap hotbar | Partial | P2 | — | Only hotbar swap exists. |
| Interaction | signs, books, beds | Missing | P4 | — | Sign and book editing in P4; bed (`Use Bed`) in P1/P5. |

## Combat

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Combat | attack entity (swing + Use Entity), entity ray-cast, reach | Missing | P6 | — |  |
| Combat | sprint reset on hit (vanilla `motion *= 0.6`, sprint off) | Missing | P6 | — |  |
| Combat | hurt camera tilt, red hurt tint, death animation | Missing | P6 | — | Hurt tint and death fall on entities in P5. |
| Combat | crit/sharpness particles, bow charge FOV | Missing | P6 | — | Particles need P8. |
| Combat | death screen, respawn, combat event message | Missing | P2 | — | Combat-event message in P6. |

## Items

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Items | item registry (names, max stack, max damage) | Missing | P3 | — |  |
| Items | item models and icons in GUI, hand, ground, frames | Missing | P3 | — | Ground items and held items on entities in P5. |
| Items | enchantment glint, tooltips, durability bar | Missing | P3 | — | Tooltips layout finishes in P4. |

## Inventory

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Inventory | window protocol decode | Partial — not verified against vanilla | P4 | — |  |
| Inventory | slot interaction (all click modes) | Missing | P4 | — | `Click Window` never sent today. |
| Inventory | container-specific screens | Missing | P4 | — |  |
| Inventory | creative inventory | Missing | P4 | — |  |

## Entities

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Entities | spawn/move/destroy/interpolation | Partial | P5 | — | Teleport snaps instead of interpolating over 3 ticks. |
| Entities | mob models | Partial — not verified against vanilla | P5 | — | Not verified visually. |
| Entities | animations (walk, body yaw, swing, hurt, death, sneak, item use) | Partial | P5 | — | Partly missing. |
| Entities | typed metadata and variants | Missing | P5 | — |  |
| Entities | player skins, capes, layers | Missing | P5 | — | `fetch_mojang_skin` implemented but unused. |
| Entities | armour, held items, name tags, shadows, fire | Missing | P5 | — |  |
| Entities | objects (projectiles, vehicles, orbs, paintings, frames, stands) | Missing | P5 | — |  |
| Entities | entity lighting | Missing | P5 | — | Fullbright today. |

## World render

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| World render | block models and textures | Partial — not verified against vanilla | P7 | — |  |
| World render | neighbour-dependent shapes (fences, panes, walls, stairs, doors…) | Missing | P7 | — |  |
| World render | block entities | Missing | P7 | — |  |
| World render | animated textures | Missing | P7 | — |  |
| World render | smooth lighting, AO, transparency, biome tint, fluids | Partial — not verified against vanilla | P7 | — | Frame time never measured (M6 criterion). |
| World render | vanilla sky/sun/moon/clouds/stars textures | Missing | P7 | — | Procedural sky today. |
| World render | fog parity (water/lava/blindness) | Unverified | P7 | — |  |
| World render | weather, lightning, world border | Missing | P7 | — |  |
| World render | block outline, first-person hand, view bobbing, F5 | Missing | P2 | — |  |

## Effects

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Effects | particles (packet + client-side) | Missing | P8 | — |  |

## HUD

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| HUD | hotbar, hearts, food, armour, XP with vanilla textures | Partial | P9 | — | Coloured-rectangle placeholders. |
| HUD | air, absorption, effect hearts, mount bar | Missing | P9 | — |  |
| HUD | chat with colours/styles/events | Partial | P9 | — |  |
| HUD | tab list, scoreboard, teams, titles, action bar, boss health | Missing | P9 | — |  |
| HUD | vanilla F3 debug screen | Missing | P9 | — | See approved-deviation candidate for F2. |

## Screens

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Screens | pause, death, disconnect, options, main menu, server list | Missing | P10 | — | Death screen logic lands in P2. |
| Screens | controls rebinding, persisted options | Missing | P10 | — |  |

## Audio

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Audio | sound engine, sound packets, music | Missing | P11 | — |  |

## Resources

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Resources | resource packs (stack, reload) | Partial — not verified against vanilla | P7 | — | Animated textures in P7. |
| Resources | language files | Missing | P9 | — | Downloaded files unused (AST-8). |

## Approved deviations

| Deviation | Status |
|---|---|
| F2 opens the Umicraft debug screen instead of taking a screenshot (ROADMAP.md §8.2). Vanilla F3 is reproduced in P9; the Umicraft debug data moves to another key. | Proposed, awaiting owner approval |
