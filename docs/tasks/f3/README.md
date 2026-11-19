# F3 — Modding API v1

Status: **in progress** (started 2026-11-19, before the end of F2 and of the
P13 sign-off, by decision of the project owner).

Design: ADR-0086 (architecture), ADR-0087 (C ABI contract). Roadmap: F3 in
`ROADMAP.md` §6. Spec: §8.

| # | Phase | Acceptance | State |
|---|---|---|---|
| F3.0 | ADR-0086 and ADR-0087; spec §8.4 and §13 amended; this plan | ADRs accepted | done |
| F3.1 | `umicraft-modapi-sys` (core ABI types, layout tests, header script); `umicraft-modhost` manifests and resolver (pure code) | unit and property tests of the resolver | done (header generation waits for cbindgen to be installed) |
| F3.2 | Loader (`libloading`), `umi_mod_init`/`shutdown`, `log`, panic → disable, `--safe-mode`, crash sentinel, SHA-256 trust list; `umicraft-modapi` with `Mod` and `umicraft_mod!` | `hello` and `panicking` example cdylibs load in an integration test | todo |
| F3.3 | Per-thread `EventBus` in `umicraft-core`, wired in the client: lifecycle, chat, input, screens; observe channel and mod worker | criterion bench of the dispatch in `docs/perf-log.md`; no cost with no mod | todo |
| F3.4 | HUD widgets, schema settings, open key bindings, client commands; external mods in the mods menu | `hud` example | todo |
| F3.5 | Raw packet hooks on the network thread, validated injection, permission gating, server-compat mode, "alters packets/movement" badge | `packet-logger` example | todo |
| F3.6 | Services (`player_physics` wrap/replace), conflict detection at load, read-only world and entity queries | `physics-tweak` example; trajectories unchanged with no mod (ADR-0076) | todo |
| F3.7 | Post-process chain with naga-validated WGSL | `shader-pass` example | todo |
| F3.8 | Per-mod profiler in the Umicraft debug overlay, budget auto-disable, mod manager (permissions, hash, enable/disable), `docs/modding/` guide and template | former M15 acceptance criteria | todo |
| F3.9 | Built-in mods on the same bus (`ModContext` → `FrameEvent`); API frozen as `1.0` | no regression of the 16 built-in mods | todo |

## F3.1 interface (frozen before the work is split, spec §14 rule 11)

### `umicraft-modapi-sys` (owner: one contributor)

Files: `crates/umicraft-modapi-sys/**`, `tools/gen-header.sh`,
`include/umicraft.h`. Types of ADR-0087: `UmiStr`, `UmiVersion`,
`UmiStatus`, `UmiModHandle`, `UmiEventId`, `UmiEventHeader` with the flag
constants, `UmiMode`, `UmiLevel`, `UmiHandlerFn`, `UmiHost` (`size`,
`version`, `log`, `subscribe`), the two tick events `UmiPreTick` and
`UmiPostTick` (`header`, `tick: u64`) with their ids, the export symbol names
and their function-pointer types, and `UMI_API_VERSION` = `0.1`.

### `umicraft-modhost` manifests and resolver (owner: one contributor)

Files: `crates/umicraft-modhost/src/{lib,manifest,resolve,error}.rs` and its
tests. No dependency on `umicraft-modapi-sys` yet.

`mod.toml` (spec §8.2, unknown keys rejected):

```toml
[mod]
id = "example_hud"         # ^[a-z][a-z0-9_]{0,63}$
name = "Example HUD"
version = "0.1.0"          # semver
authors = ["Someone"]
api_version = "0.1"        # major.minor the mod was built for
license = "MIT"            # optional
description = "…"          # optional
library = "example_hud"    # optional, defaults to id; bin/<os>-<arch>/<prefix><library><suffix>

[dependencies]
other_mod = ">=0.2, <1.0"  # semver requirement

[load]
priority = 100             # lower loads earlier; default 0
replaces = []              # service ids, ^[a-z][a-z0-9_.]{0,63}$
wraps = []

[permissions]              # shown to the user; gates packet and service hooks
network_packets = false
movement = false
filesystem = false
network_http = false
```

Resolver: `resolve(candidates, host_api) -> Resolution`. A candidate is a
manifest plus `enabled`. The resolution holds the load order (dependencies
first, then ascending priority, then id) and one rejection per mod that will
not load, with its reason: disabled, duplicate id (every copy rejected),
incompatible API (ADR-0087 rules), missing dependency, dependency version not
matched, dependency rejected (cascades), dependency cycle, service conflict
(two mods replace the same service: the later in load order is rejected).
Wrap chains: `Resolution::wrappers(service)` in load order.

## F3.2 interface (frozen before the work is split)

`UmiModHandle` is an opaque pointer to the host's per-mod `UmiModContext`
(ADR-0087, amended): host functions reach their state through it, with no
process-global registry.

### `umicraft-modapi` (mod side; files `crates/umicraft-modapi/**`)

- `trait Mod: Send + Sized + 'static { fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError>; fn shutdown(&mut self) {} }`.
- `InitCx`: `logger() -> Logger`, `observe::<E: Event>(priority, FnMut(&E) + Send + 'static)`,
  `intercept::<E: Event>(priority, FnMut(&mut E, &mut Control) + Send + 'static)`;
  both return `Result<(), ModError>` mapped from the host's `UmiStatus`.
- `Logger: Copy + Send + Sync` with `error/warn/info/debug/trace(&str)`; valid
  for the life of the process because the host table and the context are.
- `trait Event` (sealed): `const ID`, the `#[repr(C)]` raw payload, a safe view.
  F3.2 ships `PreTick` and `PostTick` (`tick() -> u64`). `Control::cancel()`
  is a no-op error-free call on events that are not cancellable.
- `umicraft_mod!(MyMod)` emits `umi_mod_api_version`, `umi_mod_init`,
  `umi_mod_shutdown`. Every entry point and every trampoline runs inside
  `catch_unwind` and returns `UmiStatus::PANICKED` on panic. The handler
  closures are boxed, their pointer is the `user` pointer, and they are
  freed by `umi_mod_shutdown`.
- `unsafe` only in `src/ffi.rs` (ADR-0087).

### `umicraft-modhost` loader (host side; files `crates/umicraft-modhost/src/{host,load,trust,sentinel,discover}*`)

- `discover(mods_dir) -> Vec<Discovered>`: one entry per sub-directory with a
  `mod.toml`: its directory and `Result<Manifest, ManifestError>`.
- `TrustList` (`trusted.toml`: mod id → lowercase hex SHA-256), `load`/`save`,
  `sha256_file(path)`; a library loads when its hash is listed, or when a
  `confirm(&Manifest, &str /*hex*/) -> bool` callback accepts it (then it
  is added to the list).
- `CrashSentinel`: `armed(path) -> bool` (left by a previous crash), `arm(path)
  -> SentinelGuard` that removes the file on `disarm()` (clean exit only, not
  on drop during a panic).
- `ModHost::load(resolution, mods_dir, trust, confirm) -> (ModHost, Vec<LoadFailure>)`:
  in resolution order, opens `bin/<os>-<arch>/<file>`, checks
  `umi_mod_api_version` with `UmiVersion::accepts`, calls `umi_mod_init`
  with the `UmiHost` table and the mod's context. A failure (missing file,
  refused hash, missing symbol, API mismatch, init error or panic) is a
  `LoadFailure` and never aborts the others.
- Host functions: `log` forwards to `tracing` with the mod id as a field
  (invalid UTF-8 replaced, length capped); `subscribe` records
  `(event, mode, priority, handler, user)` in the context, refuses unknown
  events/modes, null handlers and calls after `init` returned.
- `ModHost::subscriptions(event) -> impl Iterator` (load order, then priority)
  for the F3.3 dispatcher; `ModHost::disable(id)` calls `umi_mod_shutdown`
  once and drops the mod's subscriptions; `ModHost::shutdown_all()` at exit.
  Libraries stay mapped until `ModHost` is dropped.
- `unsafe` only in `src/ffi.rs` (ADR-0087).
