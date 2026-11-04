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
| Items | item models and icons in GUI, hand, ground, frames | Partial | P3 | Unit tests (`item_icons`, `item_tints`, `dials`, `item_world`, `umicraft_model::item`), ignored GPU tests with the real jar (`offscreen_items`, `real_icons_*`, `real_items_*`, `players_hold_*`, `items_in_use_*`, `real_skull_and_banner_icon*`, `real_skulls_and_banners_*`) | All 512 models of the 1.8.8 jar bake. Icons (GUI), dropped items (bob, spin, stack copies), the first-person item (idle, swing, eating, drinking, sword block, bow draw with the pulling models), items in a remote player's hand and in item frames are drawn. Tints: leather, potions, spawn eggs, firework charges, grass and foliage block items. The compass and the clock follow the heading, spawn and time of day (damped spring, 20 Hz). Enchantment glint on all of them. Also drawn: items held by mobs, bow pulling and cast rod models of other players, filled maps in hand and frames, world light on items. Skull and banner items (`builtin/entity`) are drawn in the GUI, in hand, on the ground and in frames with the models of the placed blocks (ADR-0034; GPU test `real_skull_and_banner_icons_render` checked by eye, not in game; no glint). Potions with an effect glint (`ItemPotion.hasEffect`, including `CustomPotionEffects`). Chest, trapped chest and ender chest items use the same path (shut, unturned, no glint). Missing: the owner screenshot diff; the new rendering is not checked in game. The glint uses a virtual cell grid instead of the stitched atlas positions, and the hover phase and dial targets use deterministic generators (ADR-0028). |
| Items | enchantment glint, tooltips, durability bar | Partial | P3 | Unit tests (`umicraft_ui::widgets::item_overlay`) | Durability bar and stack count follow `renderItemOverlayIntoGUI`; the glint is drawn on GUI icons, dropped items, items in frames, items in a remote player's hand and the first-person item (additive, texture mapped from the sprite UVs like the reference). Tooltips: box widget, durability line and potion effects (name, level, duration, "When Applied" modifiers, `CustomPotionEffects`) done; the map line is missing; layout finishes in P4. |

## Inventory

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Inventory | window protocol decode | Partial — not verified against vanilla | P4 | — |  |
| Inventory | slot interaction: left, right, shift, drag, double click, number keys, Q / Ctrl+Q, creative middle | Partial — not verified against vanilla | P4 | unit tests, no live trace yet | Shift + double click does not shift-click every matching slot; pick-block key over a slot, touchscreen mode and the empty-cursor outside click (mode 4, −999) are not sent. Middle click is filtered client side in survival. |
| Inventory | click prediction and rollback of a rejected `Confirm Transaction` | Partial — not verified against vanilla | P4 | `inventory::transactions` tests | No recipe book: the crafting result is server driven, taking it consumes one item per ingredient; no furnace fuel / brewing ingredient validation; shift-click uses a generic plan outside the player window, crafting table and plain inventories (ADR-0025). |
| Inventory | armour bar from the armour slots | Partial — not verified against vanilla | P4 | `armour_bar_follows_the_armour_slots` | `Entity Equipment` of the local player is ignored. |
| Inventory | container screens: chest (single, double, ender), crafting table, furnace (progress), dispenser, dropper, hopper | Partial — not verified against vanilla | P4 | `umicraft-ui` screen tests | Screenshot diff pending. |
| Inventory | player model in the survival inventory | Partial — not verified against vanilla | P4 | `portrait` unit tests (maths only) | `GuiInventory.drawEntityOnScreen`: drawn offscreen into a reserved 256 px block of the icon atlas (`PORTRAIT_KEY`) and shown as a 64 px item icon; follows the pointer; worn armour and held item come from the window. Also drawn in the creative Inventory tab (43, 45, scale 20; pointer measured from `posY - 30`). No GPU test yet. |
| Inventory | active effects list in the inventory | Missing | P4 | — |  |
| Inventory | container screens: brewing stand (progress arrow, bubbles), horse (saddle, armour, chest slots) | Partial — not verified against vanilla | P4 | `umicraft-ui` screen tests | Layouts and sprites from `ContainerBrewingStand`/`GuiBrewingStand`, `ContainerHorseInventory`/`GuiScreenHorseInventory`. The horse is not drawn in the window; the armour frame is always shown. |
| Inventory | container screens: enchanting table (option rows, `Enchant Item`), anvil (name field, `MC|ItemName`), beacon (power buttons, `MC|Beacon`), villager (offer arrows, offer items, `MC|TrList`, `MC|TrSel`) | Partial — not verified against vanilla | P4 | `umicraft-ui` screen tests, `container_events`, `anvil_field`, `inventory::payload` tests | Missing: enchantment names in the standard galactic alphabet, the animated book and the clue tooltip of the enchanting table; anvil cost line (`container.repair.cost`) and the rename of the first slot by the server; beacon tooltips; the villager greyed-out offer cross and the offer count tooltips. Live-server trace pending. |
| Inventory | book and quill, written book, sign editing | Partial — not verified against vanilla | P4 | `umicraft_game::editors` tests, `text_screens` tests | `GuiScreenBook` (pages, 256 characters and 14 lines per page, 50 pages, title of 16 characters, `MC|BEdit`, `MC|BSign`) and `GuiEditSign` (4 lines of 90 px, `Update Sign`). The book opens when the item is used. Deviations: the sign is a flat board instead of the 3D model; written-book pages show plain text (no styles or click events); the page arrows are hit areas without hover sound; no text selection or cursor movement inside a page (edits at the end only, as the reference). |
| Inventory | item tooltips (name by rarity, enchantments, lore, attack damage, unbreakable) | Partial — not verified against vanilla | P4 | `inventory::tooltip` tests | texts come from the language files (`Lang`); no potion, book, map, firework or banner lines; solid tooltip border. |
| Inventory | creative inventory (12 tabs, scrolling, search, trash, hotbar, `Creative Inventory Action`) | Partial — not verified against vanilla | P4 | `inventory::creative` and `creative_*` client tests | E opens it in game mode 1. Gaps: tab-name line in the Search tooltip, Shift + double click, middle-button drag, double click outside the Inventory tab, banners without patterns. Search matches `item.<name>.name` only, so variants (granite, red wool) are not found by name until the tooltip names get per-metadata keys. Hotbar number-key swap reports both slots (vanilla resends only one). |

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
| HUD | hotbar, hearts, food, armour, XP with vanilla textures | Partial | P9 | Unit tests (`umicraft_game::hud`, `render::hud::gui` incl. the four GUI scales, `client::hud_view`) | Textured hotbar with item icons, counts and the 24x22 cursor, inverted-blend crosshair, XP bar and level, held-item name (40 ticks, white), hearts/armour/food with the vanilla offsets. The coloured-rectangle HUD was deleted (P9-h); without the GUI atlas the hotbar and the status rows are not drawn (chat, titles, sidebar and F3 still are). Not compared with vanilla on screen. Missing: the item pop-up bounce (`animationsToGo`), the spectator crosshair rule, the italic shear of the held name, the `heldItemTooltips` option, the GUI-scale video option (the scale is always auto; element rectangles stay inside the screen and centred at scales 1 to 4 in unit tests). |
| HUD | air, absorption, effect hearts, mount bar | Partial | P9 | Unit tests (`umicraft_game::hud`, `client::hud_view`) | Air bubbles, absorption rows, poison/wither/hardcore hearts, regeneration wiggle, low-health and saturation shake from the seeded generator, damage blink, hunger food, mount hearts, horse jump bar. P9-h: `Riding Jump` (Entity Action 5, strength `(int)(power * 100)`) is sent when the jump key is released on a saddled horse (`EntityPlayerSP.sendHorseJump`). Not compared with vanilla on screen. Missing: max health of a mount without an `Entity Properties` packet defaults to 20; the hearts flash in the tab list. |
| HUD | chat with colours/styles/events | Partial | P9 | Unit tests (`umicraft_game::chat`, `render::hud::gui::chat`, `client::chat`) | JSON components, legacy `\u00a7` codes, colours, bold, underline, strikethrough, obfuscation, fade, wrapping with the proportional font, scroll, input history, commands, click and hover events, chat options. Not compared with vanilla on screen. Missing: the italic shear (italic text is drawn upright), Tab completion replaces the word instead of cycling in place, the chat scroll position is not kept when new lines arrive, the achievement hover. |
| HUD | tab list, scoreboard, teams, titles, action bar, boss health | Partial | P9 | Unit tests (`umicraft_game::hud`, `render::hud::gui`, `client::social_view`) | Tab list layout, header and footer, ping bars, sidebar, team prefix/suffix/colour, below-name and list objectives, titles with fade, action bar, boss bar. Not compared with vanilla on screen. Missing: player heads in the tab list (no skins yet), the heart flash of the list objective, the achievement hover. |
| HUD | vanilla F3 debug screen | Partial | P9 | `umicraft_game::debug_f3` tests, `client::debug_f3_view`, `client::debug_world` | F3 toggles it; text colour `0xE0E0E0` without shadow matches vanilla. P9-h feeds entities (drawn / loaded), particles, chunk cache, biome (`BiomeGenBase` names), light, local difficulty (inhabited time is 0 on the client), feet y from the player position, the looking-at block, the process memory (resident set against RAM, Linux only) and the GPU adapter name, driver and backend. Differences: the `Java:` / `CPU:` / `Allocated:` and GL lines are replaced by the memory and GPU lines; the looking-at properties come from the model registry variant key, not `getActualState`, so connections of fences and panes read as unconnected; `B:` (hidden entities) is always 0 because there is no frustum count. |

## Screens

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Server list data | saved servers (add, edit, remove, reorder), status ping with MOTD, players, latency, favicon, version mismatch | Partial | P10 | `servers::{book,pinger}` unit tests | Book and pinger exist (ADR-0041); the list screen, SRV lookup and the resource-pack mode per server are not done. |
| Screens | pause menu (`GuiIngameMenu`) | Partial — not verified in a window | P10 | `umicraft_ui::game_menu`, `client::window::session_control` tests | Resume, Options (returns to the menu), Statistics (sends the request and opens the screen) and Disconnect (ends the session and shows the server list) work. Achievements is inert (out of P10) and Open to LAN is inert. The death screen is P2 (ADR-0021). |
| Screens | title screen | Partial — wired, not verified in a window | P10 | Unit tests (`umicraft_ui::menus::title`, `panorama`, `client::menu_flow`, `client::screen`, `client::args`) | ADR-0042, ADR-0044. The window opens on it without `<host> <port>`: Multiplayer, Options (returns to the title), the language button (opens the Language page) and Quit Game work; Singleplayer is greyed. `--hud-demo --hud-view title` renders it offscreen. Missing: the panorama cube in the renderer (the dirt background is drawn; the screen computes the view and the window ignores it), the tilted splash (drawn upright), the Realms row. |
| Screens | server list (ping, add, edit, delete, direct connect) | Partial — wired, not verified in a window | P10 | Unit tests (`umicraft_ui::menus::{server_list,server_row,server_edit,confirm}`, `client::menu_flow`, `client::menu_rows`, `client::servers`) | ADR-0042, ADR-0044. Join, double click, add, edit, delete with confirmation, reorder, direct connect (the last address is saved as `[multiplayer] last_server`) and Refresh work on the saved book; pings start when the list opens. Missing: LAN entries, server icons, the resource-pack toggle, SRV lookup. |
| Screens | connecting, disconnected | Partial — wired, not verified in a window | P10 | Unit tests (`umicraft_ui::menus::{connecting,disconnected}`, `client::menu_flow`, `client::window::session_control`) | ADR-0042, ADR-0044. Connecting shows until the server places the player; Escape or Cancel ends the attempt. A kick or connection error shows the reason with "Back to server list" instead of closing the client. Deviation: one connecting text (no separate "Logging in" stage). |
| Screens | statistics | Partial — wired, not verified in a window | P10 | Unit tests (`umicraft_ui::menus::stats`, `client::menu_rows`, `client::game`, `client::world_view`) | ADR-0042, ADR-0044. The pause menu sends `Client Status` request-stats; the `Statistics` answer is listed by name with the `StatBase` formats. General list only: no achievements (inert), no block/item/mob tabs, no live refresh. |
| Screens | controls rebinding, persisted options | Partial | P10 | Unit tests (`client::options`, `client::options_screen`) | `options.toml` keeps the video, controls, chat, language, skin, sound and resource pack sections and the key bindings; a missing or broken file gives the defaults and every number is clamped (ADR-0017, ADR-0043). |
| Screens | options hub (`GuiOptions`) and its pages | Partial | P10 | `client::options_screen` tests | Hub, Video, Controls, Chat, Skin, Sounds, Resource Packs and Language follow the vanilla layout. Inert on purpose: Super Secret Settings, Broadcast Settings (Twitch) and Snooper Settings, which have no backend; Difficulty is the server's. Not compared with vanilla on screen. |
| Screens | video settings (`GuiVideoSettings`) | Partial | P10 | `client::options_screen` and `client::options_effects` tests | The 14 entries of the vanilla list in vanilla order. **Live:** Graphics (precipitation and screen overlays), Render Distance, Max Framerate, View Bobbing (camera and hand), GUI Scale (every screen and the HUD), Brightness (the lightmap gamma), Clouds (off / fast / fancy), Particles, Fullscreen (borderless), Use VSync (present mode), Entity Shadows. **Stored only:** Smooth Lighting (the mesher always lights with smooth light and corner AO; a switch needs a flag in `umicraft_mesh::quad_light` and a re-mesh of every column), Mipmap Levels (the block atlas has no mip chain yet), Other Block Variants (`allowBlockAlternatives`: the mesher has no random model variants switch). **Not offered:** 3D Anaglyph (omitted by decision) and Use VBOs (vanilla hides it without VBO support; wgpu has none). Brightness now starts at Moody (0.0) like `GameSettings.gammaSetting`, where the renderer used to start at 0.5. |
| Screens | skin customisation (`GuiCustomizeSkin`) | Partial | P10 | `client::options::skin`, `client::options_screen`, `client::options_effects`, `client::network` tests | The seven model-part toggles are sent as the skin-parts byte of `Client Settings` (bit `1 << partId`, MCP `EnumPlayerModelParts`) on join and when the options close with a changed layer. The local player's own model does not follow them yet. Main hand is a 1.9 option: the 1.8 packet has no field, so it is not offered. |
| Screens | sound volumes (`GuiScreenOptionsSounds`) | Partial | P10 | `client::options::sound`, `client::options_screen` tests | Master plus eight category sliders, saved in `[sound]`. Values only: nothing plays until P11. |
| Screens | resource packs (`GuiScreenResourcePacks`) | Partial | P10 | `umicraft_assets::pack_discovery`, `client::options::packs`, `client::options_screen`, `client::options_effects` tests | Two lists (available / selected, highest priority on top plus "Default"), click to move, arrows to reorder, Done applies and reloads the resources, Open Folder opens `<data dir>/resourcepacks`. `--resource-pack` on the command line replaces the saved list. Missing: the `pack.png` icons (the slot is empty), the confirmation screen for a pack of another `pack_format` (it is tinted red and selects directly), drag scrolling. Texture, model and colormap packs reload live; entity textures and sounds are not pack-aware yet. |

## Audio

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Audio | sound engine, sound packets, music | Missing | P11 | — |  |

## Resources

| Area | Feature | State | Owner milestone | Evidence | Deviation / notes |
|---|---|---|---|---|---|
| Resources | resource packs (stack, reload) | Partial — not verified against vanilla | P7 | — | Animated block textures follow the pack `.mcmeta` (ADR-0023). |
| Resources | language files | Partial | P10 | Unit tests (`umicraft_core::lang`, `umicraft_assets::languages`, `options::language`), guard `no_hardcoded_ui_text` | ADR-0039. Every UI text is a vanilla key resolved by `Lang` (selected language, then `en_US`, then a minimal offline table, then the key); the downloaded `.lang` files are read from the asset index and listed with `pack.mcmeta` names. `[language]` option and its page (`GuiLanguage`: the list from the asset index, a click switches the `Lang` at once) exist; "Force Unicode Font", Unicode font switching and right-to-left text are not done. The locale sent in `Client Settings` is still `en_us`. Migrated: pause menu, death screen, options pages, book/sign screens, tooltips, hover errors. Not yet keyed: debug screens, remaining HUD strings. |

## Approved deviations

| Deviation | Status |
|---|---|
| F2 opens the Umicraft debug screen instead of taking a screenshot (ROADMAP.md §8.2). Vanilla F3 is reproduced in P9; the Umicraft debug data moves to F4 (see the next row). | Superseded by the F4 row |
| P9-g: the Umicraft debug screen moves from F2 to F4 (F3 is the vanilla text screen, F2 stays free for the vanilla screenshot, not implemented); the old F4 resource reload is dropped, F3+T remains. | Approved by the owner (2026-11-02) |
