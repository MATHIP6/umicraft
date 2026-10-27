# Umicraft — Roadmap

> **Status:** accepted by the project owner on 2026-10-26. This file replaces
> the milestone plan of `UMICRAFT_SPEC.md` §12. The analysis behind it is in
> [`docs/roadmap-review.md`](docs/roadmap-review.md).

## 1. Goal

Umicraft's first goal is **parity with the vanilla Minecraft 1.8.8 client**:
every feature a 1.8 player uses must behave as it does in the vanilla client,
on the same server, before any optimisation or new feature is added. Once
parity is signed off (P13), the project moves on to the non-vanilla work:
multi-account live switching, the performance pass, modding and the 1.0
release (F1–F4).

## 2. Working principles

1. **The vanilla client is the specification.** The reference is the MCP
   1.8.8 source tree in `mcp/` (allowed by ADR-0001, gitignored). Before a
   feature is implemented, the matching MCP classes are **read**, and the
   behaviour (formulas, constants, packet order, timings) is reproduced from
   them. Every non-obvious rule cites its MCP class and method in the code
   comment and in `docs/sources.md`. Each milestone below lists the main
   classes to read; the list is a starting point, not a limit.
2. **Parity first.** A milestone belongs to the parity track if a vanilla
   player would notice its absence. Performance, modding and multi-account
   work wait for P13, except fixes needed to keep the client playable (each
   with a criterion benchmark, see `AGENTS.md`).
3. **Accept against vanilla, on a live server.** A parity milestone is
   accepted with the parity harness (P0): a scripted session on a local
   Paper 1.8.8 server, compared with the same script played by the vanilla
   1.8.8 client. Synthetic scenes (`--block-demo`, `--entity-demo`) are
   development aids, not acceptance.
4. **Reopen rather than mark done.** A milestone is done only when nothing it
   planned is missing or broken. Remaining work of an earlier milestone moves
   into the parity milestone that owns it (table §3).
5. **Small, focused commits**, tests for every feature, a regression test for
   every bug fix, docs and ADRs kept in sync (`AGENTS.md`).

## 3. Status of the original milestones

Assessed on `main` at `27838a4` (details in `docs/roadmap-review.md` §3).

| Milestone | Status | Remaining work moves to |
|---|---|---|
| M0 Foundations | Done (`umicraft-core` not used yet) | F3 |
| M1 Protocol & offline login | Partial — no fuzzing | P1 |
| M2 World data & first pixels | Done | — |
| M3 Movement & interaction | Partial — non-vanilla traffic, fixed-time breaking, no respawn, knockback, effects, flying | P1, P2 |
| M4 Interface foundation | Done | — |
| M5 Model system & atlas | Partial — no neighbour-dependent states, no block entities, no item models, no animated textures | P3, P7 |
| M6 Lighting, sky, fog, transparency | Partial — procedural sky, no weather, frame time not measured | P7 |
| M7 First-run assets & resource packs | **Done** (confirmed by the owner); language files unused, animated textures missing | P7, P9 |
| M8 Entities | Partial / defective — animations, metadata, skins, armour, held items, objects, lighting | P5 |
| M9 Inventory & containers | Partial — `Click Window` never sent, no item icons | P3, P4 |
| M10 Chat, scoreboard, settings, keybinds | Not started (basic chat only) | P9, P10 |
| M11 Audio | Not started | P11 |
| M12 Accounts & main menu | Not started | P10 (menus), F1 (accounts) |
| M13 Premium authentication | Not started | P12 |
| M14 Performance pass | Not started | F2 |
| M15 Modding API v1 | Not started | F3 |
| M16 Hardening & v1.0 | Not started | P12 (proxies), F4 |

## 4. Order and dependencies

| Order | Milestone | Depends on |
|---|---|---|
| 1 | P0 — Parity harness & roadmap hygiene | — |
| 2 | P1 — Protocol completeness & vanilla traffic | P0 |
| 3 | P2 — Game-logic extraction & core player loop | P1 |
| 4 | P3 — Items: registry, models & rendering | P1 |
| 5 | P4 — Inventory & containers | P2, P3 |
| 6 | P5 — Entities | P1, P3 |
| 7 | P6 — Combat & PvP | P2, P4, P5 |
| 8 | P7 — World rendering | P1 (P3 for skulls, banners, frames) |
| 9 | P8 — Particles & visual effects | P5, P7 |
| 10 | P9 — HUD, chat & social | P3, P4 |
| 11 | P10 — Screens, options & controls | P9 |
| 12 | P11 — Audio | P1, P10 |
| 13 | P12 — Online mode & real-server compatibility | P1, P10 |
| 14 | P13 — Parity sign-off | P0–P12 |
| 15 | F1 — Account manager & live switching | P12 |
| 16 | F2 — Performance pass | P13 |
| 17 | F3 — Modding API v1 | F2 |
| 18 | F4 — Hardening & v1.0 release | F1–F3 |

P3 and P7 can run in parallel with P2 (different crates). The P12
authentication spike carries external risk and blocks nothing in the parity
track, so it may start at any time.

```
P0 ─► P1 ─┬─► P2 ─────────────┬─► P4 ─┬─► P6 ─┐
          ├─► P3 ─────────────┘       │       │
          │     └────────► P5 ────────┘       │
          ├─► P7 ─────────► P8 ◄── P5          ├─► P13 ─► F1 / F2 ─► F3 ─► F4
          ├─► P9 (after P3, P4) ─► P10 ─► P11 ─┤
          └─► P12 (spike any time; finish after P10)
```

---

## 5. Parity track

### P0 — Parity harness & roadmap hygiene

**Features**
- Parity harness in `umicraft-testkit`:
  - packet-trace capture of the vanilla 1.8.8 client and of Umicraft playing
    the same scripted session against a local Paper 1.8.8 server, and a diff
    tool reporting the serverbound packets per tick;
  - trajectory comparison (extends `movement_probe`);
  - screenshot comparison against a reference set taken by the owner with the
    vanilla client. The reference images contain Mojang assets and stay
    **outside the repository** (spec §3).
- A packet-coverage test listing every protocol-47 id with its state
  (handled, or intentionally ignored with a reason).
- `docs/parity.md`: the parity register (one row per feature of
  `docs/roadmap-review.md` §4, with its state and any accepted deviation).
- Spec hygiene: state the parity goal in §1, add the requirement families of
  §7, point §12 to this file, reconcile §3 rule 1 and §14 rule 1 with
  ADR-0001, renumber the duplicated ADRs 0014 and 0015, correct the M8
  "complete" status in `README.md` and `docs/tasks/m8/README.md`, align
  ADR-0016 with the code (`held_item` and `equipment` still exist in
  `Game`).

**MCP to read:** `EntityPlayerSP.onUpdate` / `onUpdateWalkingPlayer`,
`NetHandlerPlayClient` (packet handling order).

**Done when:** the harness produces a serverbound diff for a 60-second
"walk, sneak, sprint, jump, look around" script; the coverage test exists
(failing on the known gaps); `docs/parity.md` exists; the spec reflects this
roadmap.

### P1 — Protocol completeness & vanilla traffic

**Features**
- Decode every missing clientbound play packet into a typed packet and a
  session event, and keep its state even when its consumer comes later:
  0x0A Use Bed, 0x0D Collect Item, 0x10 Spawn Painting, 0x11 Spawn XP Orb,
  0x1D/0x1E Entity Effect / Remove, 0x20 Entity Properties, 0x27 Explosion,
  0x28 Effect, 0x29 Sound Effect, 0x2A Particle, 0x2C Spawn Global Entity,
  0x33 Update Sign, 0x34 Maps, 0x35 Update Block Entity, 0x36 Sign Editor
  Open, 0x37 Statistics, 0x38 Player List Item, 0x39 Player Abilities,
  0x3A Tab-Complete, 0x3B–0x3E scoreboard and teams, 0x3F Plugin Message,
  0x41 Server Difficulty, 0x42 Combat Event, 0x43 Camera, 0x44 World Border,
  0x45 Title, 0x46 Set Compression, 0x47 Player List Header/Footer,
  0x48 Resource Pack Send, 0x49 Update Entity NBT.
- Forward the decoded but dropped 0x24 Block Action, 0x25 Block Break
  Animation and 0x2B Change Game State.
- Serverbound: add 0x03 Player, 0x11 Enchant Item, 0x12 Update Sign,
  0x13 Player Abilities, 0x14 Tab-Complete, 0x16 Client Status,
  0x18 Spectate, 0x19 Resource Pack Status.
- Vanilla traffic: send `Player` (0x03) on idle ticks; send `Steer Vehicle`
  (0x0C) only while riding (today it is sent every tick); send
  `Entity Action` on sprint and sneak transitions; keep the vanilla order of
  the packets within a tick.
- `cargo fuzz` targets for the play decoders and the chunk parser;
  `proptest` round-trips for every packet (closes the M1 criterion).

**Known problems fixed:** `docs/roadmap-review.md` §3.1.

**MCP to read:** `EntityPlayerSP` (`onUpdate`, `onUpdateWalkingPlayer`,
`onLivingUpdate`, `sendHorseInventory`), `NetHandlerPlayClient`, every
`S*Packet*` / `C*Packet*` class of `network/play`.

**Done when:** the coverage test passes with no silently skipped id; the P0
trace diff of the movement script shows the same packet ids in the same
order as vanilla; the fuzzers run 10 minutes without a panic (documented).

### P2 — Game-logic extraction & core player loop

**Features**
- Move the interaction rules out of `umicraft-client/src/game.rs` into
  `umicraft-game` (backlog "Interaction rules"): inputs and server events in,
  packets out, unit-tested.
- Hold-to-break with the vanilla progress (hardness, harvest tool,
  Efficiency, Haste, Mining Fatigue, in water, airborne), abort, break cracks
  (own and other players' via 0x25), arm swing sent and rendered; replaces
  the fixed 12-tick break.
- Death screen and respawn (`Client Status`), health reset.
- Gamemodes: survival, creative (instant break, double-tap-space flight,
  `Player Abilities`), adventure, spectator (no-clip, `Camera`).
- Movement: sneak edge clamp, sprint forbidden at food ≤ 6, status effects
  (Speed, Slowness, Jump Boost) and `Entity Properties` movement speed,
  Depth Strider, **local knockback** from `Entity Velocity` and `Explosion`.
- Item use: right-click hold and release, use-item slowdown, pick block
  (middle click), drop item (Q, Ctrl+Q).
- Camera: first-person hand, view bobbing, FOV modifiers, F5 perspectives,
  block selection outline.
- Riding: mount, dismount, passenger offset, `Steer Vehicle`.

**Known problems fixed:** `docs/roadmap-review.md` §3.2.

**MCP to read:** `PlayerControllerMP` (`clickBlock`, `onPlayerDamageBlock`,
`sendUseItem`, `onStoppedUsingItem`), `Block.getPlayerRelativeBlockHardness`,
`EntityPlayer.getToolDigEfficiency`, `EntityPlayerSP`, `EntityLivingBase`
(`moveEntityWithHeading`, `jump`, `getAIMoveSpeed`), `Entity.moveEntity`
(sneak clamp), `MovementInputFromOptions`, `EntityRenderer` (`getFOVModifier`,
`setupViewBobbing`, `hurtCameraEffect`), `AbstractClientPlayer.getFovModifier`,
`ItemRenderer`, `RenderGlobal.drawSelectionBox`, `GuiGameOver`.

**Done when:** a 30-minute live survival session (walk, sprint, sneak on
edges, break stone with and without a pickaxe, place, eat, die, respawn, fly
in creative, ride a boat) has no kick and no rubber-banding, and its trace
matches vanilla for the same script; the extracted rules have unit tests.

### P3 — Items: registry, models & rendering

**Features**
- 1.8 item registry: id, name, max stack, max damage, block item, armour
  material, tool tier, food values, cited in `docs/sources.md`.
- `models/item` parsing: generated layers, `builtin/entity`, block items
  through the block model, display transforms (GUI, first and third person,
  ground, head, fixed).
- Item rendering in the GUI (icon, count, per-item durability bar,
  enchantment glint) and in the world (dropped items with bob, spin and stack
  size; held items; item frames).
- Item NBT for display: custom name, lore, enchantments, leather colour,
  skull owner, potion type.

**Known problems fixed:** item ids drawn as text in the hotbar and the slots;
dropped items drawn as a generic cube.

**MCP to read:** `Item`, `Items`, `ItemModelMesher`, `RenderItem`
(`renderItemIntoGUI`, `renderItemOverlays`, `renderEffect`), `ItemRenderer`,
`EntityItem`, `RenderEntityItem`, `ItemStack.getTooltip`.

**Done when:** every 1.8 item shows its real icon in the GUI and on the
ground; the screenshot diff of a reference hotbar and inventory is within
the agreed threshold.

### P4 — Inventory & containers (finishes M9)

**Features**
- Slot interaction: left and right click, shift-click, number keys, middle
  click (creative), Q in the GUI, drag-split, double-click collect; cursor
  item; `Click Window` with the predicted item; rollback on a rejected
  `Confirm Transaction`.
- Player inventory: armour slots, 2×2 crafting with result, armour bar read
  from the armour slots (not from `Entity Equipment`), active effects list.
- Container screens: chest (single, double), ender chest, crafting table,
  furnace (progress from `Window Property`), brewing stand, dispenser and
  dropper, hopper, enchanting table (`Enchant Item`), anvil (`MC|ItemName`),
  beacon (`MC|Beacon`), villager trading (`MC|TrList`, `MC|TrSel`), horse
  inventory, book and quill (`MC|BEdit`, `MC|BSign`), sign editing.
- Creative inventory: tabs, search, `Creative Inventory Action`.
- Tooltips with the vanilla layout.

**Known problems fixed:** `docs/roadmap-review.md` §3.4.

**MCP to read:** `GuiContainer` (`mouseClicked`, `mouseClickMove`,
`mouseReleased`, drag logic), `PlayerControllerMP.windowClick`,
`Container.slotClick`, `GuiInventory`, `GuiContainerCreative`,
`GuiFurnace`, `GuiBrewingStand`, `GuiEnchantment`, `GuiRepair`,
`GuiBeacon`, `GuiMerchant`, `GuiScreenHorseInventory`, `GuiScreenBook`,
`GuiEditSign`.

**Done when:** on a live server every container type opens, items move with
every click mode, no full-window resync caused by a desync occurs over a
15-minute scripted session, and the trace matches vanilla for the same
clicks.

### P5 — Entities (finishes M8)

**Features**
- Animation maths: limb swing (vanilla smoothing), body yaw
  (`renderYawOffset`), teleport interpolated over 3 ticks, head pitch.
- Hurt tint and death fall (`Entity Status` 2/3), arm swing and crits
  (`Animation`), sleeping (`Use Bed`), eating, item-use poses (sword block,
  bow pull), sneaking pose for every humanoid.
- Typed metadata per entity class and variants: baby, sheep colour and
  shearing, slime size, creeper fuse and charge, wither skeleton, zombie
  villager, villager profession, horse type, variant and armour, wolf and
  ocelot taming and collar, custom name, armour-stand flags and pose, potion
  colour, arrows in body, absorption.
- Lighting from the world at the entity position (today every vertex is
  fullbright); shadows; fire overlay; invisibility.
- Players: `Player List Item` profiles, skin and cape fetch and cache
  (`fetch_mojang_skin` is implemented but unused), slim model from the
  profile, skin layers from metadata, name tags with team rules.
- Armour layers and held items in third person.
- Objects: every `Spawn Object` type (boats, minecarts, TNT, falling blocks,
  arrows, snowballs, eggs, pearls, potions, fireballs, fishing bobber,
  ender crystal, fireworks, leash knot, item frames, armour stands), XP
  orbs, paintings, lightning; client-side motion for projectiles, items and
  falling blocks; `Collect Item` animation; riding and leashes
  (`Attach Entity`).

**Known problems fixed:** `docs/roadmap-review.md` §3.3.

**MCP to read:** `EntityLivingBase` (`onUpdate` body-yaw block,
`moveEntityWithHeading` limb swing, `handleStatusUpdate`),
`NetHandlerPlayClient` (`handleEntityTeleport`, `handleEntityMovement`,
`handleSpawnObject`, `handleAnimation`), `Entity.setPositionAndRotation2`,
`DataWatcher` and each entity class's `entityInit`, `RendererLivingEntity`,
`RenderPlayer`, `ModelBiped.setRotationAngles`, `LayerBipedArmor`,
`LayerHeldItem`, `LayerCape`, `Render.renderLivingLabel`, `Render.renderShadow`,
`RenderManager`, `SkinManager`, `NetworkPlayerInfo`.

**Done when:** on a live server, side-by-side recordings of a reference
scene (players walking, sneaking, sprinting and fighting; every mob type;
projectiles; vehicles; item frames) match vanilla on the owner's checklist;
unit tests check the swing, yaw and interpolation maths against the MCP
formulas.

### P6 — Combat & PvP

**Features**
- Entity ray-cast with the vanilla reach and hitbox expansion, and the
  vanilla priority between blocks and entities.
- Attack: swing then `Use Entity` in the vanilla order; interact and
  interact-at (armour stands).
- Sprint reset on hit (motion × 0.6 and sprint stopped), knockback received
  (P2), hurt camera tilt, heart flash.
- Critical hits with particles, sharpness particles.
- Sword blocking, bow charge with FOV zoom, fishing rod, snowballs, eggs,
  pearls, splash potions, golden apples and absorption, regeneration,
  poison and wither heart styles.
- Death message from `Combat Event`.

**MCP to read:** `EntityRenderer.getMouseOver`, `PlayerControllerMP`
(`attackEntity`, `interactWithEntitySendPacket`, `onPlayerRightClick`),
`EntityPlayer.attackTargetEntityWithCurrentItem`, `Minecraft.clickMouse` /
`rightClickMouse`, `ItemSword`, `ItemBow`, `ItemFishingRod`.

**Done when:** a scripted 1v1 on a local server (melee with sprint resets,
bow, rod, pearls, splash potions) produces the same serverbound sequence as
vanilla and knockback trajectories within the P0 tolerance; a manual PvP
session on a public 1.8 network (after P12) raises no anti-cheat flag.

### P7 — World rendering (finishes M5 and M6)

**Features**
- Neighbour-dependent states (`getActualState`): fences, fence gates in
  walls, panes, iron bars, walls, stair shapes, doors (combined halves),
  double plants, vines, redstone wire, tripwire, snowy grass from the block
  above (removes the M5 deviation).
- Block entities: chests with the lid animation (`Block Action`), ender and
  trapped chests, sign text, skulls (with player skins), banners, beacon
  beam, end portal, enchanting-table book, spawner mob, moving pistons,
  flower-pot contents.
- Animated textures (AST-6), including water, lava, fire and portal.
- Vanilla sky: `sun.png`, `moon_phases.png`, `clouds.png` (fast and fancy),
  stars, sunrise colours, Nether and End skies.
- Weather: rain, snow, rain darkening, lightning, driven by
  `Change Game State`.
- World border rendering.
- Fog parity (distance, water, lava, blindness).
- A frame-time benchmark, recorded in `docs/perf-log.md` (the M6 criterion
  was never measured).
- Backlog: liquid corners on a diagonal chunk boundary.

**MCP to read:** each `Block*.getActualState`, `BlockModelShapes`,
`TileEntityRendererDispatcher` and every `TileEntity*Renderer`,
`TextureAtlasSprite.updateAnimation`, `RenderGlobal` (`renderSky`,
`renderClouds`, `renderWorldBorder`, `drawBlockDamageTexture`),
`EntityRenderer` (`renderRainSnow`, `updateFogColor`, `setupFog`),
`WorldProvider`, `World.getSkyColor`.

**Done when:** the reference screenshot set (village, Nether, End, rain,
night, redstone, fences, panes and walls, chests and signs) is within the
agreed threshold; the frame-time benchmark exists.

### P8 — Particles & visual effects

**Features**
- Particle engine (billboards, physics, lighting, the particle setting).
- `Particle` and `Effect` packets; client-side particles: digging and break,
  sprint footsteps, rain splash, water and lava drips, torch, fire and
  portal, potion swirls from metadata, crits, explosions.
- Screen overlays: underwater, in-wall, fire, pumpkin, portal, vignette.

**MCP to read:** `EffectRenderer`, `EnumParticleTypes`, every `Entity*FX`
class, `RenderGlobal.playAuxSFX` / `spawnParticle`, `GuiIngame`
(`renderVignette`, `renderPumpkinOverlay`, portal overlay `func_180474_b`), `ItemRenderer`
(`renderOverlays`).

**Done when:** every 1.8 particle type renders; reference recordings of
breaking, explosions, potions and rain match vanilla on the checklist.

### P9 — HUD, chat & social

**Features**
- Vanilla HUD from `widgets.png` and `icons.png`: hotbar with item icons,
  hearts (all styles, absorption, flash), food (hunger style), armour, air
  bubbles, XP bar and level, mount health and jump bar, held-item name,
  crosshair, GUI scale. Replaces today's coloured-rectangle placeholders.
- Chat: JSON components with styles, legacy `§` codes, click and hover
  events, input history, commands, tab completion, chat options, the
  downloaded language files instead of the hard-coded table (AST-8).
- Tab list (profiles, header and footer, ping, heads), scoreboard sidebar,
  below-name and list objectives, teams (prefix, suffix, colour, name-tag
  visibility), titles, subtitles, action bar, boss health bar (derived from
  the Wither or Ender Dragon entity in 1.8).
- Vanilla F3 debug screen; the Umicraft debug data moves to another key.

**MCP to read:** `GuiIngame`, `GuiNewChat`, `GuiChat`, `GuiPlayerTabOverlay`,
`GuiOverlayDebug`, `BossStatus`, `ScoreObjective`, `ScorePlayerTeam`,
`IChatComponent`, `ChatStyle`, `FontRenderer`, `ScaledResolution`.

**Done when:** HUD screenshots at GUI scales 1–4 match vanilla; chat from a
real server (colours, hover, click) renders like vanilla; a minigame
server's scoreboard and tab list display correctly.

### P10 — Screens, options & controls

**Features**
- Title screen, multiplayer server list (ping, add, edit, delete, direct
  connect), connecting and disconnect screens, pause menu, death screen,
  statistics screen.
- Options: video (render distance, graphics, smooth lighting, GUI scale,
  brightness, FOV, view bobbing, clouds, particles, max FPS, vsync,
  mipmaps), controls with rebinding, chat, skin customisation (sent in
  `Client Settings`), sounds, resource packs, language; persisted to disk.
- The CLI connection flags stay for tests but are no longer the main path.

**MCP to read:** `GuiMainMenu`, `GuiMultiplayer`, `ServerSelectionList`,
`GuiConnecting`, `GuiDisconnected`, `GuiIngameMenu`, `GuiOptions`,
`GuiVideoSettings`, `GuiControls`, `GuiScreenResourcePacks`, `GameSettings`.

**Done when:** a user can start the client, add a server, connect, change
every vanilla option, rebind a key, disconnect and quit without the command
line; the options survive a restart.

### P11 — Audio

**Features:** an ADR choosing the audio crate; the sound index from the
asset index; `Sound Effect` and `Effect` sounds; client-side sounds
(footsteps, block break and place, GUI clicks, rain); 3D positioning;
volume categories; music.

**MCP to read:** `SoundHandler`, `SoundManager`, `SoundEventAccessorComposite`,
`MusicTicker`, `PositionedSoundRecord`, `Entity.playStepSound`.

**Done when:** a reference session's sounds match vanilla on the checklist;
the audio thread never blocks the game or render thread (measured).

### P12 — Online mode & real-server compatibility

**Features**
- A short spike first: current Microsoft endpoints, the device-code flow,
  the licence of the secret-storage crate.
- Microsoft device code → Xbox Live → XSTS → Minecraft services → profile;
  session join; encryption handshake; keychain token storage
  (ACC-4/5/6/7, NET-4). One account is enough here; the manager is F1.
- BungeeCord and Velocity server switching (NET-10), server resource packs
  with prompt and SHA-1 check (NET-11), plugin-channel handshake.

**MCP to read:** `NetHandlerLoginClient`, `CryptManager`,
`NetHandlerPlayClient.handleResourcePack`, `handleRespawn`,
`ResourcePackRepository`.

**Done when:** a premium account joins an online-mode 1.8 server and a
BungeeCord network, switches servers without reconnecting and accepts a
server resource pack; tokens never appear in the logs.

### P13 — Parity sign-off

**Features:** run every harness script of P0–P12 against vanilla; fix the
remaining differences or record them as owner-approved deviations in
`docs/parity.md`; a 2-hour stability session on a real server.

**Done when:** every row of `docs/parity.md` is "matches vanilla" or an
approved deviation.

---

## 6. After parity

| Milestone | Content | Done when |
|---|---|---|
| **F1 — Account manager & live switching** | `umicraft-auth`, offline accounts with the standard UUID, multi-account manager, live switch, account UI in menus (ACC-1/2/3/8/10). | Former M12 and M13 acceptance criteria. |
| **F2 — Performance pass** | Frustum and occlusion culling, mesh priority queue, GPU-driven rendering, upload budget, allocation audit, CI benchmarks (backlog "Performance"). | Before/after numbers in `docs/perf-log.md`; spec §7 targets met or explained. |
| **F3 — Modding API v1** | Former M15, including the move of built-in features onto `umicraft-core`. Resolve first the contradiction between spec §8.4 (ECS for mods) and ADR-0015 (typed `EntityStore`). | Former M15 acceptance criteria. |
| **F4 — Hardening & v1.0** | Former M16 minus proxies (moved to P12): long sessions, packaging, crash reports, docs site, legal review. | `docs/release.md` fully ticked. |

The legal review of the code derived from MCP (ADR-0001, backlog "Decisions
needed") blocks **any public release**, not only F4.

## 7. Requirement families to add to spec §6

| Prefix | Area | Examples |
|---|---|---|
| CMB | Combat | attack/swing order, sprint reset, knockback received, crits, blocking/bow/rod/projectiles |
| ITM | Items | registry, item models, use actions, NBT display |
| EFX | Effects | status effects, particles, screen overlays |
| ENT | Entities | animations, metadata variants, skins and capes, armour/held items/name tags, objects and vehicles |
| BLE | Block entities | chests, signs, skulls and banners, beacon/portal/spawner |
| SCR | Screens | pause/death/disconnect, options, server list |
| PAR | Parity verification | packet-trace diff, trajectory diff, screenshot diff, parity register |

With parity as the goal, RND-10 (particles), AUD-2 (music), NET-10
(proxies) and NET-11 (server resource packs) become **P1**.

## 8. Open points

1. **Tolerances** for the harness: trajectory error per tick and screenshot
   diff threshold (to fix in P0).
2. **Accepted deviations**, e.g. F2 opening the Umicraft debug screen
   instead of taking a screenshot (to record in `docs/parity.md`).
