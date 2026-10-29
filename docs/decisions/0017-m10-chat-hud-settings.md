# ADR-0017 — M10: Chat, scoreboard, settings and keybinds

- Status: Accepted (2026-10-26); implemented in W0 to W6, see "Implementation notes" for what is left
- Date: 2026-10-26
- Deciders: project owner
- Supersedes: none
- Related: ADR-0005 (game thread), ADR-0006 (M4 UI foundation), ADR-0014 (M7
  assets), ADR-0015 (M8 entities), ADR-0016 (M9 inventory)
- Plan: [`docs/tasks/m10/README.md`](../tasks/m10/README.md)

## Context

M10 (spec §12.2; GAM-6, GAM-7, UI-3) adds the full chat, the player list, the
scoreboard, the titles, the action bar, the boss bar, the settings screens and
the rebindable keys. Before this work the windowed client had:

- one boolean per screen in `window.rs` (`chat_open`, `inventory_open`,
  `show_debug_screen`), re-tested as `a || b || c` in every handler;
- a chat reduced to a plain string with no caret and a flattened JSON line;
- keys hard-coded as `KeyCode` literals;
- no user options file; `umicraft-core::ConfigStore` existed but nothing used it.

## Decisions

The first five record the choices made when the plan was approved.

1. **Options storage.** `Options` is serialised to TOML through
   `umicraft-core::ConfigStore`, one section per area (`[video]`, `[keys]`,
   `[chat]`, `[audio]`). This is the first user of `ConfigStore`. Writes are
   atomic (temporary file, then rename); a missing or corrupt file falls back
   to the defaults without failing. *Revisit if ADR-0005's deferral of the core
   registry to M15 makes the dependency unwelcome: the section API is small and
   can be replaced by `serde` + `toml` inside `umicraft-client`.*
2. **`open_url` click events are never followed.** The server controls the
   chat, so the component is parsed and kept but the client does not open the
   address. `suggest_command` and `run_command` act only on a click by the
   player. A confirmation dialog may be added later.
3. **A minimal game menu ships in M10.** Escape opens a menu with "back to
   game" and "quit" so the options screen has an entry point. The full pause
   screen (server list, account switch) stays in M12.
4. **TOML, not `options.txt`.** The options file is Umicraft's own format.
   Compatibility with the vanilla `options.txt` is not a goal.
5. **Hover shows text only.** `show_text` is rendered as a tooltip;
   `show_item` and `show_entity` are deferred.

6. **One current screen.** `Screen` (`crates/umicraft-client/src/screen.rs`)
   is `None`, `Chat`, `Inventory`, `GameMenu` or `Debug`. It replaces the three
   booleans. The pure type answers `uses_ui` (does the immediate-mode UI get
   the pointer and keys), `blocks_game_input`, and the transitions
   `after_escape` and `after_debug_toggle`. A container opened by the server
   is shown by the inventory screen without setting the field: `App::screen()`
   derives it from the world view.
7. **Every screen change resets the pending input.** `App::set_screen` clears
   the held keys, the look delta, the queued clicks, the typed chat line and
   the UI event queue, and releases the cursor for any screen that blocks the
   game. A key released while another screen owned the keyboard no longer stays
   "held".
8. **Escape.** With no screen it opens the game menu. It closes the chat, the
   inventory and the menu. The debug screen is closed with F2 only, because
   its widgets may consume Escape. Leaving the client is now an explicit menu
   entry; before, a second Escape with the cursor released closed the window.
9. **Pure models in `umicraft-game`, pure screens in `umicraft-ui`.** The chat
   log, player list, scoreboard, titles and boss tracker are I/O-free types in
   `umicraft-game` (like `inventory.rs` in M9). `GameMenu` follows the
   `DebugScreen` pattern: it draws through a `Ui` and a `FontAtlas` and returns
   the player's choice as a value.
10. **Boss bars use the 1.8 mechanism.** Protocol 47 has no boss-bar packet:
    the bar is the health of a Wither or Ender Dragon entity. The tracker reads
    the entity store of M8.
11. **Untrusted server data is bounded.** The chat JSON parser caps recursion
    depth and text length, and a malformed component falls back to its raw text
    (as `chat::plain_text` does today).

## Consequences

- W0 changes no gameplay: the same three screens open and close the same way,
  plus the menu. The only visible differences are the menu and the cleared
  input on screen changes (the second removes a stuck-key case).
- A left click while the chat is open no longer re-captures the mouse (it did
  before, because the chat fell through to the game branch).
- W1 to W5 can run in parallel once this lands: they add packets, models,
  widgets and options, and touch `window.rs` only to add a `Screen` variant
  and one match arm each.
- The wire ids of the new packets are taken from memory of protocol 47 in the
  plan and **must be checked against MCP** and cited in `docs/sources.md`
  before W1 (ADR-0001).

## Out of scope

Audio behind the audio tab (M11), accounts and the server list (M12), mods
(M15), `show_item` and `show_entity` tooltips, glyphs outside the ASCII font
page, gamepad input.

## Implementation notes (2026-10-26)

What was decided above and how it landed, and what differs from the plan.

- **Options storage (decision 1).** `ConfigStore` is now used: by
  `crates/umicraft-client/src/options.rs`, in the sections `[video]`,
  `[controls]`, `[chat]` and `[keys]` of `options.toml` in the Umicraft data
  folder (`umicraft_assets::cache::data_dir`). `ConfigStore::save` became
  atomic (temporary file, then rename). A section that does not parse is
  replaced by its defaults without touching the others, and every number is
  clamped. The registry and the event bus of `umicraft-core` stay unused.
- **Key bindings.** 20 actions (movement, sprint, sneak, jump, inventory, chat,
  command, tab list, hotbar 1 to 9). `Escape` and `F1` to `F4` are reserved
  and cannot be bound; two actions on one key are allowed and flagged on the
  controls screen. The attack and use buttons are fixed on the mouse.
- **`--view-distance`.** A value other than its default (8) overrides the saved
  render distance at start-up, because the argument has no "not given" state.
- **What the server is told.** The render distance and the chat visibility and
  colours are sent in `Client Settings` at login (`PlayOptions.chat_mode`) and
  again when the options screen closes, only if they changed. Sending them
  while a slider is dragged would flood the server.
- **The HUD uses textured rendering and the official font.** When the official
  client jar is present, the HUD is rendered via `umicraft_render::hud_gui`
  into a `DrawList` with textures from `widgets.png` and `icons.png` (hotbar,
  selector, hearts, food, armor, XP bar & level in 0x80FF20 with black outline,
  crosshair, ping bars, boss bar). If the jar or textures are missing, it falls
  back to `draw_hud` with the built-in 3x5 font on the overlay.
- **Textured UI buttons and sliders.** `umicraft-ui` widgets (`Button` and
  `Slider`) support an optional `widgets.png` skin (tiled in two halves to fit
  any dimensions, with custom 8px slider handles). Tab button labels
  automatically adapt text scale to prevent any clipping under proportional fonts.
- **Boss bar implemented.** `BossTracker` (`umicraft_game::entity::boss`) reads
  DataWatcher float index 6 from the Ender Dragon (max health 200.0) or Wither
  (max health 300.0) in `EntityStore`. It drives `HudState.boss` which is drawn
  at (w/2-91, 12) from `icons.png` with centered name.
- **`en_US.lang` loaded at startup.** Read from the Umicraft asset cache or
  directly from `1.8.8.jar` (`assets/minecraft/lang/en_US.lang`) and merged
  into `Game.translations` via `extend` (builtin table as fallback).
- **Chat clicks and hover.** `clickEvent` and `show_text` are parsed and kept
  on each run (decisions 2 and 5), but the chat has no pointer interaction, so
  nothing acts on them yet and `open_url` is never followed.
- **Not verified against a live server.** Everything is covered by unit tests
  and by the server-less `--hud-demo`, which pushes scripted packets through
  the real game code and renders the result; no session on Paper or
  PandaSpigot was run.
