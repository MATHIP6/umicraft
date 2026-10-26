# ADR-0017 — M10: Chat, scoreboard, settings and keybinds

- Status: Accepted for W0 (2026-10-26); W1 to W6 still to implement
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
