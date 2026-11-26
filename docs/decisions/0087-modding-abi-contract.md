# ADR-0087 — The C ABI contract of the modding API

Status: accepted (2026-11-19)

## Context

Spec §8.3 asks for a stable, versioned C ABI so that mods survive client
updates within an API major and can be written in Rust, C, C++ or Zig.
Spec §13 item 4 left open the helper: `abi_stable`, `stabby` or a pure C ABI.

## Decision

### A pure C ABI

The boundary uses only `#[repr(C)]` structs, `#[repr(transparent)]`
newtypes, primitive integers, raw pointers with lengths and
`extern "C" fn` pointers. `abi_stable` and `stabby` are not used: both only
serve Rust mods, and both add a large dependency to every mod. The types live
in `umicraft-modapi-sys` and are the single source of truth.

### The header

`include/umicraft.h` is generated from `umicraft-modapi-sys` with cbindgen
and committed. cbindgen is MPL-2.0, which is not on the `deny.toml` allow
list, so it is an **external tool**, never a dependency: `tools/gen-header.sh`
runs it and CI checks that the committed header is up to date.

### Versioning

- `UmiVersion { major: u16, minor: u16 }`. The host refuses a mod whose major
  differs. Within a major, a mod built for minor `m` loads on a host with
  minor `≥ m`.
- **Major 0 is unstable**: while F3 is in progress the API is `0.x`, any
  change is allowed, and the host requires an exact minor match. F3 ends by
  freezing `1.0`.
- From `1.0` on, structs grow only by appending fields. Every growable struct
  starts with a `size` field; a mod checks `size` before using a field added
  in a later minor. Ids and enum values are never reused.

### Exports of a mod

```c
UmiVersion umi_mod_api_version(void);
UmiStatus  umi_mod_init(const UmiHost *host, UmiModHandle me);
void       umi_mod_shutdown(void);
```

`umi_mod_init` registers everything the mod needs through `host`. A mod has
no other entry point; every callback is registered with a `void *user`
pointer that the host passes back untouched.

### Core types

| Type | Meaning |
|---|---|
| `UmiStr { ptr, len }` | borrowed UTF-8, valid for the call only; `ptr` may be null when `len` is 0 |
| `UmiStatus` (`i32`) | `OK`, `ERROR`, `PANICKED`, `UNSUPPORTED`, `INVALID_ARGUMENT`, `DENIED`, `CONFLICT` |
| `UmiModHandle` (`UmiModContext *`) | opaque pointer to the host's per-mod state, passed back to every host function; a pointer and not an index so the host needs no process-global registry (amended 2026-11-19, before any mod was built) |
| `UmiEventId` (`u32`) | stable event id; never reused |
| `UmiEventHeader { size, id, flags }` | first field of every event payload; `flags` holds `CANCELLABLE`, `CANCELLED` |
| `UmiMode` | `OBSERVE`, `INTERCEPT` |
| `UmiLevel` | `ERROR`, `WARN`, `INFO`, `DEBUG`, `TRACE` |
| `UmiHandlerFn` | `UmiStatus (*)(UmiEventHeader *event, void *user)` |
| `UmiHost` | `size`, `version`, then the host functions, append-only |

### `unsafe`

The workspace denies `unsafe_code`. Each of `umicraft-modapi` and
`umicraft-modhost` has one `ffi` module with `#![allow(unsafe_code)]`; every
`unsafe` block in it carries a `// SAFETY:` comment. No other module of these
crates may use `unsafe`.

## Consequences

- C, C++ and Zig mods use the header directly; Rust mods use
  `umicraft-modapi`.
- A field added to `UmiHost` during `0.x` breaks every mod built for the
  previous minor: acceptable before the `1.0` freeze, and the reason the
  freeze is the last step of F3.

## Amendments

- **2026-11-19, F3.9 — frozen as 1.0.** `UMI_API_VERSION` is `1.0`. Within major 1:
  structs grow only by appending fields covered by their `size`, `UmiHost` only by
  appending function pointers, event ids and status values are only added, and every
  addition bumps the minor; nothing is removed, renamed or reordered. The layout tests
  of `umicraft-modapi-sys/tests/abi.rs` pin every offset of 1.0 and must not be edited
  except to add assertions. A break needs major 2 (and a new ADR). The generated
  header `include/umicraft.h` is the C view of the same contract.
- **2026-11-23 — 1.1 (ADR-0094).** First minor of major 1, made only by appending: new
  fields after the last 1.0 field of `UmiFrame`, `UmiEntity` and `UmiPlayerPhysics`
  (their `size` grew, every 1.0 offset is unchanged), five host functions after
  `post_set_param`, event 13 (`PRE_RENDER`), anchor value 9 (`UMI_HUD_ANCHOR_SCREEN`,
  refused by 1.0) and the `render_camera` service. Only the size assertions of the
  1.0 layout tests changed; every offset assertion stands.
- **2026-11-23 — 1.2 (ADR-0095).** Appended again: six host functions after
  `hud_segment` (categories, modules, settings and buttons on a module, attachments,
  `module_enabled`), events 14 (`MODULE_TOGGLED`) and 15 (`BUTTON_PRESSED`) and the
  new `UmiModuleDef`. A button has its own function rather than a new setting kind, so
  `UmiSettingDef` keeps its 1.0 size. Only the `UmiHost` size assertion changed.
- **2026-11-24 — 1.3 (ADR-0096).** One host function appended after
  `module_enabled`: `camera_capture`. Only the `UmiHost` size assertion changed.
- **2026-11-25 — 1.4 (ADR-0097).** Ten host functions appended after
  `camera_capture`, fields appended to `UmiEntity` (168 to 280 bytes), the new
  `UmiEffect`, `UmiWorldCx` and `UmiWorldDrawFn`, four entity flags and attach kind 3.
  `UmiEntity` is the first struct a mod hands the host as an **array** that grew
  (`world_entities_snapshot`): the host now records the `api_version` each library
  declares and writes rows of the size that version knows, so a 1.1 mod's buffer is
  never overrun. Any future growth of a struct passed as an array must do the same.
- **2026-11-26 — 1.5 (ADR-0098).** Three host functions appended after
  `world_highlight` (`input_press`, `input_clear_press`, `input_add_look`), event 16
  (`INPUT`) with the new `UmiInput`, and the `input` permission. Only the `UmiHost`
  size assertion changed.
