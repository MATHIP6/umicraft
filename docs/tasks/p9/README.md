# P9 — HUD, chat & social: plan and architecture

Status: **in progress** (branch `p9-hud-chat-social`). Decision:
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
umicraft-render/src/hud/     state, layout, legacy (to delete), gui/{hotbar,icons,chat,overlays,...}
umicraft-client/src/hud_view.rs
```

## Steps

| Step | Content | State |
|---|---|---|
| P9-0 | Audit, this plan, ADR-0036 | done |
| P9-a | `render/hud/` split, `ScaledResolution`, `styled_text` | in progress |
| P9-b | Hotbar with item icons, crosshair, XP, held-item name | done |
| P9-c | Vitals: hearts (styles, absorption, flash, effects), food, armour, air, mount health, jump bar | done |
| P9-d | Chat: `game/chat/`, `.lang` files, full styles, history, Tab, commands, click/hover | done |
| P9-e | Tab list (header/footer, ping, heads), sidebar, teams, below-name and list objectives | done (head skins and heart flash pending) |
| P9-f | Titles, subtitles, action bar, boss bar | done |
| P9-g | Vanilla F3, Umicraft debug on another key | done (lines, drawing, keys; data feed partial) |
| P9-h | Delete legacy HUD, switch `game.rs` to zone views, scale 1–4 captures, parity rows, sources | todo |

## Audit (starting point, P9-0)

| Feature | Present | Missing |
|---|---|---|
| Hotbar, hearts, food, armour, XP | textured in `hud_gui.rs` | item icons, absorption, flash, wither/poison styles, air, mount |
| Chat | colours, bold, underline, fade, scroll, wrap | italic, hover/click, history, Tab, `.lang` |
| Tab list / sidebar / titles / action bar | basic geometry | heads, team decoration, objectives below name and in list |
| Boss bar | `BossView` exists | verify against `BossStatus`, name tick-down |
| F3 | Umicraft debug on F2/F3 | vanilla F3 content |
