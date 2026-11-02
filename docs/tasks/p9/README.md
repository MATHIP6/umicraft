# P9 — HUD, chat & social: plan and architecture

Status: **implemented except the items under "Not done"; not compared with vanilla on screen yet** (branch `p9-hud-chat-social`). Decision:
`docs/decisions/0036-p9-hud-chat-social.md`. Roadmap entry: `ROADMAP.md` §P9.

Rule (as in P5 to P8): a step adds a module, existing big files only get
wiring; modules stay under about 400 lines and test their logic against the MCP
rules.

## Layout

```
umicraft-game/src/hud/       vitals, xp, held_name, boss_bar, tab_list, sidebar, team_names
umicraft-game/src/chat/      component, style, lang, events, history, complete, commands, wrap
umicraft-game/src/debug_f3.rs
umicraft-ui/src/styled_text.rs, screens/scale.rs (ScaledResolution)
umicraft-render/src/hud/     state, overlay_views, gui/{hotbar,icons,chat,fade,overlays,...}
umicraft-client/src/hud_view.rs
```

## Steps

| Step | Content | State |
|---|---|---|
| P9-0 | Audit, this plan, ADR-0036 | done |
| P9-a | `render/hud/` split, `ScaledResolution`, `styled_text` | done |
| P9-b | Hotbar with item icons, crosshair, XP, held-item name | done |
| P9-c | Vitals: hearts (styles, absorption, flash, effects), food, armour, air, mount health, jump bar | done |
| P9-d | Chat: `game/chat/`, `.lang` files, full styles, history, Tab, commands, click/hover | done |
| P9-e | Tab list (header/footer, ping, heads), sidebar, teams, below-name and list objectives | done (head skins and heart flash pending) |
| P9-f | Titles, subtitles, action bar, boss bar | done |
| P9-g | Vanilla F3, Umicraft debug on F4 (approved 2026-11-02) | done |
| P9-h | Delete legacy HUD, F3 data feed, `Riding Jump`, scale 1-4 tests, parity rows, sources | done (see "Not done") |

## Audit (starting point, P9-0)

| Feature | Present | Missing |
|---|---|---|
| Hotbar, hearts, food, armour, XP | textured in `hud_gui.rs` | item icons, absorption, flash, wither/poison styles, air, mount |
| Chat | colours, bold, underline, fade, scroll, wrap | italic, hover/click, history, Tab, `.lang` |
| Tab list / sidebar / titles / action bar | basic geometry | heads, team decoration, objectives below name and in list |
| Boss bar | `BossView` exists | verify against `BossStatus`, name tick-down |
| F3 | Umicraft debug on F2/F3 | vanilla F3 content |

## P9-h result

- Deleted: `legacy.rs`, `chat_hud.rs`, `social_hud.rs`, `layout.rs`, `HudLayout`,
  `StatusRow`, `Fill`, `status_fills`, `selected_index`, `draw_hud`,
  `draw_debug_only`, `HudState.debug_lines`, `HudState.experience`. The fade
  helpers moved to `hud/gui/fade.rs`. `health`, `food`, `armor`, `level` and
  `selected_slot` stay: the death screen, the headless report and the hotbar
  read them.
- No GUI atlas: the textured renderer draws chat, titles, sidebar, tab list and
  F3 with the built-in font; the hotbar and the status rows need the sprite
  sheets and are not drawn. The demo captions and the offscreen screenshot use
  the same renderer (`client::caption`).
- F3 data: `client::debug_world` collects entities, particles, chunk cache,
  biome (`umicraft_game::debug_f3::biomes`), light, local difficulty
  (`debug_f3::difficulty`), feet position and the looking-at block on the game
  thread and sends them in the `Frame`; `Renderer::adapter()` gives the
  `AdapterSummary` and `client::process_memory` the memory line.
- `Riding Jump` is sent from `HudTracker::take_jump` as `ClientCommand::RidingJump`.
- GUI scale: the HUD always uses the auto scale. `render::hud::gui::scale_tests`
  draws it at four window sizes that give scales 1 to 4 and checks that every
  rectangle stays on the screen and that the hotbar and crosshair are centred.

## Not done

- Italic shear of chat, held-item name and titles (italic text is upright).
- Player heads in the tab list (needs skins) and the heart flash of the list
  objective.
- Achievement hover in chat.
- Tab completion replaces the word instead of cycling in place.
- The chat scroll position is not kept when new lines arrive.
- The GUI-scale video option (1 to 4, auto) is not wired: needs an options
  field, the options screen and passing `ScaledResolution` to every HUD module.
- Per-zone dirty flags: the single `hud_dirty` boolean stays, see ADR-0036.
- No comparison with vanilla on screen (screenshots at scales 1 to 4, chat and
  scoreboard on a real server) has been done.
