# Built-in mods (ADR-0065)

| # | Milestone | State |
|---|---|---|
| M1 | Core crate `umicraft-builtin-mods`: types, trait, settings schema, HUD placement and snap, registry, `[mods]` persistence | done |
| M2 | Client bridge: `options/mods.rs`, `mods_runtime.rs`, `ModContext` build, `Action::Mods` shortcut | done |
| M3 | HUD rendering: `ModWidgetView` layer in `umicraft-render`, `mods_hud_view.rs` in the client | done |
| M4 | First mods: fps, coordinates, toggle sprint, cps, ping (one file each). Skipped: keystrokes and the toggle-sprint indicator, they need WASD state in `ModContext`/`ModInput` and a `hud()` that sees the settings | done |
| M5 | Mods overlay: cards, search, categories, generated settings panel, `modern` widgets, `.lang` keys | done |
| M6 | HUD widgets movable in the mods menu: drag, scale, snap, reset | done |
| M7 | Docs (ADR amendments, roadmap, parity), final fmt/clippy/test | done |

M3, M4 and M5 are independent once M1 is frozen. See ADR-0065 for the design.

## M5 notes

- `umicraft-ui` has its own view types (`ModsView`, `ModCardView`, `SettingView`,
  `SettingControl`, `ModValue`) and answers with `ModsOutcome` (`Toggle`, `Set`, `Hud`,
  `Close`); it does not depend on this crate.
- `umicraft-client/src/mods_view.rs` builds the views from the registry (texts through
  `Lang`) and applies the outcomes; `window/mods_menu.rs` then copies the registry's
  config into `options.mods` and saves. The menu is rebuilt from the registry every frame
  while it is open.
- New `modern` widgets: `toggle`, `modern_slider` (`SliderRange`), `tabs`, `search_field`,
  `fit_text`.
- `ModsScreen` is a page machine: landing (the original menu, restored), list, settings. The Mods
  button opens the list; the panel cross and Escape step back; `--hud-demo --hud-view mods` renders
  the landing and `mods_list` the list.
- Fixed after review: Escape steps back a page (`ModsScreen::can_go_back`,
  the window forwards the key to the UI while a page is open), the search placeholder no longer sits
  under the caret, and a settings page taller than the panel scrolls with the wheel (same step and
  bounds as the card list).

## M6 notes

- There is no editor screen (`Screen::HudEditor` was removed). `umicraft-ui/src/menus/mods/hud_layer/`
  holds `HudLayer` and its view types (`HudEditorView`, `HudWidgetView`, `HudEditorOutcome`:
  `Placed`, `Reset`, `ResetAll`); `drag.rs` is the pure mouse logic (hit test, move, corner resize,
  wheel step), `guides.rs` the alignment guides, `toolbar.rs` the small bottom bar (landing only),
  `shield.rs` the zones where a press never grabs a widget (Mods button, cross, open panel, bar).
  The snapping is not in the UI: the client hands in `hud_editor_view::snap`, which calls `snap_rect`.
- `ModsView.hud` carries the widgets; `ModsScreen::show` takes the HUD font, the brand font and the
  `Snap` function. The layer draws between the dark sheet and the page, so the panel covers it.
- `umicraft-client/src/hud_editor_view.rs` builds the widgets from the registry (it reuses
  `mod_widget_views`) and applies the outcomes: `Placed` goes through `HudPlacement::from_rect`
  and `set_placement`, resets through `reset_placement`; `mods_view::apply` forwards `ModsOutcome::Hud`.
  `window/mods_menu.rs` saves the options after each outcome that changes something (a drop or a
  scale step, never a drag frame).
- The mod-widget layer (`ModLine`, `ModWidgetView`, `measure`, `draw_mod_widgets`) lives in
  `umicraft_ui::mod_widget`; `umicraft-render` re-exports the types. While `Screen::Mods` is open the
  HUD leaves the mod widgets out and the menu draws them, where the pointer holds them.
- Gap in the frozen crate: `MIN_SCALE`/`MAX_SCALE` are not exported by `umicraft-builtin-mods`; the
  client reads the range off `HudPlacement::sanitized`.
- `--hud-demo --hud-view mods|mods_list` render the landing and the list with three widgets
  (`hud_demo_mods.rs`).

## Not checked in a real window

Only scripted captures (`--hud-demo --hud-view mods|mods_list`) and unit tests cover the
mods menu and the movable widgets. The mouse feel (drag, wheel, corner handle), Escape, the
save to `options.toml` and the French texts on screen still need a manual pass.
