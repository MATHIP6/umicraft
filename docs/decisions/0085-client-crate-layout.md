# ADR-0085 — Layout of the client crate

Status: accepted (2026-11-18)

## Context

After ADR-0072 to ADR-0075 split the largest files, the root of
`umicraft-client/src` still held 112 modules side by side (`hud_view`,
`hud_cache`, `hud_demo_mods`, `creative_input`, `creative_screen`, ...).
The prefixes were doing the job of folders, and 80 of the modules were
`pub` although only the binary, three integration tests and the benches use
the library from outside.

## Decision

The crate root keeps the threads (`network`, `game`, `window`,
`world_view`, `headless`), the session's settings and services (`options`,
`servers`, `accounts`, `profiles`, `audio_bridge`, `discord`, `language`,
`pack_offer`), the command line (`args`, `launch_session`, `setup_cli`),
`input`, `screenshot` and `error`. Everything else is in a domain folder:

| Folder | What it holds |
|---|---|
| `demos` | the server-less runs, the benchmark scene, the `e1` scenario |
| `screens` | the `Screen` state (formerly `screen`), the menus, the options screens, `inventory/`, `creative/`, the book and sign screens |
| `hud` | the player HUD, the social overlays, the chat, the dials, the cache |
| `debug` | the F3 screen, the debug chords, the world facts, the frame metrics |
| `mods` | the client side of the built-in mods, the mods menu, the HUD editor |
| `scene` | entities, block entities, items, maps, particles, weather, fog, overlays, cracks |
| `meshing` | the mesh worker pool, the queue, the mesh settings |
| `resources` | the terrain array, models, fonts, icons, sky and panorama pictures |

`ui_input`, `mouse_look` and `clipboard` join `input`; `options_effects`
joins `options`; `screenshot_file` joins `screenshot`. Inside a folder the
prefix is dropped (`hud_view` → `hud::player`, `creative_screen` →
`screens::creative::draw`, `mesher` → `meshing::pool`).

A module is `pub` only on a path the binary, the integration tests or the
benches use; the rest is `pub(crate)`. Narrowing the visibility showed code
that only the public API kept alive:

- removed: `E1Run::finish`, `TextureAnimator::len`, the unread
  `MeshResult::build_micros` (the pool counters already aggregate the build
  times) and the unused re-exports of `options` and `servers`;
- marked `#[cfg(test)]`: ten helpers only the unit tests call.

The doctest of `servers::parse_address` cannot run on a crate-private item;
it is kept as an `ignore` example and a unit test now covers it.

## Consequences

- Test names are the same before and after; the paths of the modules
  change. ADRs and task notes keep the paths of their time; the living docs
  (`docs/sources.md`, `docs/profiling.md`, `docs/parity.md`, the
  vanilla-agent scenarios) point to the new files.
- A new module goes into the folder of its domain, without a prefix that
  repeats the folder.
