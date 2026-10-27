# Roadmap review — vanilla 1.8 parity (2026-10-26)

> **Outcome (2026-10-26).** Accepted by the owner; the roadmap now lives in
> [`ROADMAP.md`](../ROADMAP.md). Corrections since this review: M7 is confirmed
> working by the owner (Done); the parity reference is the MCP 1.8.8 client
> source in `mcp/` (open question 1); the outer spec copy was deleted.
>
> **Scope.** A review of the roadmap in `UMICRAFT_SPEC.md` §12 against the
> project goal (*a client that behaves as closely as possible to the vanilla
> 1.8 client*) and against the code on `main` at `27838a4` ("Implement M9
> inventory & container interfaces"). It ends with a revised, prioritised
> roadmap. It does not change the spec; once the owner accepts the proposal,
> §6 and §12 are updated from it.

---

## 0. Method, sources and limits

**What was examined**

- `UMICRAFT_SPEC.md` (the repository copy), `README.md`, `docs/backlog.md`,
  `docs/perf-log.md`, ADR-0001 to ADR-0016, `docs/tasks/m4..m8`.
- The source of every crate of the workspace (12 crates, ~76 k lines),
  focusing on what is *wired end to end*: packet decoded → session event →
  game state → render/UI, and input → game → serverbound packet.
- The local MCP 1.8 reference (allowed by ADR-0001) to check two suspected
  defects (limb swing, entity teleport).
- `cargo test --workspace` on `27838a4`: **all tests pass**, 12 GPU tests are
  `#[ignore]`d (opt-in). The gaps below are therefore *functional* gaps that
  the test suite does not cover, not failing tests.

**What could not be verified from the files** (marked *unverified* below)

- No live server session was run and the windowed client was not launched.
  Every visual statement ("looks right", "exact MCP geometry") made in commit
  messages and task cards is unverified here.
- The M1 (10 min) and M3 (30 min) live-session acceptance runs: no log or
  report of them is in the repository; `runtime/m1-acceptance.umcapt` exists
  outside the repository but was not replayed.
- Claims of the M7 report (734 objects downloaded and verified, two resource
  packs tested) are taken from `docs/tasks/m7/README.md` only.
- Whether the client is kicked by common anti-cheats (the packet-level
  divergences in §3.1 make it likely, but it was not observed).

**Housekeeping found while reviewing**

- There are **two diverging copies of the spec**: `mc-project/UMICRAFT_SPEC.md`
  (outside the repository) and `umicraft/UMICRAFT_SPEC.md`. They differ in §13
  (decisions 2 and 3 are marked decided only in the repository copy). The
  repository copy should be the only one.
- `pull_requests/umicraft` is a second clone at `ab98838` (before M9).
- ADR numbers 0014 and 0015 are each used twice
  (`0014-m7-…`/`0014-movement-media`, `0015-fluid-rendering`/`0015-m8-…`).
- `docs/tasks/m7/README.md` is in French; the rest of `docs/` is in English.

---

## 1. Executive summary

1. **The roadmap is organised around the original vision (performance,
   modding, multi-account), not around parity.** Entire areas that a 1.8
   player uses every minute have no milestone: **combat and knockback,
   items (models, use, durability, enchantments), potion effects, death and
   respawn, gamemodes and flying, block entities (chests, signs, skulls,
   banners), particles, weather, the vanilla HUD textures, the pause and
   death screens, block-break progress and cracks, the first-person hand.**
   GAM-4 (attack/interact/item use, P1) was dropped when the old M7 was split
   and is assigned to no milestone today.
2. **Milestones marked complete are not complete.** M8 is declared
   "Completed" on the strength of a 5-entity synthetic demo; on a live
   server, entities have no hurt/death/swing animations, a wrong limb-swing
   formula, no body-yaw logic, fullbright lighting, no armour, no held item,
   no real skins and no metadata-driven variants. M9 renders the windows but
   **never sends `Click Window`**, so items cannot be moved — its own
   acceptance criterion is not met. M3 and M5 also have significant gaps (see
   §3).
3. **The client does not talk to the server like a vanilla client.** It never
   sends `Entity Action` (sprint/sneak), `Use Entity` (attack),
   `Animation` (arm swing), `Client Status` (respawn) or the per-tick
   `Player` (0x03) packet, and it sends `Steer Vehicle` (0x0C) every tick
   although vanilla only sends it while riding. 33 clientbound packet ids are
   not decoded at all, and 3 decoded ones are silently dropped.
4. **Acceptance criteria are demo-based.** Most milestones are accepted on
   synthetic offline scenes (`--block-demo`, `--entity-demo`). There is no
   parity harness that compares Umicraft with the vanilla client on the same
   server (packet traces, trajectories, screenshots).
5. **Proposal:** freeze new feature milestones (accounts, performance,
   modding) and run a **parity track** first: protocol completeness and
   vanilla traffic → core player loop → items and inventory → entities →
   combat → world rendering → particles/effects → HUD/chat/social → screens
   and settings → audio → server features, with a parity harness built
   first. Online-mode login stays early because real servers need it;
   multi-account, performance and modding come after parity (§5).

---

## 2. Coherence of the current roadmap with the goal

| # | Issue | Evidence | Consequence |
|---|---|---|---|
| C1 | The goal stated by the owner (vanilla parity first) is not written anywhere in the spec. §1.1/§1.2 put performance, modding and accounts first. | Spec §1.1, §1.2, §15 | Milestones are prioritised on the wrong axis; parity work has no home. |
| C2 | No milestone for combat, knockback, items, effects, death/respawn, gamemodes, block entities, particles, weather, vanilla HUD, screens. | §12.2 M0–M16 | These features are either missing or appear as side effects of unrelated milestones. |
| C3 | GAM-4 (attack, interact, item use) has no milestone after the re-ordering of §12.1. | §6.5 vs §12.1 table ("M7 (1st..4th part)") | Combat was silently dropped. |
| C4 | RND-10 particles is P2 and AUD-2 music is P2, NET-11 server resource packs is P2. | §6.4, §6.7, §6.1 | All three are part of the vanilla experience; with parity as the goal they are P1. |
| C5 | Requirement tables still reference the *old* milestone numbers (§12.1 says so explicitly). | §12.1 last paragraph, ACC-4 "premium by M13" | Ambiguous references; two numbering schemes in one document. |
| C6 | Acceptance criteria are verified with synthetic demos rather than against a live server and the vanilla client. | `docs/tasks/m8/README.md`, `docs/perf-log.md` M8 row | Milestones are closed while live behaviour is broken (M8, M9). |
| C7 | Milestones have no "definition of parity": no reference recordings, no screenshot comparison, no packet-trace diff. | §11 lists physics conformance only | Parity cannot be measured, so "done" is subjective. |
| C8 | Proxy compatibility (NET-10) is in M16 "Hardening", but BungeeCord/Velocity server switching is required to play on most public 1.8 networks. | §12.2 M16 | Real-world parity blocked until the very end. |
| C9 | §5.1 says every feature is a module of `umicraft-core`; no crate uses `umicraft-core`. | `docs/backlog.md` "Architecture" | The architecture section describes a target, not the code. Fine if stated; today it is not. |
| C10 | §3 rule 1 and §14 rule 1 still forbid looking at decompiled code, while ADR-0001 waives it and the code cites MCP everywhere. | Spec §3 banner vs rule text; `crates/umicraft-model/src/registry.rs` header | Contradictory normative text; the legal risk is recorded only in the backlog. |
| C11 | M6 acceptance requires "frame time does not regress by more than 10 %" but no frame-time benchmark exists; meshing time was used as a proxy. | `docs/backlog.md` "Performance" | The M6 criterion was not actually measured. |
| C12 | §13 decision 2 (ECS) was decided as "typed `EntityStore`", which is not an ECS; §8.4 point 3 (mods register ECS components/systems) is now inconsistent. | ADR-0015, spec §8.4 | The modding milestone inherits an unresolved contradiction. |
| C13 | The order "accounts (M12) before performance (M14) before modding (M15)" places multi-account live switching — a non-vanilla feature — before most parity work. | §12.1 | Conflicts with "parity before new features". |

---

## 3. Re-assessment of each milestone

Status legend: **Done** — criteria met and nothing planned is missing;
**Partial** — usable but planned items are missing or broken;
**Not started**; *unverified* — cannot be decided from the files.

| Milestone | Claimed | Re-assessed | Main reasons |
|---|---|---|---|
| M0 Foundations | done | **Done (with debt)** | Workspace, CI (Linux only), `umicraft-core` exists and is tested, but no crate uses it. |
| M1 Protocol & offline login | done | **Partial** | Fuzzing criterion not met: only a random-bytes test, no `cargo fuzz`/`proptest` (backlog). 10-minute session *unverified*. |
| M2 World & first pixels | done | **Done** | Chunk/bulk/unload/block changes wired; baselines recorded. |
| M3 Movement & interaction | done | **Partial** | See §3.1 and §3.2: wrong serverbound traffic, fixed 12-tick break, no respawn, no knockback, no effects, no sneak edge-clamp, no flying. 30-minute session *unverified*. |
| M4 Interface foundation | done | **Done** | Widgets, font, headless path. |
| M5 Model system & atlas | done | **Partial** | No neighbour-dependent ("actual") states: fences, panes, walls, iron bars never connect, stairs are always straight, doors/double plants/vines/redstone use fixed keys. Block entities (chest, sign, skull, banner, ender chest…) fall back to a cube. Item models not loaded. Animated textures missing (AST-6). |
| M6 Lighting, sky, fog, transparency | done | **Partial** | Lighting/AO/transparency present. Sky, sun, moon and clouds are procedural (no `sun.png`, `moon_phases.png`, `clouds.png`); no weather; `Change Game State` (rain) is dropped. Frame-time criterion not measured (C11). |
| M7 First-run assets & packs | done | **Mostly done** | CLI setup only (no in-client progress UI); no resource-pack selection screen; animated textures missing; language files and sounds are downloaded but unused. Download claims *unverified*. |
| M8 Entities | **complete** | **Partial / defective** | See §3.3. |
| M9 Inventory & containers | in progress | **Partial** | See §3.4. Acceptance criterion "items can be moved" not met. |
| M10 Chat, scoreboard, settings, keybinds | — | **Not started** (chat is basic) | Chat is plain text with a hard-coded translation table; no § colour codes, no tab-complete, no tab list, no scoreboard, no titles, no settings, no rebinding. |
| M11 Audio | — | **Not started** | No audio crate, no sound packets decoded. |
| M12 Accounts & main menu | — | **Not started** | No `umicraft-auth`, no main menu, connection is via CLI arguments. |
| M13 Premium auth | — | **Not started** | Encryption primitives exist in `umicraft-protocol`; no auth flow. |
| M14–M16 | — | **Not started** | Backlog lists the missing frustum culling, mesh priority queue, frame benchmark. |

### 3.1 Protocol: packets not handled

**Clientbound, not decoded (skipped silently by `PlaySession::react`)** —
`crates/umicraft-net/src/play/session.rs:419`:

| Id | Packet | Needed for |
|---|---|---|
| 0x0A | Use Bed | sleeping pose |
| 0x0D | Collect Item | pickup animation |
| 0x10 | Spawn Painting | paintings |
| 0x11 | Spawn Experience Orb | XP orbs |
| 0x1D / 0x1E | Entity Effect / Remove Entity Effect | Speed, Slowness, Jump Boost, Haste, Fatigue, Blindness, Night Vision, inventory effect list |
| 0x20 | Entity Properties | movement speed attribute (sprint/slowness), horse jump |
| 0x27 | Explosion | block removal + **explosion knockback** |
| 0x28 | Effect | block break particles/sounds, doors, records |
| 0x29 | Sound Effect | nearly all 1.8 sounds |
| 0x2A | Particle | server particles |
| 0x2C | Spawn Global Entity | lightning |
| 0x33 / 0x35 / 0x36 | Update Sign / Update Block Entity / Sign Editor Open | sign text, skulls, banners, spawners, flower pots |
| 0x34 | Maps | maps in item frames / hand |
| 0x37 | Statistics | statistics screen |
| 0x38 | Player List Item | tab list, **player skins**, gamemode of others, ping |
| 0x39 | Player Abilities | **flying**, fly speed, FOV walk speed, invulnerable |
| 0x3A | Tab-Complete | chat completion |
| 0x3B–0x3E | Scoreboard Objective / Update Score / Display Scoreboard / Teams | sidebar, below-name, team prefixes and colours, name-tag visibility |
| 0x3F | Plugin Message | `MC|Brand`, `MC|BOpen`, `MC|TrList` (villager trades), BungeeCord |
| 0x41 | Server Difficulty | options screen |
| 0x42 | Combat Event | death message / death screen |
| 0x43 | Camera | spectator camera |
| 0x44 | World Border | border render, warning vignette |
| 0x45 | Title | titles, subtitles, **action bar** |
| 0x46 | Set Compression (play) | some proxies |
| 0x47 | Player List Header/Footer | tab list |
| 0x48 | Resource Pack Send | server resource packs |
| 0x49 | Update Entity NBT | (rare) |

**Clientbound, decoded but dropped by the session:** 0x24 Block Action
(chest lid, note block, piston), 0x25 Block Break Animation (other players'
cracks), 0x2B Change Game State (rain start/stop, gamemode change, demo,
credits, arrow hit sound, fade).

**Clientbound, forwarded but ignored by the game thread**
(`crates/umicraft-client/src/game.rs:1045`): 0x1A Entity Status (hurt, death,
eating and the other 1.8 statuses), 0x0B Animation (swing, damage, crit,
magic crit, leave bed), 0x1B Attach Entity (riding, leashes).
0x12 Entity Velocity is applied to *other* entities' records only
(`game.rs:1030`): **the local player never receives knockback**, and the
stored velocity of other entities is never used.

**Serverbound, missing or never sent**

| Id | Packet | State in code | Consequence |
|---|---|---|---|
| 0x02 | Use Entity | encoded, never built | **no attack, no interact** |
| 0x03 | Player (on ground only) | not encoded | vanilla sends it every tick with no movement/look change; the server's per-tick view of the player differs |
| 0x0A | Animation (swing) | encoded, never built | other players never see the arm swing; attack order differs from vanilla |
| 0x0B | Entity Action | encoded, never built | **the server never learns that the player sprints or sneaks** (sprint knockback, sneak name-tag, sneak on edges for others) |
| 0x0C | Steer Vehicle | **sent every tick** (`game.rs:1445`) | vanilla only sends it while riding; non-vanilla traffic, anti-cheat risk |
| 0x0E | Click Window | encoded, `ClientCommand::ClickWindow` exists, never built | inventory items cannot be moved (M9) |
| 0x10 | Creative Inventory Action | encoded, never built | creative mode inventory |
| 0x11 | Enchant Item | missing | enchanting table |
| 0x12 | Update Sign | missing | writing signs |
| 0x13 | Player Abilities | missing | toggling flight |
| 0x14 | Tab-Complete | missing | chat completion |
| 0x16 | Client Status | missing | **respawn** (backlog: 1 774 resyncs in 90 s while dead), statistics, inventory achievement |
| 0x18 | Spectate | missing | spectator teleport |
| 0x19 | Resource Pack Status | missing | servers that require a pack |

### 3.2 M3 / core player loop: defects

- **Block breaking is a fixed 12-tick click** (`BREAK_TICKS`,
  `crates/umicraft-client/src/game.rs:80`), not hold-to-break with the
  vanilla progress (hardness, tool, Efficiency, Haste/Mining Fatigue,
  in water, not on ground). Instant break in creative is handled; obsidian,
  tools and speed differences are not. No crack overlay, no arm swing, no
  break particles.
- **No death screen and no respawn** (backlog "Respawn after death").
- **No local knockback** (velocity and explosion), §3.1.
- **No potion effects in movement** (Speed, Slowness, Jump Boost; Depth
  Strider) — backlog.
- **No sneak edge clamp** (vanilla prevents walking off a block edge while
  sneaking); not found in `crates/umicraft-game/src/physics.rs` or
  `collision.rs`.
- **No flying / creative / spectator movement**; `Player Abilities` is not
  decoded.
- **No hunger rule for sprinting** (vanilla forbids sprint at food ≤ 6) — not
  found in `physics.rs`.
- **No item use**: right-click only places blocks (no eating/drinking, bow,
  sword blocking, throwing, buckets, flint and steel, with the vanilla
  movement slowdown while using an item).
- **No pick-block (middle click), no drop (Q), no F5 perspective, no
  first-person hand, no view bobbing, no FOV modifiers** (sprint, Speed,
  bow); the FOV is a constant 70° (`crates/umicraft-render/src/camera.rs:8`).
- **No block selection outline.**
- Interaction logic lives in a 27-field private `Game` in the client, with
  `start_break`, `place_block`, `finish_break`, `acknowledge_block`,
  `apply_placement`, `apply_correction` untested (backlog "Interaction
  rules"). This blocks combat and item use.
- Armour points are computed from `Entity Equipment` sent *for the local
  player* (`game.rs:913`), which a vanilla server does not send for the
  player itself; vanilla reads the armour slots 5–8 of the inventory.
  ADR-0016 says `held_item`/`equipment` were replaced by `WindowManager`,
  but both fields are still in `Game` — the ADR and the code disagree.

### 3.3 M8 entities: defects (why the animations look broken)

Confirmed in code (file references in `crates/umicraft-game/src/entity/store.rs`
and `crates/umicraft-client/src/entity_textures.rs`):

1. **Limb-swing formula wrong.** The store does
   `limb_swing += speed * 4.0` with the raw, unclamped speed, *before*
   smoothing. Vanilla adds the smoothed, clamped amount:
   `limbSwingAmount += (min(speed*4,1) - limbSwingAmount) * 0.4;
   limbSwing += limbSwingAmount` (MCP `EntityLivingBase.java:1726-1737`).
   Legs swing too fast and jerk when speed changes.
2. **No body-yaw logic.** The body is drawn at the packet yaw and the head
   at `head_yaw - yaw`. Vanilla computes `renderYawOffset` client-side (body
   turns towards the movement direction and follows the head within 75°,
   `EntityLivingBase.java:1829-1872`). Heads and bodies are misaligned,
   bodies snap.
3. **Teleports snap.** `EntityStore::teleport` sets the position directly;
   vanilla interpolates a teleport over 3 ticks like a relative move
   (`NetHandlerPlayClient.handleEntityTeleport`, `setPositionAndRotation2(…, 3, true)`).
4. **No hurt / death / swing / crit animations**: `Entity Status` and
   `Animation` are ignored (no red hurt tint, no 20-tick death fall, no arm
   swing, no crit particles).
5. **Entities are fullbright**: every vertex gets `light: 0x00FF_00FF`
   (`entity_textures.rs:724`), so mobs glow in caves and at night.
6. **Metadata reduced to two bytes.** Only index 0 (flags) and 10 (skin
   parts) are read. Baby mobs, sheep colour/sheared, slime size, creeper
   fuse/charged, wither skeleton, zombie villager (`EntityKind::ZombieVillager`
   is never produced by `from_mob_type`), villager profession, horse
   type/variant/armour, wolf tamed/angry/collar, custom names, armour-stand
   poses, potion-effect colour, absorption, arrows-in-body, item-in-use
   (blocking / eating / bow pose) are all ignored.
7. **No real skins in game.** `fetch_mojang_skin` exists in
   `umicraft-assets` but the client never calls it; all players are drawn
   as Steve or Alex, slim chosen from the UUID hash rather than the profile.
   This also needs `Player List Item` (0x38), which is not decoded. No capes,
   no skin-layer toggles applied.
8. **No armour layers, no held items, no name tags, no shadows, no fire
   overlay** on entities.
9. **Objects:** only `Item` (2) and `Arrow` (60) are mapped; `Item` is drawn
   as a generic cube, not the item; arrows are not drawn at all. Boats,
   minecarts, TNT, falling blocks, XP orbs, paintings, item frames, armour
   stands, snowballs, eggs, ender pearls, potions, fireballs, fishing bobber,
   ender crystals, fireworks, leash knots and lightning are missing.
10. **Projectiles and items do not move between packets**: velocity is stored
    but never integrated (vanilla simulates these entities client-side).
11. **Riding** (`Attach Entity`) is ignored: passengers are not placed on
    vehicles.
12. The M8 acceptance (`docs/tasks/m8/README.md`) was a synthetic 5-entity
    scene; "move smoothly on a live server" was not demonstrated in the
    repository.

### 3.4 M9 inventory: defects

- **`Click Window` is never sent**; no mouse handling for slots, no cursor
  item, so the acceptance criterion "items can be moved" is not met.
- `Confirm Transaction`: rejected ids are echoed (good), but there is no
  rollback of the local prediction (`game.rs:1095` "follow-up").
- Slots show the **item id as text**, not an icon; the HUD hotbar does the
  same (`crates/umicraft-render/src/hud.rs:546`). Item models
  (`models/item`, generated layers, block-as-item) are not implemented.
- Durability uses one reference constant instead of each item's maximum
  damage.
- No tooltips, enchantment glint, creative inventory screen, drag-splitting,
  shift-click, number keys, double-click collect (ADR-0016 lists them as
  modes but no code produces them).
- Container-specific UIs (furnace progress, brewing, enchanting with
  `Enchant Item`, anvil rename via `MC|ItemName`, beacon `MC|Beacon`,
  villager trading `MC|TrList`, horse inventory, book editing) are absent.
- The armour bar does not read the armour slots (see §3.2).

### 3.5 Other parity gaps not tied to an existing milestone

- **Block entities**: chest, trapped/ender chest (with lid animation via
  Block Action), sign text, skulls, banners, beacon beam, end portal,
  enchanting-table book, spawner mob, piston moving blocks.
- **Weather**: rain/snow, rain darkening of sky and light, lightning.
- **Particles**: none at all.
- **HUD**: vanilla textures (`widgets.png`, `icons.png`) unused; health is
  drawn as coloured rectangles; no air bubbles, no absorption/poison/wither
  hearts, no heart flash on damage, no mount health / jump bar, no held-item
  name, no vanilla F3 screen (F2 opens the M4 debug screen; vanilla F2 is a
  screenshot).
- **Screens**: no pause menu, death screen, disconnect screen, options,
  main menu, server list, connecting screen.
- **Chat**: no `§` legacy codes, no JSON styles, no hover/click events, no
  input history; the translation table is hard-coded instead of the
  downloaded language files (AST-8).

---

## 4. Vanilla 1.8 parity inventory

The checklist the revised roadmap is built from. **D** done, **P** partial,
**M** missing, **U** unverified.

| Area | Feature | State |
|---|---|---|
| Connection | status ping, offline login, compression, keep-alive, disconnect reason | D |
| | online-mode login (Microsoft auth, session join, encryption) | M |
| | brand + client settings | D |
| | plugin channels (MC\|Brand in, REGISTER, BungeeCord) | M |
| | proxy server switch (dimension-swap respawn trick) | U |
| | server resource packs | M |
| Movement | walk/sprint/sneak/jump/swim/ladder/web/soul sand/ice/slime | D (U on live server) |
| | sneak edge clamp | M |
| | hunger sprint rule, effects (Speed/Slowness/Jump), Depth Strider | M |
| | flying, creative, spectator | M |
| | knockback (velocity, explosion) | M |
| | riding (boat, minecart, horse, pig) | M |
| | serverbound traffic identical to vanilla (0x03, 0x0B, no 0x0C) | M |
| Interaction | hold-to-break with vanilla progress, cracks, particles | M |
| | place blocks | D |
| | use items (food, potions, bow, sword block, rod, throwables, buckets) | M |
| | pick block, drop item, swap hotbar | P (hotbar only) |
| | signs, books, beds | M |
| Combat | attack entity (swing + Use Entity), entity ray-cast, reach | M |
| | sprint reset on hit (vanilla `motion *= 0.6`, sprint off) | M |
| | hurt camera tilt, red hurt tint, death animation | M |
| | crit/sharpness particles, bow charge FOV | M |
| | death screen, respawn, combat event message | M |
| Items | item registry (names, max stack, max damage) | M |
| | item models and icons in GUI, hand, ground, frames | M |
| | enchantment glint, tooltips, durability bar | M |
| Inventory | window protocol decode | D |
| | slot interaction (all click modes) | M |
| | container-specific screens | M |
| | creative inventory | M |
| Entities | spawn/move/destroy/interpolation | P (teleport snaps) |
| | mob models | D (U visually) |
| | animations (walk, body yaw, swing, hurt, death, sneak, item use) | P / M |
| | typed metadata and variants | M |
| | player skins, capes, layers | M |
| | armour, held items, name tags, shadows, fire | M |
| | objects (projectiles, vehicles, orbs, paintings, frames, stands) | M |
| | entity lighting | M (fullbright) |
| World render | block models and textures | D |
| | neighbour-dependent shapes (fences, panes, walls, stairs, doors…) | M |
| | block entities | M |
| | animated textures | M |
| | smooth lighting, AO, transparency, biome tint, fluids | D |
| | vanilla sky/sun/moon/clouds/stars textures | M (procedural) |
| | fog parity (water/lava/blindness) | U |
| | weather, lightning, world border | M |
| | block outline, first-person hand, view bobbing, F5 | M |
| Effects | particles (packet + client-side) | M |
| HUD | hotbar, hearts, food, armour, XP with vanilla textures | P (placeholder) |
| | air, absorption, effect hearts, mount bar | M |
| | chat with colours/styles/events | P |
| | tab list, scoreboard, teams, titles, action bar, boss health | M |
| | vanilla F3 debug screen | M |
| Screens | pause, death, disconnect, options, main menu, server list | M |
| | controls rebinding, persisted options | M |
| Audio | sound engine, sound packets, music | M |
| Resources | resource packs (stack, reload) | D |
| | language files | M |

---

## 5. Revised roadmap

### 5.1 Principles

1. **Parity first.** A milestone is in the parity track if a vanilla 1.8
   player would notice its absence. Performance work (M14), modding (M15),
   multi-account live switching (ACC-2, ACC-8) and other non-vanilla
   features wait until the parity track is complete, except for fixes that
   are needed to keep the client playable.
2. **Reopen before adding.** M3, M5, M6, M8 and M9 are reopened; their
   remaining items are moved into the milestones below rather than kept as
   "done with follow-ups".
3. **Accept against vanilla, on a live server.** Every parity milestone is
   accepted with the parity harness (P0): a scripted session on a local
   Paper 1.8.8 server, compared with the same script run by the vanilla
   1.8.9 client. A synthetic demo is a development aid, not acceptance.
4. **Keep requirement IDs, renumber milestones once.** New requirement
   families are proposed in §5.4; the spec's §6 tables are updated to point
   at the new milestone ids only.

### 5.2 Order and dependencies

| Order | Milestone | Replaces / reopens | Depends on |
|---|---|---|---|
| 1 | **P0 — Parity harness & roadmap hygiene** | new | — |
| 2 | **P1 — Protocol completeness & vanilla traffic** | M1, M3 (traffic), NET-5 | P0 |
| 3 | **P2 — Game-logic extraction & core player loop** | M3 (reopened) | P1 |
| 4 | **P3 — Items: registry, models & rendering** | M5 (items), M9 (icons) | P1 |
| 5 | **P4 — Inventory & containers (finish M9)** | M9 | P2, P3 |
| 6 | **P5 — Entities parity (finish M8)** | M8 | P1, P3 |
| 7 | **P6 — Combat & PvP parity** | GAM-4 | P2, P4, P5 |
| 8 | **P7 — World rendering parity (finish M5/M6)** | M5, M6 | P1 |
| 9 | **P8 — Particles & visual effects** | RND-10 | P5, P7 |
| 10 | **P9 — HUD, chat & social** | M10 (part) | P3, P4 |
| 11 | **P10 — Screens, options & controls** | M10 (part), M12 (menus) | P9 |
| 12 | **P11 — Audio** | M11 | P1, P10 |
| 13 | **P12 — Online mode & real-server compatibility** | M13, NET-10, NET-11 | P1, P10 |
| 14 | **P13 — Parity sign-off** | new | P0–P12 |
| 15 | F1 — Accounts manager & live switching | M12 (accounts) | P12 |
| 16 | F2 — Performance pass | M14 | P13 |
| 17 | F3 — Modding API v1 | M15 | F2 |
| 18 | F4 — Hardening & v1.0 | M16 | F1–F3 |

P3 and P7 can run in parallel with P2 (different crates); P12's auth spike
can start early because it has external risk and blocks nothing in the
parity track except testing on online-mode servers.

```
P0 ─► P1 ─┬─► P2 ─────────────┬─► P4 ─┬─► P6 ─┐
          ├─► P3 ─────────────┘       │       │
          │     └────────► P5 ────────┘       │
          ├─► P7 ─────────► P8 ◄── P5          ├─► P13 ─► F1/F2/F3 ─► F4
          ├─► P9 (after P3, P4) ─► P10 ─► P11 ─┤
          └─► P12 (spike early; finish after P10)
```

### 5.3 Milestones

#### P0 — Parity harness & roadmap hygiene

- **Features**
  - A parity harness in `umicraft-testkit`: (a) packet-trace capture of the
    vanilla 1.8.9 client and of Umicraft running the same scripted session
    against a local Paper 1.8.8 server, and a diff tool that reports the
    serverbound packet sequence per tick; (b) trajectory comparison
    (already partly in `movement_probe`); (c) a reference screenshot set
    taken by the owner with the vanilla client (kept outside the repository
    — they contain Mojang assets) and a comparison script.
  - A "packet coverage" test that lists every protocol-47 id with its state
    (handled / intentionally ignored with reason).
  - Spec updates: state the parity goal in §1, add the requirement families
    of §5.4, replace §12 with this roadmap, keep a single spec copy, fix the
    duplicated ADR numbers, reconcile §3 rule 1 and §14 rule 1 with
    ADR-0001.
  - Reset statuses: README "M8 complete" → "M8 partial"; M8 task README
    status; ADR-0016 consequences vs code.
- **Known problems to fix**: C5, C6, C7, C10; two spec copies; ADR
  numbering.
- **Dependencies**: none.
- **Done when**: the harness produces a serverbound diff for a 60-second
  "walk, sneak, sprint, jump, look around" script; the coverage test exists
  (and fails, listing the gaps of §3.1); the spec reflects this roadmap.

#### P1 — Protocol completeness & vanilla traffic

- **Features**
  - Decode every clientbound id of §3.1 into typed packets and session
    events, even when the consumer comes later (store the state: abilities,
    effects, properties, player list, scoreboard, titles, world border,
    difficulty, plugin messages).
  - Forward 0x24, 0x25, 0x2B instead of dropping them.
  - Serverbound: add 0x03 Player, 0x11, 0x12, 0x13, 0x14, 0x16, 0x18, 0x19.
  - Vanilla traffic rules: send `Player` (0x03) on idle ticks; send
    `Steer Vehicle` only while riding; send `Entity Action` on sprint and
    sneak transitions (and stop-sleeping, open-inventory on horse); keep the
    vanilla per-tick order (MCP `EntityPlayerSP.onUpdate` /
    `onUpdateWalkingPlayer`).
  - `Set Compression` in play state.
  - `cargo fuzz` targets for the play decoders and the chunk parser;
    `proptest` round-trips for every packet (closes M1).
- **Known problems to fix**: §3.1 entirely; M1 fuzz criterion.
- **Dependencies**: P0.
- **Done when**: the coverage test passes with no silently skipped id; the
  P0 trace diff of the movement script shows no difference in packet ids or
  order with the vanilla client; fuzzers run 10 minutes without a panic in
  CI or locally (documented).

#### P2 — Game-logic extraction & core player loop

- **Features**
  - Move interaction rules from `umicraft-client/src/game.rs` into
    `umicraft-game` (backlog "Interaction rules"): inputs + server events in,
    packets out, unit-tested.
  - Hold-to-break with the vanilla progress formula (hardness, harvest tool,
    Efficiency, Haste, Mining Fatigue, water, airborne), `ABORT_DESTROY`,
    break cracks (local and 0x25), arm swing sent and rendered.
  - Death and respawn: death screen, `Client Status` respawn, health reset.
  - Gamemodes: survival, creative (instant break, flying with double-tap,
    `Player Abilities`), adventure, spectator (no-clip, flying, `Camera`).
  - Movement: sneak edge clamp, sprint hunger rule, effects (Speed,
    Slowness, Jump Boost) and `Entity Properties` movement speed, Depth
    Strider, **local knockback** from `Entity Velocity` and `Explosion`.
  - Item use framework: right-click hold/release (`Player Block Placement`
    with `-1` position, `RELEASE_USE_ITEM`), use-item slowdown, pick block
    (middle click), drop item (Q / Ctrl+Q).
  - Camera: first-person hand placeholder, view bobbing, FOV modifiers,
    F5 perspectives, block selection outline.
  - Riding: mount/dismount, passenger offsets, `Steer Vehicle`.
- **Known problems to fix**: §3.2; fixed 12-tick break; armour from the
  wrong source (moves to P4).
- **Dependencies**: P1.
- **Done when**: a 30-minute live survival session (walk, sprint, sneak on
  edges, break stone with/without a pickaxe, place, eat, die, respawn, fly in
  creative, ride a boat) produces no kick, no rubber-banding, and a
  serverbound trace equivalent to vanilla for the same script; the
  extracted rules have unit tests.

#### P3 — Items: registry, models & rendering

- **Features**
  - Item registry for 1.8 (id → name, max stack, max damage, block item,
    armour material, tool tier, food) with sources in `docs/sources.md`.
  - `models/item` parsing (generated layers, `builtin/entity`,
    block-as-item via the block model, display transforms for GUI, hand,
    ground, head, fixed).
  - Item rendering in the GUI (icons, count, durability per item, glint) and
    in the world (dropped items with bob/spin and stack size, held items,
    item frames).
  - Item NBT read for display name, lore, enchantments, colour (leather),
    skull owner, potion type.
- **Known problems to fix**: item ids drawn as text in hotbar and slots;
  dropped items drawn as a generic cube.
- **Dependencies**: P1 (metadata/slot data), M5 atlas (done).
- **Done when**: every 1.8 item renders with its real icon in the GUI and on
  the ground; GUI screenshot diff with vanilla below the agreed threshold
  for a reference hotbar and inventory.

#### P4 — Inventory & containers (finish M9)

- **Features**
  - Slot interaction: left/right click, shift-click, number keys, middle
    click (creative), drop (Q in GUI), drag-split (mode 5), double-click
    collect; cursor item rendering; `Click Window` with the predicted item;
    rollback on rejected `Confirm Transaction`.
  - Player inventory: armour slots, 2×2 crafting with result, armour bar
    from inventory slots, active effects list.
  - Container screens: chest (single/double), ender chest, crafting table,
    furnace (progress via Window Property), brewing stand, dispenser/dropper,
    hopper, enchanting table (`Enchant Item`, seed text), anvil
    (`MC|ItemName`), beacon (`MC|Beacon`), villager trading (`MC|TrList`,
    `MC|TrSel`), horse inventory, book and quill (`MC|BEdit`/`MC|BSign`).
  - Creative inventory (tabs, search, `Creative Inventory Action`).
  - Tooltips with the vanilla layout.
- **Known problems to fix**: §3.4; ADR-0016 vs code (`held_item`,
  `equipment` fields).
- **Dependencies**: P2, P3.
- **Done when**: on a live server every container type opens, items move
  with every click mode, the server never resends a full window because of
  a desync over a 15-minute scripted session, and the trace diff matches
  vanilla for the same clicks.

#### P5 — Entities parity (finish M8)

- **Features**
  - Fix animation maths: limb swing, body yaw (`renderYawOffset`), teleport
    interpolation, head pitch.
  - Hurt tint/time and death animation (`Entity Status` 2/3), arm swing and
    crit/magic-crit triggers (`Animation`), sleeping (`Use Bed`), eating,
    item-use poses (sword block, bow pull), sneaking pose for all humanoids.
  - Typed metadata per entity class and variants (baby, sheep colour,
    slime size, creeper fuse/charged, skeleton type, zombie villager,
    villager profession, horse type/variant/armour, wolf/ocelot tame
    state and collar, custom name, armour stand flags and pose, potion
    colour, arrows in body, absorption).
  - Entity lighting from the world light at the entity position; shadows;
    fire overlay; invisibility.
  - Players: `Player List Item` profiles, real skin and cape fetch and
    cache (`fetch_mojang_skin`), slim model from the profile, skin layers
    from metadata, name tags with team rules.
  - Armour layers and held items in third person.
  - Objects and other entities: all `Spawn Object` types, XP orbs,
    paintings, global entities (lightning), item frames with maps, armour
    stands; client-side motion for projectiles/items/falling blocks;
    `Collect Item` animation; `Attach Entity` riding and leashes.
- **Known problems to fix**: §3.3 entirely.
- **Dependencies**: P1, P3.
- **Done when**: on a live server, side-by-side recordings of a reference
  scene (players walking, sneaking, sprinting, fighting, mobs of every type,
  projectiles, vehicles, item frames) match vanilla to the eye of the owner
  on an agreed checklist; unit tests cover the swing/yaw/interpolation
  maths against MCP formulas.

#### P6 — Combat & PvP parity

- **Features**
  - Entity ray-cast with vanilla reach (3.0 survival, 5.0 creative block
    reach / 3.0 attack), hitbox expansion, priority between blocks and
    entities.
  - Attack: swing then `Use Entity` attack in vanilla order; interact and
    interact-at (armour stands).
  - Sprint reset on hit (vanilla: `motion *= 0.6`, sprint stopped when
    attacking while sprinting), knockback received (from P2), hurt camera
    tilt, damage flash on hearts.
  - Critical hits (falling, not on ground, not in water) with particles;
    sharpness particles.
  - Sword blocking, bow charge with FOV zoom and release, fishing rod,
    snowballs/eggs/pearls/splash potions, golden apples (absorption),
    regeneration/poison/wither heart styles.
  - Combat event death message, death screen (from P2).
- **Known problems to fix**: GAM-4 has no milestone (C3).
- **Dependencies**: P2, P4, P5.
- **Done when**: a scripted 1v1 on a local server (melee with sprint
  resets, bow, rod, pearls, splash potions) produces the same serverbound
  sequence as vanilla and the same knockback trajectories within the
  tolerance agreed in P0; manual PvP session on a public 1.8 network
  (after P12) without anti-cheat flags *(manual check)*.

#### P7 — World rendering parity (finish M5/M6)

- **Features**
  - Neighbour-dependent block states ("actual state"): fence, fence gate in
    walls, pane, iron bars, wall, stairs shapes, doors (combined halves),
    double plants, vines `up`, redstone wire, tripwire, snowy grass from the
    block above (remove the M5 deviation).
  - Block entities: chests (with lid animation from `Block Action`), ender
    and trapped chests, signs with text, skulls (with player skins),
    banners, beacon beam, end portal effect, enchanting book,
    spawner mob, moving pistons, flower pot contents.
  - Animated textures (AST-6) including water/lava/fire/portal.
  - Vanilla sky: `sun.png`, `moon_phases.png`, `clouds.png` (fast/fancy),
    stars as in vanilla, sunrise colours, Nether and End skies.
  - Weather: rain and snow, rain darkening, lightning, `Change Game State`.
  - World border rendering.
  - Fog parity (distance, water, lava, blindness); verify against reference
    screenshots.
  - Close the backlog item "liquid corners at a diagonal chunk boundary".
- **Known problems to fix**: §3 M5 and M6 rows; C11 (add a frame-time
  benchmark now so later work can be measured).
- **Dependencies**: P1; P3 for skull/banner/item-frame textures.
- **Done when**: the reference screenshot set (village, Nether, End,
  rain, night, redstone, fences/panes/walls, chests and signs) matches
  vanilla within the agreed threshold; a frame-time benchmark exists and is
  recorded in `docs/perf-log.md`.

#### P8 — Particles & visual effects

- **Features**: particle engine (billboards, physics, lighting),
  `Particle` packet, `Effect` packet (block break particles, smoke, potion
  splash), client-side particles (digging, sprint footsteps, rain splash,
  water/lava drips, torch/fire/portal smoke, potion swirls from metadata,
  crits, explosion), particle setting (all/decreased/minimal).
- Screen overlays: underwater, in-wall, fire, pumpkin blur, portal,
  vignette, damage tilt.
- **Dependencies**: P5, P7.
- **Done when**: every 1.8 particle type renders; reference recordings of
  break, explosion, potion, rain match vanilla on the agreed checklist.

#### P9 — HUD, chat & social

- **Features**
  - Vanilla HUD from `widgets.png`/`icons.png`: hotbar with item icons,
    hearts (all styles, absorption, flash), food (with hunger effect), armour
    from inventory, air bubbles, XP bar and level, mount health and jump
    bar, held-item name fade, vanilla crosshair, GUI scale.
  - Chat: JSON components with styles, legacy `§` codes, click and hover
    events, input with history, commands, tab completion, chat settings
    (opacity, scale, width), language files instead of the hard-coded table
    (AST-8).
  - Tab list (`Player List Item`, header/footer, ping, heads), scoreboard
    sidebar, below-name and list objectives, teams (prefix/suffix, colour,
    name-tag visibility), titles/subtitles/action bar, boss health bar for
    the Wither and Ender Dragon (1.8 derives it from the boss entity).
  - Vanilla F3 debug screen text (keep the Umicraft debug data on another
    key).
- **Dependencies**: P3, P4.
- **Done when**: HUD screenshots at GUI scales 1–4 match vanilla; chat from
  a real server (colours, hover, click) renders like vanilla; a minigame
  server's scoreboard and tab list display correctly.

#### P10 — Screens, options & controls

- **Features**: title screen, multiplayer server list (ping, add/edit/
  delete, direct connect), connecting and disconnect screens, pause menu,
  death screen, options (video: render distance, graphics, smooth lighting,
  GUI scale, brightness, FOV, view bobbing, clouds, particles, max FPS,
  vsync, mipmaps; controls with rebinding; chat; skin customisation which
  sends `Client Settings`; sounds; resource packs; language), statistics
  screen, persisted options file. Replace the CLI connection flags as the
  primary path (keep them for tests).
- **Dependencies**: P9 (font/widgets) and the M4 layer.
- **Done when**: a user can start the client, add a server, connect,
  change every vanilla option, rebind a key, disconnect and quit without
  the command line; options survive a restart.

#### P11 — Audio

- **Features**: audio crate ADR, sound index from the asset index, sound
  events from `Sound Effect`/`Effect`, client-side sounds (footsteps,
  block break/place, GUI clicks, rain, entity ambient where client-driven),
  3D positioning, volume categories, music.
- **Dependencies**: P1 (packets), P10 (volume options).
- **Done when**: the M11 acceptance plus: a reference session's sounds
  match vanilla on an agreed checklist; the audio thread never blocks the
  game or render thread (measured).

#### P12 — Online mode & real-server compatibility

- **Features**: Microsoft device-code auth → XBL → XSTS → Minecraft
  services → profile; session join; encryption handshake; token storage in
  the keychain (ACC-4/5/6/7, NET-4). BungeeCord/Velocity server switching
  (NET-10), server resource packs with prompt and SHA-1 (NET-11), plugin
  channel handshake. *Only one account is required here; the multi-account
  manager is F1.*
- **Dependencies**: P1; P10 for the login UI. The auth spike (endpoint and
  licence validation) can start at any time.
- **Done when**: a premium account joins an online-mode 1.8 server and a
  BungeeCord network, switches servers without a reconnect, accepts a
  server resource pack; tokens never appear in logs.

#### P13 — Parity sign-off

- **Features**: run the full harness (all scripts of P1–P12) against
  vanilla; fix the remaining diffs or document them as accepted deviations
  in `docs/parity.md`; long-session stability (2 h) on a real server.
- **Done when**: `docs/parity.md` lists every feature of §4 as matching or
  as an accepted, owner-approved deviation.

#### After parity (F1–F4)

The existing M12 (multi-account manager and live switching), M14
(performance), M15 (modding) and M16 (hardening, release, legal review)
keep their current content and acceptance criteria. Two adjustments:

- Performance fixes that are needed for playability (frustum culling, mesh
  priority queue, entity mesh allocation per frame) may be pulled into the
  parity track, each with a criterion benchmark (AGENTS.md rule).
- The legal review of ADR-0001-derived code (backlog "Decisions needed") is
  a **blocker of any public release**, not just of F4; schedule it as soon
  as a release date exists.

### 5.4 Requirement families to add to spec §6

| Prefix | Area | Examples |
|---|---|---|
| CMB | Combat | CMB-1 attack/swing order, CMB-2 sprint reset, CMB-3 knockback received, CMB-4 crits, CMB-5 blocking/bow/rod/projectiles |
| ITM | Items | ITM-1 registry, ITM-2 item models, ITM-3 use actions, ITM-4 NBT display (names, lore, enchantments) |
| EFX | Effects | EFX-1 status effects in movement and HUD, EFX-2 particles, EFX-3 screen overlays |
| ENT | Entities | ENT-1 animations, ENT-2 metadata variants, ENT-3 skins/capes, ENT-4 armour/held items/name tags, ENT-5 objects and vehicles |
| BLE | Block entities | BLE-1 chests, BLE-2 signs, BLE-3 skulls/banners, BLE-4 beacon/portal/spawner |
| SCR | Screens | SCR-1 pause/death/disconnect, SCR-2 options, SCR-3 server list |
| PAR | Parity verification | PAR-1 packet-trace diff, PAR-2 trajectory diff, PAR-3 screenshot diff, PAR-4 parity register |

Re-prioritise for the parity goal: RND-10 (particles), AUD-2 (music), NET-11
(server resource packs) and NET-10 (proxies) become **P1**; ACC-2 and ACC-8
(live multi-account switching) and every modding requirement stay as they
are but move after the parity track.

---

## 6. Open questions for the owner

1. **Reference client.** The server is Paper 1.8.8; should the parity
   reference be the vanilla 1.8.8 or 1.8.9 client? (They differ only in
   minor fixes, but the trace harness needs one.)
2. **Tolerances.** What thresholds count as "matching" for trajectories
   (e.g. ≤ 1e-6 blocks per tick) and screenshots (pixel diff / SSIM)?
3. **Accepted deviations.** Are any vanilla behaviours intentionally *not*
   reproduced (e.g. F2 as debug screen instead of screenshot, the
   `grass`/`mycelium` metadata deviation of M5)?
4. **Online mode timing.** Should P12's auth spike start now in parallel,
   given that most public 1.8 PvP servers are online-mode and P6's manual
   check depends on it?
5. **ADR-0001.** Is the MCP-derived code acceptable for the foreseeable
   future, or should a clean-room re-derivation be planned before more
   derived tables (item registry, metadata indices) are added in P3/P5?
