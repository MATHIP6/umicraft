# F3 — Modding API v1

Status: **in progress** (started 2026-11-19, before the end of F2 and of the
P13 sign-off, by decision of the project owner).

Design: ADR-0086 (architecture), ADR-0087 (C ABI contract). Roadmap: F3 in
`ROADMAP.md` §6. Spec: §8.

| # | Phase | Acceptance | State |
|---|---|---|---|
| F3.0 | ADR-0086 and ADR-0087; spec §8.4 and §13 amended; this plan | ADRs accepted | done |
| F3.1 | `umicraft-modapi-sys` (core ABI types, layout tests, header script); `umicraft-modhost` manifests and resolver (pure code) | unit and property tests of the resolver | in progress |
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
