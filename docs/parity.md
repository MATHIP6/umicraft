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
| Movement | walk/sprint/sneak/jump/swim/ladder/web/soul sand/ice/slime | Partial | P2 | Vanilla vs Umicraft movement run, 2026-10-26 ([parity-harness.md](parity-harness.md#vanilla-comparison-2026-10-26)) | Ground speeds of walk (0.2159), sneak (0.0648) and sprint (0.2806 block/tick) match vanilla on the live server; swim/lava/ladder/web/soul sand are unit-tested against the reference formulas but not compared on a live server yet (they were removed by mistake in commit 05f0cc1 and restored on 2026-10-28). |
| Movement | sneak edge clamp | Partial | P2 | — | Implemented in `umicraft_game::collision` (the review missed it); its Z axis cancelled every sneaking Z move until 2026-10-26 (fixed, regression test). Edge behaviour not yet compared with vanilla. |
| Movement | hunger sprint rule, effects (Speed/Slowness/Jump), Depth Strider | Partial | P2 | Unit tests (`physics`, `game`) | Depth Strider reads the `ench` NBT of the boots. Not compared with vanilla on a live server. |
| Movement | flying, creative, spectator | Partial | P2 | Unit tests (`physics`, `gamemode`, `game`) | `Player Abilities`, double-tap-space flight, no-clip and the `Camera` view (ADR-0021). Not compared with vanilla on a live server. |
| Movement | knockback (velocity, explosion) | Partial | P2 | Unit tests (`game`) | `Entity Velocity` and `Explosion` change the local motion (ADR-0021). Not compared with vanilla on a live server. |
| Movement | riding (boat, minecart, horse, pig) | Partial | P2 | Unit tests (`riding`, `game`) | The rider has no physics and sits on the seat of the vehicle (`umicraft_game::riding`, ADR-0027); `Steer Vehicle` is sent. Horse rearing offset, horse jump (`Entity Action` 5), the sitting body pose and the vehicle rendering (P5) are missing. Not run on a live server. |
| Movement | serverbound traffic identical to vanilla (0x03, 0x0B, no 0x0C) | Partial | P1 | P1 run, 2026-10-26 ([parity-harness.md](parity-harness.md#p1-run-2026-10-26)) | Implemented in `umicraft_game::traffic`: 0x03 on idle ticks, 0x0B sprint/sneak transitions before the movement packet, 0x0C only while riding, silence until the player's column loads. The live run had the player blocked by terrain, so the moving and sprint ticks still have to be compared from a clear spot. |

## Interaction

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Interaction | hold-to-break with vanilla progress, cracks, particles | Partial | P2 | Unit tests (`digging`, `interaction`, `game`) | Vanilla progress (hardness, tool, Efficiency, Haste, fatigue, water, air), own and 0x25 cracks, abort. Block fragments from P8. Not compared with vanilla on a live server. |
| Interaction | place blocks | Partial — not verified against vanilla | P2 | — |  |
| Interaction | use items (food, potions, bow, sword block, rod, throwables, buckets) | Partial | P2 | Unit tests (`item_use`, `game`) | Right button held and released, repeat every 4 ticks, 0.2 slowdown, `Entity Status` 9. Eat / drink / bow / sword start a local use; throwables, buckets and rods only send the use packet. Hoes, seeds, signs and doors also send it after their block use (harmless). Bow draw animation and FOV zoom, and the item models, wait for P3 / P6. |
| Interaction | pick block, drop item, swap hotbar | Partial | P2 | Unit tests (`pick`, `game`) | Middle button picks a block (creative fills a free slot and sends 0x10); Q / Ctrl+Q send the drop packets. The block-to-item table is hand-written until the P3 registry; picking an entity (creative) waits for P5 / P6. |
| Interaction | signs, books, beds | Missing | P4 | — | Sign and book editing in P4; bed (`Use Bed`) in P1/P5. |

## Combat

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Combat | attack entity (swing + Use Entity), entity ray-cast, reach | Partial | P6 | Unit tests (`combat`, `item_use`, `interaction`, `game`) | `getMouseOver` ported: block reach, 3-block survival entity reach, 6-block creative ray, hitbox border, baby and slime sizes, item frames, marker stands, spectators ignored (ADR-0029). Right click sends interact-at then interact, then the item use unless the client consumes the click: item frames, boats, minecarts, horses, adult villagers, armour stands, a bucket on a cow, a bowl or shears on a mushroom cow, the food of each animal, a saddle on a pig and a dye on a sheep are known to consume it, with the lead (tying, untying, tamed pets only for their owner), a named name tag on a mob, a spawn egg on any ageable mob, a wolf's meat, bone and new collar, fish for an ocelot, and the minecart types that answer true (rideable, chest, furnace, hopper). An adult animal is taken as ready to breed (the client does not see the cooldown), a villager as not trading and a cart as free; a cured zombie villager (golden apple) is not consumed, its weakness effect is not tracked. The ray also meets boats, minecarts, end crystals, large fireballs, primed TNT, falling blocks, paintings and leash knots with the boxes of the reference. Not compared with a live server. |
| Combat | sprint reset on hit (vanilla `motion *= 0.6`, sprint off) | Partial | P6 | Unit tests (`combat::sprint`, `game`) | Local only against players: the client world answers false to `attackEntityFrom` for mobs (ADR-0029). |
| Combat | hurt camera tilt, heart flash | Partial | P6 | Unit tests (`combat::damage_view`, `hud_hearts`, `game`) | `hurtCameraEffect` roll `-sin((hurtTime / 10)^4 pi) * 14` from `Entity Status` 2 and `Animation` 1 of the local player; the hearts blink every 3 ticks for 20 after a loss (10 after a gain) and outline the old health for a second (`renderPlayerStats`, the ticks of the game thread stand for the frames). Hurt tint and death fall on entities are P5. Not compared with a live server. |
| Combat | crit/sharpness particles, bow charge FOV | Partial | P6 | Unit tests (`combat::effects`, `view`, `game`) | `Animation` 4 and 5 start the 3 bursts of `EntityParticleEmitter` on the entity; against another player the client also predicts its own critical (falling, off the ground, ladder, water, blindness, riding) and Sharpness stars. Against mobs only the server's packets count (the client world answers false). The bow narrows the FOV by up to 15 % over a second (`getFovModifier`). The stars are the `Crit` and `MagicCrit` particles of P8; their random numbers come from the engine's generator, not from one `java.util.Random` per emitter. |
| Combat | death screen, respawn, combat event message | Partial | P2 | Unit tests (`game`, `umicraft-ui`) | Death screen and `Client Status` respawn (ADR-0021); the Title Screen button is P10. The vanilla client shows no message from `Combat Event`: `handleCombatEvent` only feeds the Twitch metadata, the text comes from the chat packet. |

## Items

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Items | item registry (names, max stack, max damage) | Partial | P3 | Unit tests (`umicraft_game::items`) | 337 items generated from `Item.registerItems` (ADR-0024). Fish food values depend on the metadata in vanilla and are 0 in the table. Not yet compared with vanilla by an owner checklist. |
| Items | item models and icons in GUI, hand, ground, frames | Partial | P3 | Unit tests (`item_icons`, `item_tints`, `dials`, `item_world`, `umicraft_model::item`), ignored GPU tests with the real jar (`offscreen_items`, `real_icons_*`, `real_items_*`, `players_hold_*`, `items_in_use_*`, `real_skull_and_banner_icon*`, `real_skulls_and_banners_*`) | All 512 models of the 1.8.8 jar bake. Icons (GUI), dropped items (bob, spin, stack copies), the first-person item (idle, swing, eating, drinking, sword block, bow draw with the pulling models), items in a remote player's hand and in item frames are drawn. Tints: leather, potions, spawn eggs, firework charges, grass and foliage block items. The compass and the clock follow the heading, spawn and time of day (damped spring, 20 Hz). Enchantment glint on all of them. Also drawn: items held by mobs, bow pulling and cast rod models of other players, filled maps in hand and frames, world light on items. Skull and banner items (`builtin/entity`) are drawn in the GUI, in hand, on the ground and in frames with the models of the placed blocks (ADR-0034; GPU test `real_skull_and_banner_icons_render` checked by eye, not in game; no glint). Missing: chest items (`builtin/entity`), the owner screenshot diff; the new rendering is not checked in game. The glint uses a virtual cell grid instead of the stitched atlas positions, and the hover phase and dial targets use deterministic generators (ADR-0028). |
| Items | enchantment glint, tooltips, durability bar | Partial | P3 | Unit tests (`umicraft_ui::widgets::item_overlay`) | Durability bar and stack count follow `renderItemOverlayIntoGUI`; the glint is drawn on GUI icons, dropped items, items in frames, items in a remote player's hand and the first-person item (additive, texture mapped from the sprite UVs like the reference). Tooltips: box widget and durability line done; potion effects and the map line are missing; layout finishes in P4. |

## Inventory

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Inventory | window protocol decode | Partial — not verified against vanilla | P4 | — |  |
| Inventory | slot interaction: left, right, shift, drag, double click, number keys, Q / Ctrl+Q, creative middle | Partial — not verified against vanilla | P4 | unit tests, no live trace yet | Shift + double click does not shift-click every matching slot; pick-block key over a slot, touchscreen mode and the empty-cursor outside click (mode 4, −999) are not sent. Middle click is filtered client side in survival. |
| Inventory | click prediction and rollback of a rejected `Confirm Transaction` | Partial — not verified against vanilla | P4 | `inventory::transactions` tests | No recipe book: the crafting result is server driven, taking it consumes one item per ingredient; no furnace fuel / brewing ingredient validation; shift-click uses a generic plan outside the player window, crafting table and plain inventories (ADR-0025). |
| Inventory | armour bar from the armour slots | Partial — not verified against vanilla | P4 | `armour_bar_follows_the_armour_slots` | `Entity Equipment` of the local player is ignored. |
| Inventory | container screens: chest (single, double, ender), crafting table, furnace (progress), dispenser, dropper, hopper | Partial — not verified against vanilla | P4 | `umicraft-ui` screen tests | Screenshot diff pending. |
| Inventory | player model in the inventory screens | Missing | P4 | — | `GuiInventory.drawEntityOnScreen`; not done in the survival inventory nor in the creative Inventory tab. |
| Inventory | active effects list in the inventory | Missing | P4 | — |  |
| Inventory | container screens: brewing stand, enchanting table, anvil, beacon, villager, horse, book, sign | Missing | P4 | — | Fall back to a chest layout (P4d). |
| Inventory | item tooltips (name by rarity, enchantments, lore, attack damage, unbreakable) | Partial — not verified against vanilla | P4 | `inventory::tooltip` tests | English table only until a language file loader exists; no potion, book, map, firework or banner lines; solid tooltip border. |
| Inventory | creative inventory (12 tabs, scrolling, search, trash, hotbar, `Creative Inventory Action`) | Partial — not verified against vanilla | P4 | `inventory::creative` and `creative_*` client tests | E opens it in game mode 1. Gaps: player model of the Inventory tab, tab-name line in the Search tooltip, Shift + double click, middle-button drag, double click outside the Inventory tab, banners without patterns. Search matches `item.<name>.name` only, so variants (granite, red wool) are not found by name until the tooltip names get per-metadata keys. Hotbar number-key swap reports both slots (vanilla resends only one). |

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
| World render | neighbour-dependent shapes (fences, panes, walls, stairs, doors…) | Partial — not verified against vanilla | P7 | Unit tests (`umicraft_mesh::actual_state::*`), `tests/actual_state.rs` (wiring); `umicraft_model` `model_by_key` | Fences, panes, bars, walls, gates in walls, stair shapes, doors, double plants, vines, redstone wire, tripwire and snowy grass (ADR-0030). Approximations: `is_opaque()` stands for `isFullCube`; redstone power providers are an id table. Not compared with the owner's screenshots. |
| World render | block entities | Partial — not verified in game | P7 | Unit tests (`umicraft_mesh::block_entity`, `chest_lids`, `block_entity_render::*`, `piston::*`, `umicraft_model::entity::{chest,sign,banner,book}`) | Chests (single, double, trapped, ender) with the lid, signs with text, skulls (owner skins), banners (patterns), beacon beam, end portal, enchanting-table book, spawner mob, flower-pot plants and moving pistons with a player push (code only, no GPU or in-game check). Accepted deviations: sign text styles and the edit marker; Christmas chests; chest and piston sounds; book follows the camera only and flips pages with a deterministic generator; beam has no fade-in, no glass colours and is additive; the end portal layers are added instead of replacing the frame; banner sway uses the client clock; spawner draws only the main mob layers. |
| World render | animated textures (water, lava, fire, portal, prismarine, sea lantern) | Partial | P7 | Unit tests (`animation`, `texture_animator`, `client_resources`) | Stepped once per 20 Hz tick from the `.mcmeta` timeline, with interpolation; not compared with vanilla frame by frame. Clock and compass wait for P3. |
| World render | smooth lighting, AO, transparency, biome tint, fluids (sloped surface, flow direction, biome tint; restored 2026-10-28) | Partial — not verified against vanilla | P7 | — | Frame time never measured (M6 criterion). |
| World render | vanilla sky/sun/moon/clouds/stars textures | Partial — not verified in game | P7 | Unit tests (`umicraft_render::sky::*`, `clouds::*`), ignored GPU tests `offscreen_sky` | The sky mirrors `renderSky`: dome, 1500-star field from `Random(10842)`, `sun.png` and `moon_phases.png`, sunrise fan, End box, Nether without sky, fast and fancy clouds from `clouds.png`, with original fallbacks without a jar. Missing: rain and thunder inputs (P7f), the fog parts that depend on the camera (P7g); the sky reads lighter than vanilla (sRGB target, as the terrain fog). Cloud drift uses the client clock, not `cloudTickCounter`; fast clouds write no depth. Not compared with the owner's screenshots. |
| World render | fog parity (water/lava/blindness) | Partial — not verified in game | P7 | Unit tests (`umicraft_render::fog`, `fog_context`), ignored GPU tests `offscreen_fog` | Linear and exponential fog from `setupFog` / `updateFogColor`: plain, rain, water, lava, blindness, night vision, void below y=32, Nether, End, sunrise tint toward the sun. Missing: respiration and aqua affinity, the boss-bar colour, a flat world's void factor; the eye light ignores rain; exponential fog uses radial distance; the rain pass fogs linearly even under water (ADR-0032). |
| World render | weather, lightning | Partial — not verified in game | P7 | Unit tests (`server::weather`, `umicraft_render::weather`, `precipitation`), ignored GPU test `offscreen_weather` | Rain and snow quads from `renderRainSnow` with the vanilla offsets and per-column seeds, rain and thunder darkening of sky and fog, the lightning flash of the sky colour. There is no client ramp: 1.8 shows exactly what reasons 7 and 8 send (ADR-0031). Missing: splashes (P8), sounds (P11), the flash on the terrain lightmap, the temperature noise term. |
| World render | world border | Partial — not verified in game | P7 | Unit tests (`umicraft_render::border`, `border_view`), ignored GPU test `offscreen_border` | Four scrolling walls from `forcefield.png` coloured by status (blue stationary, green growing, red shrinking), fade with the distance, depth-biased. One stretched quad per wall; the diameter is not interpolated inside a tick. |
| World render | block outline, first-person hand, view bobbing, F5 | Partial | P2 | Unit tests (`game`, `selection`) | Outline = vanilla selected bounding box (`umicraft_game::selection`), liquids are never targeted, hand, bobbing, FOV, F5 with the held item in the local body. |

## Effects

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Effects | particles (packet + client-side) | Partial | P8 | Unit tests, ignored GPU tests (`offscreen_particles`), criterion `particles` | 33 of the 42 types are simulated and drawn; block fragments, `Particle`/`Effect`/`Explosion` packets, ambient block particles, rain splashes and sprint dust are wired. Not done: item-sprite particles, footprints, the large explosion sheet, potion swirls from metadata, crit stars from `Animation`, `ItemTake`, `MobAppearance`, firework shells. Not compared with vanilla on a screen. |
| Effects | particles option (All / Decreased / Minimal) | Partial | P8 | Unit tests (`spawn`, `options`) | Saved with the options and applied by the spawn filter; the video screen cycles it. |
| Effects | screen overlays (underwater, in-wall, fire, pumpkin, portal, vignette) | Partial | P8 | Unit tests, ignored GPU tests (`offscreen_overlays`) | Overlays need the model terrain; `is_opaque` approximates the in-wall test. Not compared with vanilla on a screen. |

## HUD

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| HUD | hotbar, hearts, food, armour, XP with vanilla textures | Partial | P9 | — | Coloured-rectangle placeholders. |
| HUD | air, absorption, effect hearts, mount bar | Partial | P9 | Unit tests (`hud_hearts`) | Absorption, regeneration, poison, wither, hardcore and the low health shake are done with the hearts (P6); the air bubbles and the mount bar are not. |
| HUD | chat with colours/styles/events | Partial | P9 | — |  |
| HUD | tab list, scoreboard, teams, titles, action bar, boss health | Missing | P9 | — |  |
| HUD | vanilla F3 debug screen | Partial — lines and drawing done (P9-g), not all data wired | P9 | `umicraft_game::debug_f3` tests | F3 toggles it; the Umicraft debug screen moved to F4 (proposed, awaiting owner approval). GPU adapter/backend and memory replace the GL/Java lines. Entities, particles, light, biome and looking-at data are not fed yet (P9-h). |

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
| Resources | resource packs (stack, reload) | Partial — not verified against vanilla | P7 | — | Animated block textures follow the pack `.mcmeta` (ADR-0023). |
| Resources | language files | Missing | P9 | — | Downloaded files unused (AST-8). |

## Approved deviations

| Deviation | Status |
|---|---|
| F2 opens the Umicraft debug screen instead of taking a screenshot (ROADMAP.md §8.2). Vanilla F3 is reproduced in P9; the Umicraft debug data moves to another key. | Proposed, awaiting owner approval |
| P9-g: the Umicraft debug screen moves from F2 to F4 (F3 is the vanilla text screen, F2 stays free for the vanilla screenshot, not implemented); the old F4 resource reload is dropped, F3+T remains. | F4 proposed, awaiting owner approval |
