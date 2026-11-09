# 0065. Built-in mods

Status: accepted

## Context

The client has a mods menu (Right Shift, ADR-0056) with no mods behind it. The
player should be able to switch on and configure features of the client itself (FPS
counter, position, toggle sprint...) and, for the ones that draw on the HUD, move and
resize them. These are *built-in* mods, compiled into the client. They are distinct
from the external mods with an API (F3), whose design is still open (spec 8.4 and
ADR-0015).

`game.rs`, `window/mod.rs` and `renderer.rs` are already thousands of lines; the
feature must not grow them.

## Decision

- **A new crate, `umicraft-builtin-mods`**, with no GPU, window or network
  dependency. It stays testable on its own and is clearly not the F3 API: nothing in
  it is public API for external mods.
- **Data-driven mods.** A mod implements `BuiltinMod`: `meta`, a settings schema
  (`SettingDef`), and optionally `hud`/`draw` (text lines in a reused `HudContent`)
  and `modify_input`. The settings page is generated from the schema and one generic
  renderer draws every widget, so a new mod is one small file in `mods/` plus a line
  in `mods::all()`.
- **Read-only context.** Mods receive a `Copy` `ModContext` built once per frame;
  they never reach into the client.
- **Placement is resolution independent**: one of nine anchors, an offset in GUI
  pixels and a scale (0.5 to 3). A widget is always kept on screen.
- **Persistence** in `options.toml` as `[mods.<id>]` (`enabled`, `settings`,
  `placement`). The file is user-editable, so every value is sanitized (clamped,
  snapped, falls back to the default); tables of unknown ids are written back so
  switching versions loses nothing. Setting keys and mod ids are append-only.
- **Per frame, not per tick.** Mods update once per rendered frame, with the click
  count since the previous frame, so a counter such as CPS has one time base.
- **Client glue stays thin**: `options/mods.rs` (the section), `mods_runtime.rs`
  (owns the registry, builds the context), three calls in `window/mod.rs`.

## Consequences

- The widget-moving logic (`HudPlacement::rect`/`from_rect`, `snap_rect`) is unit tested
  here; the UI only wires it to the mouse.
- A mod that needs data the context lacks adds a field to `ModContext` (one place).
- `ModInput` only has the fields the first mods need and grows with them.

## Amendments (as built)

- **Widget layer lives in `umicraft-ui`** (`mod_widget.rs`: `ModLine`, `ModWidgetView`,
  `measure`, `draw_mod_widgets`), not in `umicraft-render`. `render` depends on `ui`, and
  the mods menu must preview the widgets with the HUD's own drawing code; `render`
  re-exports the types. The UI never depends on `umicraft-builtin-mods`: it works on
  view types, and the client converts (`mods_view.rs`, `hud_editor_view.rs`,
  `mods_hud_view.rs`).
- **Language keys.** A mod uses `umicraft.mods.<id>.name`, `.desc` and
  `umicraft.mods.<id>.<setting>`; the option of a choice is `<label_key>.<option>`.
  A test in `mods/mod.rs` checks that every key exists in `en_US.lang` and `fr_FR.lang`.
- **Shortcut.** The mods menu opens with `Action::Mods` (default Right Shift), which is
  rebindable. Left Shift stays sneak.
- **The old mods menu is kept.** `Screen::Mods` is a small state machine of pages
  (`ModsScreen`): the *landing* (logo, "UMICRAFT" wordmark, big Mods button, close cross,
  game blur and opening animation, as before the built-in mods), then the *list* (cards,
  tabs, search, in a panel), then the *settings* of one mod. The Mods button opens the
  list; the cross of the panel and Escape step back (settings, list, landing); Escape on
  the landing and its cross close the menu. The window asks `can_go_back()` before it takes
  Escape. A first version of this work replaced the landing by the list and added a separate
  `Screen::HudEditor`; that was a misreading of the request and was reverted.
- **Amended: a mod can keep a small state between runs.** `BuiltinMod` has `state`,
  `load_state` and `take_state_changed`; the registry saves it as `[mods.<id>.state]` (same
  lenient reading as the settings, keys append-only) and the window saves the options as soon
  as a mod reports a change, not only from the menu. Toggle sprint uses it for its switch
  (`toggled`), so sprint is still toggled after a restart.
- **Amended: the landing page is gone.** The Mods button of the landing only opened the
  list, one click for nothing, so the menu now opens on the list (`ModsScreen` has the
  pages list and settings). Escape steps back from the settings and closes the menu from the
  list; the cross of the panel closes it. The logo moved into the panel header and the line
  on how to move the widgets sits above the panel, only while a HUD widget is on. The reset
  bar stays under the panel. `--hud-view mods_list` is kept as another name for `mods`.
- **HUD widgets move directly in the mods menu.** There is no editor screen. The widgets of
  the enabled HUD mods are drawn by the menu (`menus/mods/hud_layer`, `HudLayer`) over the
  dark sheet and under the panel, on every page, and are dragged with the mouse anywhere
  but on a button or on the open panel (a press there never starts a drag). Snapping,
  guides are unchanged; the corner handle has no step (the scale follows the pointer to any
  value in range) and a wheel notch is 0.05; the sheet and the blur are the
  menu's own. Changes leave as `ModsOutcome::Hud(..)` (`Placed`, `Reset`, `ResetAll`) and
  are applied by `hud_editor_view.rs`; the options are saved on a drop or a scale step, not
  per drag frame. A small bar with "Reset" and "Reset all" shows on the landing only. The
  normal HUD leaves the mod widgets out while `Screen::Mods` is open.
- **A look shared by the HUD mods.** Every HUD mod lists, after its own options, seven
  shared ones (`mods/hud_style.rs`, language keys `umicraft.mods.hud.*`): background on or
  off, its color and opacity, the corner radius, the padding, and the shadow of the text
  (vanilla, a custom color, or none). The registry reads them into `HudContent::style`
  after each `draw`; the client turns it into a `WidgetStyle` of the widget layer, which
  also measures with it (the padding changes the size). Fps, coordinates, cps and ping
  also have a text `color`. The defaults are the look the widgets had before.
- **A gear on each widget.** The frame of a widget in the HUD layer is a little larger than
  its text on every side and taller by a strip under the last line. While the pointer is over
  the widget, or the widget is selected, a gear (`gear.svg`, `Icon::Gear`) shows in that strip
  at the left, so it never covers a line: the icon alone, no plate, half faded. Its edge is 16 %
  of the smaller side of the text box (never under 4 GUI units), so it follows the box and the
  scale of the widget. A widget so low that the strip would fall off the bottom of the screen
  gets it above the text instead (gear at the top left), and the frame is always pushed back
  inside the screen, so the corner handle and the gear stay in view at every edge. The frame is what the pointer grabs, and the corner handle is at its
  corner; the placement and the snapping stay those of the text. A press on the gear never
  starts a drag; it answers `HudEditorOutcome::OpenSettings`, which the menu takes for itself
  and turns into the settings page of that mod. It never reaches the client.
- **Fullbright.** A visual mod with no setting (`fullbright`, off by default, `Icon::Lightbulb`).
  The client tells the renderer each frame whether it is on (`ModsRuntime::fullbright`,
  `Renderer::set_fullbright`); the renderer then makes every entry of the lightmap white, so
  block light, sky light, the night and the brightness option no longer matter. The shade of
  the faces and the ambient occlusion stay, so the world keeps its volume; the fog and the
  sky are not touched.
- **Scrolling.** The scrollbar of the card list and of a settings page (`scrollbar.rs`) can be
  grabbed: a press on the thumb holds it where it was caught, a press on its track (a zone of
  7 GUI units, wider than the thumb) jumps there, and the drag follows the pointer outside the
  bar until the release. The wheel and the thumb set the target offset; the list draws a glide
  to it (`Motion::glide`: a critically damped spring, no overshoot, settled in about a quarter
  of a second), except while the thumb is held, when it follows at once. A settings page
  starts at the top without gliding from the previous mod's offset.
- **Known limits.** `keystrokes` and the toggle-sprint indicator need WASD state in
  `ModContext`/`ModInput` and a `hud()` that sees the settings. The directions of
  `coordinates` are not localized (the mods do not see the language). `ping_ms` is
  `None` until the client measures the ping, and `sprinting` is the key state, not the
  player's.
