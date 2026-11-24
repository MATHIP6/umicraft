# Writing mods for Umicraft

Guide and index of the modding API (ADR-0086, ADR-0087, spec §8). The API is
version **1.3** (1.0 frozen in F3.9, 1.1 appended in ADR-0094, 1.2 in ADR-0095, 1.3 in
ADR-0096): a
mod built for 1.y loads on every client offering 1.x with x ≥ y, and nothing a 1.0
mod uses is ever removed or moved within major 1. API 1.1 added the camera of `FRAME`
and `world_to_screen`, screen widgets with outlines and segments, entity facts and the
render-thread snapshot, scene depth in post passes, flight and game mode from a
physics layer, and the `render_camera` service with `PRE_RENDER`. API 1.2 added tabs
of a mod's own, modules (one card and switch per feature) and buttons in the mods
menu. API 1.3 lets a camera layer capture the movement keys and the mouse look, for a
free camera.

## What a mod is

A mod is a native dynamic library (`.so`, `.dll`, `.dylib`) plus a `mod.toml`
manifest. It talks to the client through a versioned C ABI, so it can be written in
Rust (the `umicraft-modapi` crate) or in any language that speaks C. It runs inside
the client process with your user's full rights: **it is not sandboxed**. The
declared permissions are shown to the player and the host enforces only the two
that gate API calls (below). Install only mods you trust.

## Getting started

1. Copy `mods/template` (see its [README](../../mods/template/README.md)): one
   setting, one tick observer, a shutdown message, every line commented.
2. Rename the id, the library and the folder (the folder must be named after the
   id), build with `cargo build --release`.
3. Install into `<data dir>/mods/<id>/` (`mod.toml` and
   `bin/<os>-<arch>/<library file>`), then trust the library (mods menu or
   `<data dir>/mods/trusted.toml`). `--safe-mode` starts without any mod.

The data dir is the `umicraft` folder of the platform data directory
(`~/.local/share/umicraft`, `~/Library/Application Support/umicraft`,
`%APPDATA%\umicraft`). `<os>-<arch>` is `linux-x86_64`, `linux-aarch64`,
`windows-x86_64`, `windows-aarch64`, `macos-x86_64` or `macos-aarch64`; the file is
`lib<library>.so`, `<library>.dll` or `lib<library>.dylib`. Richer examples are in
[mods/examples](../../mods/examples/README.md).

## Manifest reference (`mod.toml`)

Unknown keys are errors, so a typo is reported instead of ignored.

### `[mod]` (required)

| Key | Type | Default | Meaning |
|---|---|---|---|
| `id` | string | required | Unique id, `^[a-z][a-z0-9_]{0,63}$`. The mod's folder must have this name. |
| `name` | string | required | Display name, not empty. |
| `version` | string | required | The mod's own semantic version. |
| `authors` | array of strings | required | Author names (may be empty). |
| `api_version` | string | required | `major.minor`, e.g. `"1.3"`: the same major as the client and a minor no higher than the client's. (Before 1.0 the minor had to match exactly.) |
| `license` | string | none | SPDX expression. |
| `description` | string | none | One line for the mods menu. |
| `library` | string | the id | Library file stem under `bin/<os>-<arch>/`; letters, digits, `_`, `-`. |

### `[dependencies]`

`other_id = "<semver requirement>"`, such as `">=0.2, <1.0"`. Default: none. A
dependency that is missing, has a version outside the requirement or cannot load
itself keeps this mod from loading; so does a dependency cycle or depending on
itself. Dependencies load first.

### `[load]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `priority` | integer | `0` | Lower loads earlier among mods with no dependency between them. |
| `replaces` | array of service ids | `[]` | Services this mod replaces entirely; a second mod replacing the same one does not load. |
| `wraps` | array of service ids | `[]` | Services this mod wraps; any number of mods may, chained in load order. |

A service id matches `^[a-z][a-z0-9_.]{0,63}$`. The services are `player_physics`
(see [world.md](world.md)) and, since 1.1, `render_camera` (see
[camera.md](camera.md)); an unknown one is refused before the library is opened. A service may not be listed twice, nor in both lists.

### `[permissions]` (all `false` by default)

| Key | What it does |
|---|---|
| `network_packets` | Gates the packet events (`PACKET_RECEIVED`, `PACKET_SEND`) and packet injection. Without it the host answers `DENIED`. |
| `movement` | Gates `player_physics` (wrapping or replacing it). |
| `camera` | Gates `render_camera` (API 1.1). Not held off by server-compatible mode: it changes what is drawn, not what the server sees. |
| `filesystem` | Shown to the player only ("files"). |
| `network_http` | Shown to the player only ("http"). |

In server-compatible mode (`multiplayer.mod_server_compat`, one client switch) the
host holds off every mod that declares `network_packets` or `movement`, so players
can follow a server's rules. The mods menu marks those with an "alters
packets/movement" badge.

## Lifecycle

- **Init.** `init` runs once at load and registers everything (events, settings,
  widgets, keys, commands, physics layers, post passes). A registration after
  `init` returns is refused, and an `init` that returns an error or panics leaves
  the mod unloaded.
- **Disable is a flag.** Turning a mod off in the menu only stops its handlers.
  `shutdown` runs once, at client exit, after every thread that dispatches to mods
  has stopped; the library stays mapped until then.
- **Failure.** A panic in a handler, or too many budget overruns (below), switches
  the mod off until the next start; the first reason is kept and shown in the
  manager, and the player gets a notice naming the mod.
- **Safe mode.** `--safe-mode` loads no external mod. The client also enters it
  when the previous run did not exit cleanly: a sentinel file in the data dir
  (`umicraft.running`) exists exactly while the client runs, so a crash inside
  native code is caught on the next start. Only one instance owns the sentinel.
- **Trust.** A library loads only if its SHA-256 is in `trusted.toml`; a rebuilt
  library must be trusted again.

## Threads and budgets

Handlers run on the thread that owns the event and must be quick.

| Thread | What runs there | Budget |
|---|---|---|
| game | `PRE_TICK`, `POST_TICK`, `CHAT_*` intercepts, commands, physics layers; the world queries answer only here | 1 ms per call |
| render | `CONNECTED`, screen and key events, `FRAME`, `PRE_RENDER`, camera layers, HUD widget draws | 1 ms per call; 0.5 ms for a widget's draw function |
| network | packet intercepts | 1 ms per call |
| worker | one per mod: every `observe` handler | none; its queue holds 256 events |

A call over its budget is an overrun (logged on the first, then every 100th).
**50 overruns in a row** switch the mod off, counted separately for event handlers,
widget draws and commands. An observation that finds the worker's queue full is
dropped and counted, never waited for. Counters feed the F3 profiler.

## Pages

- [events.md](events.md): the event table, threads, cancellation, screen names.
- [ui.md](ui.md): settings, HUD widgets, key bindings, commands, chat, and since 1.2
  tabs, modules and buttons in the mods menu.
- [packets.md](packets.md): raw packet events and injection (needs `network_packets`).
- [world.md](world.md): block and entity queries, the `player_physics` layer.
- [post.md](post.md): WGSL post-process passes over the picture of the world.
- [camera.md](camera.md): the `render_camera` service and `PRE_RENDER` (API 1.1), and
  the input capture of a free camera (API 1.3).

## Safety and fair play

- Native mods are not sandboxed (spec §8.6); there is no token API, so a mod cannot
  read account credentials through the API, but native code can do anything the
  user can.
- The client never downloads or loads a mod because a server asks for it.
- Umicraft ships no cheating features, and the official examples have none
  (spec §10): `physics_tweak` only measures. Using mods against a server's rules
  can get your account banned; server-compatible mode exists for that reason.
- There is no raw key stream: a mod receives only the key bindings it declared.

## Profiler and mod manager

The F3 screen lists one line per external mod, right after the CPU/GPU lines:
`mod <id>: <calls/s> calls, <mean µs> µs avg, <max µs> max, <overruns> over,
<dropped> dropped`, plus `off`, `held` or `failed` when the mod is not running.
Rates come from snapshots one second apart; with no external mod there is
no line. The mods menu shows each mod's description, version, authors,
permissions (`packets`, `movement`, `files`, `http`, `camera`), the first 12 hex digits of its
hash and the reason of a failure. A library that is not trusted gets a card
("Not trusted: <hash>") whose switch trusts it and loads it at the next start.

## The C ABI

Mods in other languages use `include/umicraft.h` (committed once generated), produced from
`umicraft-modapi-sys` with `tools/gen-header.sh` (cbindgen, an external tool).
The contract, versioning and exports are in
[ADR-0087](../decisions/0087-modding-abi-contract.md).
