# 0084. Discord Rich Presence

Status: accepted

## Context

The client should show on Discord what the player is doing. Discord is an
optional desktop program: the client must work the same without it.

## Decision

- **Crate.** `discord-rich-presence` 1.1 (MIT, pure Rust, talks to the local IPC
  socket; compatible with the project licence, spec §3). No Discord SDK, no
  native library.
- **Own thread.** The IPC calls block, so `discord::Presence` hands each change
  to a `DiscordRpc` thread over a channel. The window calls `show` every loop
  turn; only a change is sent.
- **Quiet failures.** If Discord is not running, the worker retries every 15 s
  and logs at `debug`. Nothing reaches the player.
- **Coarse status.** Only "In the menus", "Connecting to a server" and "Playing
  on a server", plus the time since launch. No server address, name, account or
  coordinates: privacy by default. Images and buttons are left for later.
- **No option yet.** The presence is always on; an option or flag can be added
  when someone asks to turn it off.

## Consequences

- One more dependency (`uuid` 0.8 comes with it).
- `Status::of` and the worker loop are tested with a fake link; the real
  socket is not exercised by tests.
