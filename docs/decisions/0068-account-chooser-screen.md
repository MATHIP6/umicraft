# ADR-0068 — Account chooser screen

Status: accepted (2026-11-09)

## Context

F1 adds accounts (offline and Microsoft). The UI needs a chooser reachable from
the server list, without the UI crate knowing anything about authentication.

## Decisions

- **Pure screen in `umicraft_ui::menus::accounts`**, in the style of ADR-0042:
  `AccountsScreen::show(ui, atlas, lang) -> Option<AccountsOutcome>`, public
  pure layout functions, `tick()` counters, no I/O. The UI owns only view
  models (`AccountRow`, `AccountBadge`, `DeviceCodePrompt`); the client maps
  its account store to them and acts on the outcomes.
- **Dialogs replace the screen**, like vanilla screens: Add (offline name with
  live validation `[A-Za-z0-9_]{1,16}`, or Microsoft), Edit (offline only),
  Delete (the existing `ConfirmScreen`) and the Microsoft device code (shown
  while the caller sets it, Cancel emits `CancelLogin`).
- **Heads are a flat placeholder square** until the client can hand the UI a
  skin texture; the slot keeps the row layout final.
- **Server list entry point**: a button at the top right
  (`ServerListOutcome::OpenAccounts`) labelled with the active account name
  set by `ServerListScreen::set_active_account`.
- **Texts** are `umicraft.accounts.*` keys in the client pack (en_US, fr_FR);
  tests carry the few they assert on (`lang_with_account_texts`).
