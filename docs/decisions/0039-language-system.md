# ADR-0039 — Language system

Status: accepted (2026-11-04)

## Context

UI texts were English literals spread over `umicraft-ui` and the client
(pause menu, death screen, options pages, book and sign screens, tooltips).
They could not follow the language option, and the vanilla wording was
copied into the repository (spec §3 allows the downloaded `.lang` files only).

## Decisions

- **Keys in code, text from files.** Code holds vanilla translation keys
  (`menu.returnToGame`, `key.forward`, ...). The texts come from the
  `minecraft/lang/<code>.lang` objects of the first-run download; none is
  stored in the repository.
- **`umicraft_core::lang`.** The lowest crate both the UI and the game can
  use. It owns the `.lang` parser and the format expansion (moved from
  `umicraft_game::chat::lang`, which re-exports them) and the `Lang` handle:
  `tr(key)`, `tr_args(key, &[..])` (`%s`, `%d`, `%1$s`, `%%`), `get(key)`.
  `Lang` is one merged table built in fallback order: offline `builtin`, then
  `en_US`, then the selected language; a key found nowhere returns the key
  (`I18n.format`). The `builtin` table is an offline/test fallback only: the
  pause and death screens, the chat system messages and the few texts vanilla
  hard-codes (`Super Secret Settings...`, `Invalid Item!`, the durability
  line) under `umicraft.*` keys.
- **`umicraft_assets::languages`.** Lists the languages of the cached asset
  index (names and regions from the `pack.mcmeta` object, else the file name)
  and loads one `.lang`; typed errors (`UnknownLanguage`, `Missing`,
  `HashMismatch`). A code is validated (`ll_RR`) before it names an object.
- **Option and client.** `[language] code = "en_US"` in `options.toml`
  (malformed codes return to the default). The client builds an `Arc<Lang>`
  at startup, keeps it in `App`, and installs it as the shared current
  language (`client::language`) for the game thread (chat components,
  overlays, tooltips). Switching language is one `install` of a new handle;
  the options page for it is a later P10 step.
- **Screens take `&Lang`.** `GameMenuLabels::new(&Lang)` borrows its texts,
  `death_screen`, `OptionsScreen::show` and the text screens take a `&Lang`.
- **Guard.** `crates/umicraft-client/tests/no_hardcoded_ui_text.rs` scans the
  production code of `umicraft-ui` and `umicraft-client` for a denylist of
  vanilla UI literals.

## Consequences

- Units vanilla itself hard-codes (`" chunks"`, `" fps"`, `%`, `px`) stay in
  code; they have no key.
- Without the downloaded files only the `builtin` keys read as text; other
  keys show as themselves.
- Item and block names of the registry that are not vanilla keys are still
  humanised from the key (`tooltip_draw`).
