# Mod UI: settings, HUD widgets, keys, commands, chat

What a native mod can show and ask of the player (API 0.3, ADR-0086, F3.4). Everything
is declared during `umi_mod_init`; a declaration made later is refused. All texts are
literal UTF-8 (a mod ships no language file) and are copied by the host. The limits
below are enforced by `umicraft-modhost` (`src/ui/mod.rs`); a declaration over a limit
makes `init` return an error and the mod does not load.

## Settings

`InitCx::toggle`, `slider`, `choice` and `color` declare a setting and return a
`Setting<T>` handle (`Copy + Send + Sync`) whose `get()` reads the current value from
any thread without a lock. The mods menu lists them on the mod's card, in declaration
order. Keys follow `^[a-z][a-z0-9_]{0,63}$`. A slider is clamped and snapped to its
step, an unknown choice or a malformed colour falls back to the default.

## HUD widgets

`InitCx::hud_widget(key, label, Placement, draw)`. The host calls `draw(&mut Hud)` on
the render thread once per frame, only while the mod is enabled and the mods menu is
closed; the widget is drawn with the built-in ones (same look, same mirroring near the
right edge). `Hud::{line, label, cell, tick, reserve}` fill it; the content is cleared
before each call. A draw call has a 0.5 ms budget: a failing or slow draw function
fails the mod. The player can move, scale and reset the widget in the HUD editor
(ids `ext:<mod>:<widget>`); locking a widget is not offered for external mods.

API 1.1 adds two primitives. Both count against the existing caps (256 items of each
kind per widget), and both fall under the same 0.5 ms budget:

- `Hud::outline(x, y, w, h, color, alpha)` (C: `hud_outline`) draws the one-pixel
  edges of a rectangle. They are four rectangles, so an outline needs room for four
  more ticks.
- `Hud::segment(x1, y1, x2, y2, color, alpha, thickness)` (C: `hud_segment`) draws a
  line at any angle. The thickness is in GUI pixels and is clamped to 0.5..=8.

### Widgets on the screen (API 1.1)

`Placement::new(Anchor::Screen).offset(x, y)` (C: anchor `UMI_HUD_ANCHOR_SCREEN` =
9) anchors a widget to the screen itself:

- its coordinates are GUI pixels from the top-left corner of the screen, shifted by
  the offset;
- the scale is always 1;
- there is no panel, no padding and no mirroring;
- the widget is neither listed nor movable in the HUD editor, and a saved layout does
  not move it.

This is the widget for annotations over the world. A `FRAME` intercept projects a
world point with `Frame::world_to_screen`, and the widget draws at the result in the
same frame (`mods/examples/world_marker`).

## Key bindings

`InitCx::key(key, label, default_binding)`. The binding names are the ones of
`options.toml`: a winit key name (`KeyG`, `Digit1`, `ArrowUp`) or `MouseN` (`Mouse4`
is the first side button); empty is unbound. Each enabled mod's bindings are listed
on the controls page under a heading with the mod's name, and can be rebound or reset
there. A press of the bound key or button with no screen open sends `KEY_PRESSED`
(id 9, render thread, payload `key: str`) to that mod only; mods never see other
keystrokes.

If a mod binding shares its key with a built-in action, both fire (as two vanilla
bindings on one key do) and the controls page shows the key in red. If the action
opened a screen first (the chat key, for instance) the mod binding does not fire.

## Commands

`InitCx::command(name, usage, run)` registers `/name`. Names follow
`^[a-z][a-z0-9_-]{0,31}$`; a name already taken by another mod is refused for the
second one. When the player sends a line, enabled mods' commands run first, on the
game thread, under the 1 ms handler budget, and the line is never sent to the server.
A handler that returns `Err` means "bad arguments": the `usage` is printed and the
mod stays enabled. Any other failure (panic, other status,
budget) disables the mod and the player sees a notice. A line that is not one of a
mod's commands goes on as before (a server command, `CHAT_SEND`, ...). Tab completion
of `/par` adds the mods' command names to the server's answer.

## Chat

`InitCx::chat()` returns a `Chat` (`Copy + Send + Sync`) whose `print(&str)` shows a
local line in the player's chat (never sent). It can be called from any thread, also
after `init`. The client delivers the lines once per frame; lines printed while no
session runs are dropped, and a full outbox (64 lines) drops the newest.

## Persistence

The player's choices for each mod are in `<options dir>/mods/<id>.toml`
(`options.toml` sits in the same options dir): `enabled`, `[settings]`, `[placements]`
and `[keys]`. The file is written when the player changes something (menu, HUD editor,
controls page), read after the mods load, and sanitised against the declarations, so a
hand-edited value cannot leave its range. Entries a mod no longer declares are kept for
a later version. A broken or unwritable file is logged and the defaults are used.

A mod the player switched off still loads (so it can be switched back on in the mods
menu, card category "External"); a mod that failed shows as off with a hint and keeps
its switch until the client restarts. A mod whose id equals the id of a built-in mod
(`fps`, `zoom`, ...) is not loaded; the log says why.

## Limits

| What | Limit |
|---|---|
| Label (setting, widget, key binding) | 128 bytes |
| Command usage | 256 bytes |
| Option of a choice setting | 64 bytes, 1 to 32 options |
| Key (binding) name | 32 bytes |
| Text of one HUD item (longer is cut) | 256 bytes |
| Lines, cells, labels, ticks per widget and frame | 256 each |
| `chat_print` line | 1024 bytes |
| Settings per mod | 64 |
| HUD widgets per mod | 16 |
| Key bindings per mod | 32 |
| Commands per mod | 32 |
| HUD draw call budget | 0.5 ms |
| Command handler budget | 1 ms |
