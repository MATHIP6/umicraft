# ADR-0095 — Modding API 1.2: categories, modules and buttons in the mods menu

Status: accepted (2026-11-23)

## Context

With API 1.1 an external mod gets exactly one card, under the single "External"
tab, with toggle, slider, choice and colour settings. Mods that bundle several
features ("client suites") cannot offer what the built-in mods have: one card and
one switch per feature, grouped under a tab of their own. Nor can a setting trigger
an action (reset a counter, reload a list): there is no button.

The owner decided (2026-11-23):

- a mod **cannot** put cards in the built-in tabs (HUD, Gameplay, Visual);
- a mod **may create** tabs of its own;
- a button only notifies the mod (no state shown on the card).

## Decision

API **1.2**, append-only as in ADR-0087. The exact types are frozen in
`docs/tasks/api-1.2/README.md`.

### Categories

`register_category(key, label)` (init only) declares a tab. At most
`MAX_CATEGORIES` (2) per mod; the label is at most 32 bytes. A tab is shown only
when one of the mod's modules uses it, after the built-in tabs and "External", in
load order. Tabs of different mods with the **same label** are merged, so a family of
mods by one author can share one tab. A category is never a built-in tab: there is
no way to name one.

### Modules

`register_module(UmiModuleDef)` (init only) declares a module: a card of its own,
with a key, a label, an optional description, an optional category (a key of this
mod's categories; empty means the "External" tab) and a default state. At most
`MAX_MODULES` (16) per mod. The mod's own card stays in "External": it is the
mod's master switch and carries version, permissions, hash and failure.

A module's switch is the player's choice, persisted in the mod's config file under
`[modules]`. It is lock-free (`module_enabled` from any thread). The module runs
only while its mod runs; while the mod is off the module card says so but keeps its
switch, as a built-in mod keeps its settings.

`MODULE_TOGGLED` (id 14) tells the mod a switch changed. Like `KEY_PRESSED` it is a
render-thread event aimed at the owning mod only.

`module_attach(module, kind, key)` ties a HUD widget or a post-process pass to a
module: it is drawn only while the module is on, and the HUD editor hides it while
off, the same way a built-in mod's widget follows its mod. Event handlers, key
bindings, commands and service layers are not attached: the mod checks
`module_enabled` itself (a wrapping layer calls `next` and returns). Attaching
everything would need a module argument on every registration, for little gain.

### Settings and buttons on a module

`register_setting_in(module, def)` declares a setting on a module's card instead of
the mod's. Keys stay unique per mod, so `setting_get` and the config file are
unchanged. `UmiSettingDef` is **not** extended: a 1.1 host checks `size` against the
1.1 struct, and a separate function keeps every existing call valid.

`register_button(module, key, label, text)` declares a button row: `label` on the
left, a button reading `text` on the right. A click sends `BUTTON_PRESSED` (id 15)
to the owning mod (render thread, targeted, like `KEY_PRESSED`). A button has no
value: it is not saved, and `setting_get` answers `INVALID_ARGUMENT` for its key.

### The tab row

Tabs were laid out at their natural width and could run past the panel. When they
do not fit, every tab now gets the same share of the width and its label is cut with
an ellipsis; the full name is still in the cards' search.

## Consequences

- A 1.2 mod is refused by a 1.1 host (its `api_version` is 1.2); a 1.0/1.1 mod loads
  on a 1.2 host unchanged, with one card as before.
- Cards of modules have ids `ext-module:<mod>:<module>`, never mistaken for a mod's
  card or a widget.
- No new permission: these are menu entries, they do not change what the server
  sees.
