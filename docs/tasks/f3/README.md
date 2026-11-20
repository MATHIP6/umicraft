# F3 — Modding API v1

Status: **in progress** (started 2026-11-19, before the end of F2 and of the
P13 sign-off, by decision of the project owner).

Design: ADR-0086 (architecture), ADR-0087 (C ABI contract). Roadmap: F3 in
`ROADMAP.md` §6. Spec: §8.

| # | Phase | Acceptance | State |
|---|---|---|---|
| F3.0 | ADR-0086 and ADR-0087; spec §8.4 and §13 amended; this plan | ADRs accepted | done |
| F3.1 | `umicraft-modapi-sys` (core ABI types, layout tests, header script); `umicraft-modhost` manifests and resolver (pure code) | unit and property tests of the resolver | done (header generation waits for cbindgen to be installed) |
| F3.2 | Loader (`libloading`), `umi_mod_init`/`shutdown`, `log`, panic → disable, `--safe-mode`, crash sentinel, SHA-256 trust list; `umicraft-modapi` with `Mod` and `umicraft_mod!` | `hello` and `panicking` example cdylibs load in an integration test | done |
| F3.3 | Per-thread `EventBus` in `umicraft-core`, wired in the client: lifecycle, chat, input, screens; observe channel and mod worker | criterion bench of the dispatch in `docs/perf-log.md`; no cost with no mod | done |
| F3.4 | HUD widgets, schema settings, open key bindings, client commands; external mods in the mods menu | `hud` example | done |
| F3.5 | Raw packet hooks on the network thread, validated injection, permission gating, server-compat mode, "alters packets/movement" badge | `packet_logger` example | done |
| F3.6 | Services (`player_physics` wrap/replace), conflict detection at load, read-only world and entity queries | `physics_tweak` example; trajectories unchanged with no mod (ADR-0076) | done |
| F3.7 | Post-process chain with naga-validated WGSL | `shader_pass` example | done |
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
  once and drops the mod's subscriptions (changed in F3.3: disable is a flag
  only, `umi_mod_shutdown` runs in `shutdown_all`); `ModHost::shutdown_all()` at exit.
  Libraries stay mapped until `ModHost` is dropped.
- `unsafe` only in `src/ffi.rs` (ADR-0087; split into `src/ffi/` in F3.3).

### Rule for F3.3 (found in the F3.2 review)

A mod's trampoline reaches its closure through the `user` pointer without a
lock, and `umi_mod_shutdown` frees the closures. The host must therefore
never call `umi_mod_shutdown` while a dispatch to that mod may be in flight:
disabling flips the mod's flag first, and the shutdown runs at a quiescent
point (between ticks on the game thread, between frames on the render
thread, after the mod's worker has drained), or at client exit.

### Known limits after F3.2 (owned by F3.8)

- The crash sentinel is one file per data directory: a second client started
  while the first runs sees it and starts in safe mode, and the first one's
  clean exit removes it while the second still runs. F3.8 makes it a
  per-instance lock.
- An untrusted library is refused with its SHA-256 and the line to add to
  `trusted.toml` in the log; the confirmation dialog comes with the mod
  manager. Every discovered mod is enabled until then.
- Mods load in the windowed client only, not in the headless one.

## F3.3 interface (frozen before the work is split)

API **0.2** (exact match while `0.x`): events `CHAT_RECEIVED` (3), `CHAT_SEND`
(4), `CONNECTED` (5), `DISCONNECTED` (6), `SCREEN_OPENED` (7),
`SCREEN_CLOSED` (8) with their payloads in `umicraft-modapi-sys`, the flag
`UMI_EVENT_MUTABLE` (4) and `UmiHost::event_set_text`. `KeyInput` moves to
F3.4 with the open key bindings (it needs a stable key code table).

| Event | Thread | Cancellable | Mutable |
|---|---|---|---|
| `PRE_TICK`, `POST_TICK` | game | no | no |
| `CHAT_RECEIVED` | game | yes (line hidden) | JSON text |
| `CHAT_SEND` | game | yes (nothing sent) | line |
| `CONNECTED`, `DISCONNECTED` | render | no | no |
| `SCREEN_OPENED`, `SCREEN_CLOSED` | render | no | no |

### `umicraft-core` (`src/events.rs`)

- `Event: 'static` (no `Send`: a bus belongs to one thread; the bus itself
  stays `Send` because handlers are, so it can be built on the main thread
  and moved). Handlers are kept sorted at registration (stable within a
  priority), never per dispatch.
- `EventBus::has_subscribers::<E>() -> bool`, so an emitter skips building a
  payload (and its allocations) when nobody listens: no cost without mods.

### `umicraft-modhost` dispatch (`src/events/**`, `src/dispatch.rs`, `src/worker.rs`, `benches/`)

- **Host events**: one Rust type per ABI event, `#[repr(C)]` with the ABI
  payload as first field, owning its strings (the payload's `UmiStr` points
  into them): `PreTick`, `PostTick`, `ChatReceived`, `ChatSend`,
  `Connected`, `Disconnected`, `ScreenOpened`, `ScreenClosed`. Constructors
  take the client's values; accessors read the (possibly replaced) text and
  the cancel flag. Built-in Rust modules can subscribe to the same types on
  the core bus. They are `Send` (the pointers target owned heap data; one
  justified `unsafe impl` in `ffi.rs`) so an observer can get a copy.
- `EventThread { Game, Render }` and the static table event → thread.
- `ModHost::install(&self, bus: &mut EventBus, thread: EventThread)`
  registers one adapter per subscription of that thread. An intercept
  adapter: skips a disabled mod; sets `MUTABLE` (and `CANCELLABLE` per the
  table) for the call; times the call; maps `CANCELLED` to
  `EventCx::cancel`; on `PANICKED` or another non-`OK` status disables the
  mod (flag only) and logs once. An observe adapter sends a copy to the
  mod's worker without blocking (bounded queue of 256, dropped when full and
  counted).
- **Worker**: one thread per mod with observers, `umicraft-mod-<id>`,
  started by `install`'s first observe subscription and stopped at
  `shutdown_all`.
- **Budget**: an intercept call over 1 ms counts as an overrun and is logged
  (first one, then every 100th); 50 consecutive overruns disable the mod.
- **Stats** per mod (atomics, for the F3.8 profiler): calls, total and max
  nanoseconds, overruns, dropped observations.
- **Disable is a flag**: `ModHost::disable` no longer calls
  `umi_mod_shutdown`; only `shutdown_all` does, once, after the client has
  stopped every thread that dispatches (shutdown-quiescence rule above).
- `event_set_text`: checks `MUTABLE` and the id, validates UTF-8, stores the
  replacement in the host event and repoints the payload's `UmiStr`.
- Criterion bench `dispatch`: `PreTick` with no subscriber, with one Rust
  handler, with one foreign intercept handler; `ChatSend` with no
  subscriber. Numbers go to `docs/perf-log.md`.

### `umicraft-modapi` (mod side)

Views `ChatReceived` (`json()`, `position()`, `set_json()`), `ChatSend`
(`text()`, `set_text()`), `Connected` (`address()`), `Disconnected`
(`reason()`), `ScreenOpened`/`ScreenClosed` (`name()`). Text getters return
`&str` borrowed from the event (invalid UTF-8 from a non-conforming host
reads as empty). Setters return `Result<(), ModError>` and call
`event_set_text`.

### Client wiring (after the two above)

Game thread: `PreTick`/`PostTick` around `tick()`, `ChatReceived` before
`push_chat`, `ChatSend` before the chat command is queued. Render thread:
`Connected`/`Disconnected` in `session_control`, `ScreenOpened`/`Closed` in
`set_screen` with stable names. The game thread receives its bus from
`Session::start`; `exiting` drops the session before `shutdown_all`.

## F3.4 interface (frozen before the work is split)

API **0.3**: `UmiSettingDef`/`UmiSettingValue`/`UmiSettingKind`,
`UmiHudPlacement`, opaque `UmiHudCx`, `UmiHudDrawFn`, `UmiCommandFn`,
event `KEY_PRESSED` (9, render thread, payload `UmiKeyPressed { header, key }`)
and the host functions `register_setting`, `setting_get`,
`register_hud_widget`, `hud_line`/`hud_label`/`hud_cell`/`hud_tick`/`hud_reserve`,
`register_key`, `register_command`, `chat_print` (ADR-0086 amendment: no raw
key events). Registrations are accepted during `umi_mod_init` only. Texts
are literal UTF-8, copied by the host. Keys of settings, widgets and key
bindings follow `^[a-z][a-z0-9_]{0,63}$`; command names `^[a-z][a-z0-9_-]{0,31}$`.

### `umicraft-modhost` (host side)

- Depends on `umicraft-builtin-mods` for the shared HUD and settings data
  model (`HudContent`, `HudPlacement`, `Anchor`, `SettingValue`) instead of
  duplicating it; validation rules shared with the built-in settings
  (slider clamp and step, colour format) are moved into public pure helpers
  of `umicraft-builtin-mods` and reused.
- Per-mod declarations in `ModContext`: owned setting specs with their value
  held in atomics (`setting_get` from any thread without a lock), widget
  specs (draw fn, user, default placement, current placement, a
  `Mutex<HudContent>`), key specs (id, label, default binding name).
- Host-wide shared state (`Arc`, created by `ModHost::load` and given to every
  context): the command table (a name taken twice → `CONFLICT` for the
  second mod) and the chat outbox (bounded queue of 64 local lines, dropped
  when full) that `chat_print` feeds.
- **Enable switch.** A mod is enabled when the player wants it and it has not
  failed. `ModHost::set_enabled(id, bool)` changes the player's wish and can
  re-enable a mod the player switched off (its handlers were never freed);
  a failed mod stays off until restart.
- **Config.** `ExternalModConfig { enabled, settings, placements, keys }`
  (serde, unknown keys kept) with `load(path)`/`save(path)` helpers;
  `ModHost::config(id)` and `ModHost::apply_config(id, &cfg)` (values
  sanitised against the specs; unknown keys kept for a later version of the
  mod). `ModHost::set_setting(id, key, value)`, `set_placement(id, widget,
  placement)`, `set_key(id, key, binding_name)`.
- **HUD.** `ModHost::draw_widgets(&self) -> Vec<DrawnWidget>` (or a visitor
  without allocation): for each widget of each enabled mod, clear its
  content, call `draw` on the render thread with a `UmiHudCx` that points to
  it, timed against a 0.5 ms budget with the F3.3 counters; a non-`OK` status
  or a budget exhaustion fails the mod. A `DrawnWidget` exposes mod id,
  widget key, label, placement and `&HudContent`. The `hud_*` host functions
  reject a null cx, non-finite numbers and invalid UTF-8.
- **Keys.** `ModHost::key_bindings()` lists `(mod id, key id, label,
  default binding name, current binding name)`. Host event `KeyPressed`
  carries a target mod id outside the ABI payload, and the dispatch adapter
  calls only the target mod's handlers (a `HostEvent::target` method,
  `None` for every other event).
- **Commands.** `ModHost::commands() -> CommandTable` (`Send + Clone`, moved
  to the game thread with the game bus): `run(line) -> CommandOutcome
  { NotACommand, Ran, Rejected, Failed }` (`/name args`; the handler is
  budgeted like an intercept handler; `ERROR` means the arguments were
  refused, so the usage is printed and the mod stays enabled; a panic or
  another status fails the mod), `complete(prefix) ->
  Vec<String>` for tab completion, `usage(name)`. `ModHost::chat_outbox()`
  returns the receiver end of the local lines.

### `umicraft-modapi` (mod side)

`InitCx::{toggle, slider, choice, color}` return a `Setting<T>` handle
(`Copy + Send + Sync`, `get()`); `InitCx::hud_widget(key, label, Placement,
FnMut(&mut Hud) + Send)` with `Hud::{line, label, cell, tick, reserve}`;
`InitCx::key(key, label, default)` and the `KeyPressed` view (`key()`);
`InitCx::command(name, usage, FnMut(&str) -> Result<(), ModError> + Send)`;
`InitCx::chat() -> Chat` (`Copy + Send + Sync`, `print(&str)`). The host
functions a handle needs after `init` are kept in the mod-side static
introduced in F3.3, generalised to the whole host table.

### Client (after the two above)

External mods appear in the mods menu as cards of an extra category (name and
description from `mod.toml`, settings from their specs, enable switch);
card ids are prefixed `ext:` so outcomes route to `ModHost`. Their widgets
are drawn with the built-in widgets and are movable in the HUD editor. Their
key bindings are listed on the controls page under the mod's name and
dispatched as `KeyPressed` on a press with no screen open. `/name` lines are
run by the `CommandTable` on the game thread before `ChatSend`; tab
completion merges local command names. Each mod's choices persist in
`<options dir>/mods/<id>.toml`. A mod id equal to a built-in mod id is
refused at load.

## F3.5 interface (frozen before the work is split)

API **0.4**. Packets cross the ABI as raw `id` + body bytes (protocol 47 play
state, no length prefix, no compression).

| Event | Thread | Cancellable | Mutable |
|---|---|---|---|
| `PACKET_RECEIVED` (10) | network | yes (the session never sees it) | no |
| `PACKET_SEND` (11) | network | yes (nothing written) | no |

Neither event is mutable in v1: a clientbound body cannot be validated before
the session handles it (most ids are decoded only there, and a bad body ends
the session), and a serverbound packet is changed by cancelling it and
injecting the replacement, which `packet_send` validates. `Set Compression`
(0x46) and `Disconnect` (0x40) are never dropped: dropping them would break
the framing or hide the kick reason.

### `umicraft-modapi-sys`

- `UmiBytes { ptr: *const u8, len: usize }` (borrowed bytes, any content).
- `UmiPacket { header, id: u32, data: UmiBytes }`, payload of both events.
- `UmiStatus::BUSY` (7): a bounded queue is full, try again later.
- `UmiHost::packet_send(me, id: u32, data: UmiBytes) -> UmiStatus`, appended.
- `UMI_API_VERSION` = `0.4`.

### `umicraft-net` (files `crates/umicraft-net/**`, callers in `umicraft-testkit`)

- `trait PacketFilter` in `play`: `receive(&mut self, id: u8, body: &[u8]) ->
  bool` (false drops a clientbound packet before `PlaySession::handle`, except
  0x46 and 0x40, which `drive` always keeps), `send(&mut self, packet:
  &PlayServerbound) -> bool` (false drops a serverbound packet: commands,
  replies and injections alike), `inject(&mut self, out: &mut
  Vec<PlayServerbound>)` (appends packets to write; `drive` calls it after
  every loop turn, runs them through `send` and writes them). All three have
  pass-through defaults; `NoFilter` is the unit implementation.
- `drive(connection, session, commands, counters, filter: &mut impl
  PacketFilter, on_event)`. `packets_received` still counts dropped packets;
  `packets_sent` counts written ones only.
- Injection latency: one loop turn, which the game thread's movement packets
  bound to a tick.

### `umicraft-modhost`

- `EventThread::Network`; host events `PacketReceived` and `PacketSend`
  (`new(id: u8, body: &[u8])`, `id() -> u8`, `data() -> &[u8]`, `Clone`
  repoints the payload, `Send` in `ffi/event.rs`).
- `ModContext` keeps the manifest's `Permissions` (`with_shared(id,
  permissions, shared)`; `ModContext::new` keeps none). `subscribe` to a
  packet event without `network_packets` → `DENIED`.
- **Server-compat mode** (`HostShared` atomic, `ModHost::set_server_compat(bool)`,
  `server_compat()`): while on, a mod whose manifest declares
  `network_packets` or `movement` is not enabled. `ModContext::alters_game()`.
  `is_enabled = wanted && !failed && !(compat && alters_game)`.
- `install_mod` skips failed mods only (a mod the player switched off before
  the bus was built must still work when switched back on; regression).
- **Injection**: `packet_send` (any thread, after init): `DENIED` without the
  permission or when the mod is not enabled; `INVALID_ARGUMENT` for an id
  above 0xff, a body that `PlayServerbound::decode` refuses or does not
  consume entirely; `BUSY` when the shared queue (64 packets) is full. Accepted
  packets go to `ModHost::packet_outbox() -> PacketOutbox` (`Send + Clone`,
  `drain(&mut Vec<PlayServerbound>)`, `clear()`). Stats count injected packets.
  New dependency: `umicraft-protocol` (no auth, no I/O).
- `ModView` (or the mods listing) exposes the permissions for the badge.

### `umicraft-modapi`

Views `PacketReceived` and `PacketSend` (`id() -> u8`, `data() -> &[u8]`);
`InitCx::packets() -> Packets` (`Copy + Send + Sync`, `send(id: u8, body:
&[u8]) -> Result<(), ModError>`, `BUSY` mapped to a distinct `ModError`).

### Example `mods/examples/packet_logger`

Declares `network_packets = true`; observes both events and logs one line per
packet id per second with its count and bytes; a HUD widget shows packets/s
in and out; `/swing` injects an Animation (0x0A, empty body).

### Client

The network thread gets a `NetMods { bus, outbox }` and a `ModPacketFilter`
(zero cost without subscribers: `has_subscribers` before copying a body,
`outbox` drained only when non-empty); the outbox is cleared when a session
starts, so a stale injection never reaches another server. Server-compat
mode is an option (`multiplayer.mod_server_compat`) shown as a switch in the External
category of the mods menu; cards of mods that declare `network_packets` or
`movement` carry an "alters packets/movement" badge.

## F3.6 interface (frozen before the work is split)

API **0.5** (types already in `umicraft-modapi-sys/src/services.rs`):
`UmiServiceMode { WRAP, REPLACE }`, `UmiPlayerPhysics` (flags `ON_GROUND` and
`SPRINTING` writable, `SNEAKING`/`FLYING`/`IN_WATER` read-only; position,
velocity, fall distance writable; yaw/pitch read-only), `UmiPhysicsInput`,
opaque `UmiPhysicsCx`, `UmiPhysicsFn`, `UmiEntity`, `UmiEntityVisitFn`,
`UmiStatus::UNAVAILABLE` (8) and the host functions `register_physics`,
`physics_next`, `world_block`, `world_entities`.

### `umicraft-game` (files `crates/umicraft-game/**`)

- `pub trait PhysicsOverride: Send { fn step(&mut self, player: &mut Player,
  intent: &InputIntent, vanilla: &mut dyn FnMut(&mut Player)); }` in
  `physics.rs` (or a new `physics/override.rs`). `vanilla` runs
  `physics::step` with the tick's world.
- `GameState::set_physics(&mut self, physics: Option<Box<dyn PhysicsOverride>>)`;
  `tick` and the replay of `apply_correction` call the override when set,
  `physics::step` directly otherwise (no indirection, no allocation: the
  trajectory with no override is bit for bit the ADR-0076 one; the existing
  `physics` bench must not move).
- `EntityKind::name(self) -> &'static str`: stable snake_case names, one per
  variant, never reused.

### `umicraft-modhost`

- **Physics layers**: `register_physics` during init only; `DENIED` without
  the `movement` permission or when the mode's list in `mod.toml` (`wraps` for
  `WRAP`, `replaces` for `REPLACE`) lacks `player_physics`; `CONFLICT` for a
  second layer of the same mod; `INVALID_ARGUMENT` for a null fn or an unknown
  mode. At load, a manifest naming a service other than `player_physics` in
  `wraps`/`replaces` fails that mod (`LoadError::UnknownService`) before init.
- `ModHost::physics() -> Option<PhysicsChain>` (`None` when no mod registered
  a layer). `PhysicsChain: Send + Clone`; `is_active(&self) -> bool` (some
  layer enabled now); `step(&self, state: &mut UmiPlayerPhysics, input:
  &UmiPhysicsInput, vanilla: &mut dyn FnMut(&mut UmiPlayerPhysics))`.
  Wrappers nest in load order (first loaded = outermost) around the base: the
  enabled replacing layer, else `vanilla`. A disabled layer is skipped (its
  `next` is called for it). `physics_next` runs the rest of the chain on the
  same state, once (`DENIED` the second time; `UNSUPPORTED` in a replacing
  layer). A layer is budgeted like an intercept handler (`BUDGET`) on its own
  time, the time inside `physics_next` excluded. A layer that fails (panic,
  non-`OK`, budget) fails its mod and its own changes are discarded: if it had
  called `physics_next`, the state is restored to what the inner chain produced
  (snapshot right after `physics_next` returned), so the player still moves by
  vanilla and the inner layers that tick; if it had not, the state is restored to
  what the layer received and the rest of the chain runs on it. Replacing layers
  sit inside all wrappers, whatever the load order. Stats count calls.
- **World queries**: `pub trait WorldQuery { fn block(&self, x: i32, y: i32,
  z: i32) -> Option<u16>; fn visit_entities(&self, visit: &mut dyn
  FnMut(&EntityView<'_>) -> bool); }` (`block`: wire state `id << 4 | meta`,
  `None` when the chunk is not loaded; `visit` returns false to stop) and
  `EntityView<'a> { id: i32, kind: &'a str, position: [f64; 3], yaw: f32,
  pitch: f32, name: &'a str }`. `pub fn with_world<R>(world: &dyn WorldQuery,
  f: impl FnOnce() -> R) -> R` lends the world to the current thread for `f`
  (restored on unwind, nestable). `world_block`/`world_entities` answer only
  inside such a scope (`UNSUPPORTED` elsewhere, e.g. on a worker or the
  render thread), `UNAVAILABLE` for an unloaded chunk, `INVALID_ARGUMENT` for
  null pointers. No permission (read-only, ADR-0086).

### `umicraft-modapi`

`InitCx::wrap_physics(FnMut(&mut Physics<'_>) -> Result<(), ModError> + Send)`
and `InitCx::replace_physics(…)`; `Physics::{state(), state_mut(), input(),
next()}` with views `PlayerState` (position, velocity, on_ground, sprinting,
fall_distance getters and setters; sneaking, flying, in_water, yaw, pitch
getters) and `Input` (forward, strafe, jump, sneak, sprint, yaw, pitch).
`InitCx::world() -> World` (`Copy + Send + Sync`): `block(x, y, z) ->
Result<Block { id: u16, meta: u8 }, ModError>`, `entities(FnMut(&Entity<'_>)
-> bool) -> Result<(), ModError>` with `Entity::{id, kind, position, yaw,
pitch, name}`. `ModError::Unavailable` for `UNAVAILABLE`.

### Example `mods/examples/physics_tweak`

`movement = true`, `wraps = ["player_physics"]`. A slider "Fall speed" (0.5
to 1.0, default 1.0: at the default the step is untouched): after `next()`,
a falling player's downward velocity is scaled. In its `PostTick`
intercept it reads the block under the feet and the nearest other player
with the world queries and stores them for a HUD widget ("Physics tweak":
block under you, nearest player and distance).

### Client

`GameMods` gains `physics: Option<PhysicsChain>`. A `ModPhysics` adapter
implements `PhysicsOverride` (Player ↔ `UmiPlayerPhysics`, writing back only
changed fields) and is set on every new `GameState` when the chain exists;
with no active layer it calls `vanilla` straight away. The game thread lends
a `WorldQuery` over `World` and `EntityStore` (`with_world`) around the game
tick (physics included), the tick events, the chat events and the commands.

## F3.7 interface (frozen before the work is split)

API **0.6**: host functions `register_post_pass(me, key, label, wgsl)` (init
only; key `^[a-z][a-z0-9_]{0,63}$`; `CONFLICT` for a key the mod already used;
`INVALID_ARGUMENT` with the naga error in the log for a shader that breaks the
contract) and `post_set_param(me, key, index 0..=3, x, y, z, w)` (any thread).
No permission: a pass only transforms the picture of the world it is given.

### The shader contract (owned by `umicraft-modhost`, implemented by `umicraft-render`)

The host prepends this prelude, verbatim, to the mod's source:

```wgsl
struct UmiPost {
    resolution: vec2<f32>,
    time: f32,
    frame: u32,
    params: array<vec4<f32>, 4>,
};
@group(0) @binding(0) var umi_scene: texture_2d<f32>;
@group(0) @binding(1) var umi_sampler: sampler;
@group(0) @binding(2) var<uniform> umi: UmiPost;

struct UmiVertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn umi_vs(@builtin(vertex_index) index: u32) -> UmiVertexOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: UmiVertexOut;
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}
```

The mod supplies `@fragment fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32>`
and helpers. The composed module must parse and validate with naga
(`ValidationFlags::all()`, default capabilities), have exactly the two entry points
`umi_vs` (vertex) and `fs_main` (fragment), and declare no resource beyond the
three of the prelude. Source at most 64 KiB. The uniform is 80 bytes
(`resolution` in pixels, `time` in seconds since the client started, `frame` a
counter, `params` set by the mod, zero until set). The pass draws one triangle
(3 vertices, no vertex buffer) into a target of the scene's format; `uv` is 0..1,
y down; the sampler is linear, clamp to edge.

### `umicraft-render` (files `crates/umicraft-render/**`)

- `post_chain.rs`: `PostPassSource { label: String, wgsl: String }`,
  `PostPassFrame { enabled: bool, params: [[f32; 4]; 4] }`,
  `Renderer::set_post_passes(&mut self, passes: &[PostPassSource]) ->
  Vec<Result<(), String>>` (compiles once, inside a wgpu validation error
  scope; a pass that fails is kept but never drawn, its error returned),
  `Renderer::update_post(&mut self, frames: &[PostPassFrame])` (per frame, no
  allocation; extra or missing entries mean disabled).
- In `draw`: when at least one pass is enabled, the world is drawn into the
  scene texture (as for the blurs), the hand is drawn into it, then the passes
  run in order, ping-ponging between the scene and one extra full-size target so
  that the result ends in the scene texture; motion blur and the menu blur then
  read it as today, and without them the scene is copied to the screen before the
  screen overlays and the HUD, which stay sharp. With no enabled pass the frame is
  exactly as before (no extra pass, no allocation, no texture while the chain is
  empty).

### `umicraft-modhost`

`src/post.rs`: `PRELUDE` (the text above), `compose(source) -> String`,
`validate(source) -> Result<String /*composed*/, PostShaderError>` (typed
error: too long, not UTF-8, parse, validation, entry points, extra resources;
naga messages kept), registration in the mod context (sealed at init like the UI
declarations), params in a lock-free or short-lock store readable from the render
thread, `ModHost::post_passes(&self) -> impl Iterator<Item = PostPassView<'_>>` in
load then registration order (`mod_id`, `key`, `label`, `wgsl` composed, `enabled`
= mod enabled) and `PostPassView::params(&self) -> [[f32; 4]; 4]`. New
dependency: `naga` (workspace version of the renderer, `wgsl-in`).

### `umicraft-modapi`

`InitCx::post_pass(key, label, wgsl) -> Result<PostPass, ModError>`;
`PostPass: Copy + Send + Sync`, `set_param(index: u32, value: [f32; 4]) ->
Result<(), ModError>`. Doc points to `docs/modding/post.md`.

### Example `mods/examples/shader_pass`

A sepia/greyscale pass with a slider "Strength" (0 to 1, default 0.5) and a
choice "Tone" (grey, sepia), pushed into `params[0]` from a `PreTick` observer;
a strength of 0 returns the picture unchanged.

### Client

At startup, after the mods load, the client hands `ModHost::post_passes` to
`Renderer::set_post_passes` and logs (and marks failed via the host) a pass the
GPU refused; every frame it fills a reused `Vec<PostPassFrame>` from the views
and calls `update_post`.
