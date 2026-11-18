# 0080. PvP HUD mods: keystrokes, combo counter, armor status, potion effects

Status: accepted

## Context

Four built-in mods (ADR-0065) were asked for: keystrokes, combo counter, armor status
and potion effects. The first mods only drew coloured text lines from a small
`ModContext`. These four need more: the held keys, the fights of the session, the worn
armor and the active effects as data, and, for the display, boxes, pictures before a line
and a second text under it.

## Decision

- **`ModContext` grows by plain `Copy` facts**: `keys` (`KeyState`), `combat`
  (`CombatInfo`: running totals of hits and hurts), `armor` (helmet first, four
  `ArmorPiece`), `held`, `effects` (up to `MAX_EFFECTS`, `EffectInfo`). Mods still never
  reach into the client.
- **`HudContent` also holds boxes and picture lines.** `cell(rect, pressed, color)`
  writes a box at a fixed place (keystrokes); `icon_line(icon, color)` writes a line with a
  `HudIcon` before it and a `detail` text under it. A mod only *names* the picture
  (`Armor(slot)`, `Held`, `Effect(id)`); the client resolves it (`mod_icons.rs`), so the mods
  know nothing about textures. A widget uses lines or boxes, not both.
- **Widgets of icons start without a background** (`hud_settings_bare!`, the shared
  options with `background` off); the other mods keep the panel by default.
- **Pictures come from the player's assets** (spec §3): item icons through the same icon keys
  as the hotbar (`ItemView::slot_item`), potion status icons from the grid of
  `container/inventory.png` already used by the inventory effect list. Nothing is bundled.
- **Mirroring.** A widget whose centre is on the right half of the screen draws its rows
  as text then picture, right-aligned, so the pictures stay against the screen edge.
- **Hits are confirmed, not assumed.** The server never says a hit landed. The game thread's
  `CombatLog` counts a hit when the entity just attacked is hurt (`Entity Status 2` or
  `Animation 1`) within 10 ticks of the attack; it counts a hurt when the local player is
  hurt (status, animation or a drop of health). A hurt may count twice for one real hurt; the
  combo only needs the total to change.
- **Data path.** The hit and hurt totals and the active effects travel with `HudState`
  (`combat`, `active_effects`); the armor and the held item are read on the render thread from
  the published player window slots (`mods_facts::equipment`: slots 5 to 8, helmet first,
  then hotbar slot of the selected index); the keys come from the key bindings.
- **Ids and keys follow ADR-0065.** `keystrokes`, `combo_counter`, `armor_status`,
  `potion_effects`; language keys `umicraft.mods.<id>.*` in `en_US.lang` and `fr_FR.lang`.

## Consequences

- Effect times are those the HUD snapshot carries, so they change once a second like the
  inventory effect list; the blink of the last ten seconds uses the frame clock.
- The server-less `--hud-demo` shows the widgets without pictures (no session, no assets).
- A future reach mod would add its own field to `CombatInfo` (append-only).

## Amendments

- **Invert option.** Armor status and potion effects have an `invert` toggle (off by
  default) that swaps the side of the picture and the text: the client mirrors a widget
  on the right half of the screen on its own, and `invert` flips that choice for the widget
  (`HudContent::invert`, read in `mods_hud_view`).
- **Test effect in the mods menu.** While the mods menu is open, a potion effects widget that
  has nothing to show draws a test effect (Speed II, 1:23) through
  `ModRegistry::preview`, so the player has something to place. The HUD itself still shows
  only real effects.
- **Menu icons.** Four Phosphor icons (MIT, same set as the others, regenerated with
  `docs/tools/gen_icons.py`): `keyboard`, `lightning`, `shield`, `flask`.
