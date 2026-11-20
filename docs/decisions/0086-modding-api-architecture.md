# ADR-0086 — Architecture of the native modding API (F3)

Status: accepted (2026-11-19)

## Context

F3 (former M15) delivers the native modding API of spec §8: mods are
dynamic libraries that hook events, replace or wrap services, read the
world and add UI. Three facts shape the design:

- `umicraft-core` already has a `Module` trait, a typed `EventBus` with
  priorities and cancellation, a string-keyed `ServiceRegistry` and a
  `Scheduler`, but nothing in the client instantiates them (M0 skeleton).
- The built-in mods (ADR-0065) are a separate, closed, per-frame system that
  reads a `Copy` `ModContext` snapshot. Their settings schema, HUD placement
  and mods menu are worth reusing; their trait is not the external API.
- The client runs a render thread, a game thread (20 TPS) and a network
  thread connected by bounded channels with no locks on hot paths
  (`docs/threading.md`). The render thread never waits for a mod (spec §8.5).

The roadmap orders F3 after F2 (performance). The project owner started F3
on 2026-11-19 while F2 and the P13 sign-off are still open; F3 work must not
regress the frame time (every dispatch path gets a criterion benchmark).

## Decision

### Crates

| Crate | Role | May depend on |
|---|---|---|
| `umicraft-modapi-sys` | The C ABI: `#[repr(C)]` types, ids, function-pointer types. No code, no dependency. The C header `include/umicraft.h` is generated from it with cbindgen (ADR-0087). | nothing |
| `umicraft-modapi` | The Rust helper for mod authors: safe wrappers, the `Mod` trait, the `umicraft_mod!` macro, `catch_unwind` on every entry point. Mods depend on this crate only. | `umicraft-modapi-sys` |
| `umicraft-modhost` | The host side: `mod.toml` manifests, dependency and conflict resolution, loading with `libloading`, the host function table, budgets, per-mod profiling, safe mode, hash trust list. | `umicraft-modapi-sys`, `umicraft-core` |
| `umicraft-core` | Gains the per-thread event bus and the service registry actually used by the client. Knows nothing about the C ABI. | — |
| `umicraft-client` | Emits the events from its threads, provides the built-in services, shows mods in the mods menu. | all of the above |

`umicraft-modhost` never depends on `umicraft-auth`, the client, wgpu or
winit: the token isolation of ACC-10 follows from the dependency graph, and
the host is testable without a window.

### Events

- **Thread ownership.** Every event belongs to one thread (game, render or
  network). Each thread owns its own `EventBus`; there is no shared lock.
  Handler tables are built once after loading and are immutable; disabling
  a mod flips a per-mod `AtomicBool` that the dispatcher checks.
- **Two modes.** `intercept` handlers run synchronously on the owning
  thread, by ascending priority, and may cancel or modify the event; they
  have a time budget. `observe` handlers receive a copy on the mod's worker
  thread through a bounded channel (dropped when full, counted), cannot
  cancel, and never delay the owning thread.
- **The core stays ABI-agnostic.** `umicraft-modhost` adapts a foreign
  callback into a core handler closure that adds the budget, the profiling
  and the disable-on-failure. Built-in Rust modules and external mods share
  one bus.
- **Payloads.** Events that cross the ABI are `#[repr(C)]` structs from
  `umicraft-modapi-sys`, built directly by the client (no conversion).
  Every payload starts with `UmiEventHeader { size, id, flags }`.

### Memory

No ownership crosses the boundary. The host lends data for the duration of
one call (`UmiStr { ptr, len }`, UTF-8). A mod writes through host functions
(`hud_text(ctx, …)`, `event_set_text(ev, …)`), and the host copies. Neither
side frees memory the other allocated.

### Services

Services are replaced or wrapped as a whole, at most once per tick or per
frame, never per block, vertex or packet byte (spec §8.5). v1 opens
`player_physics`. Two mods replacing the same service without declaring
compatibility fail to load with a clear error (spec §5.4); wrap chains are
ordered by manifest priority.

### Entities: no ECS for mods (amends spec §8.4 item 3)

ADR-0015 chose a typed `EntityStore` owned by the game thread that publishes
immutable snapshots. Spec §8.4 item 3 asked for an ECS for mods. v1 resolves
the contradiction in favour of ADR-0015: mods get **read-only entity
queries** over a `#[repr(C)]` snapshot, and services. Mod-defined components
and systems are out of v1 and may be reopened in a later API major.

### Lifecycle and safety

- **Unload means disable.** `umi_mod_shutdown` is called, the mod's handlers
  and services are removed, its worker stops, but the library stays mapped
  until the client exits. A hot `dlclose` is unreliable (thread-local
  destructors, threads the mod started) and is not offered.
- **Panics.** A Rust panic across `extern "C"` aborts the process (Rust ≥
  1.81), so `umicraft-modapi` catches it on the mod side and returns
  `UMI_STATUS_PANICKED`; the host then disables the mod and reports it. A
  native crash in a C mod cannot be caught: a crash sentinel file written at
  start and removed at clean exit makes the next start offer safe mode.
- **Trust.** v1 has no signing authority. An unknown library is shown with
  its SHA-256 hash and loads only after the user confirms; confirmed hashes
  are kept in a trust list. The signed official index is deferred to F4
  (spec §13 item 6).
- **Permissions** of the manifest are shown, not enforced, except that the
  host refuses packet and service hooks to a mod whose manifest does not
  declare them, and server-compat mode disables mods that declare
  `network_packets` or `movement`.
- **Never** auto-download or auto-load a mod a server asks for (spec §8.6).

### Extension points of v1

| Area | Thread | v1 content |
|---|---|---|
| Lifecycle | game | `PreTick`, `PostTick`, `Connected`, `Disconnected`, `AccountChanged` (name, UUID, kind only) |
| Chat | game | `ChatReceived` (mutable), `ChatSend` (cancellable), client commands |
| Input and screens | render | `KeyInput`, `ScreenOpen`, `ScreenClose`, open key bindings |
| HUD and UI | render | HUD widgets placed with `HudPlacement`, schema settings in the mods menu |
| Packets | network | raw in/out (`id` + bytes) intercept (cancel) and observe; injection re-validated by `PlayServerbound::decode` (F3.5 amendment) |
| Services | game | `player_physics` |
| World | game | read-only block and entity queries, game thread only (F3.6 amendment) |
| Rendering | render | a post-process chain of naga-validated WGSL |
| Data | — | `config/mods/<id>.toml`, `data/` overrides by priority |

Out of v1: replacing the mesher, a general render graph (RND-12), whole new
screens, ECS components.

## Consequences

- Built-in features move onto the bus progressively (F3.9): the built-in
  mods' `ModContext` becomes a `FrameEvent` snapshot.
- The `unsafe` code of the API is confined to one `ffi` module in
  `umicraft-modapi` and one in `umicraft-modhost` (ADR-0087).
- Each phase of `docs/tasks/f3/README.md` adds a benchmark of the dispatch it
  touches; with no mod loaded the cost must be within noise.
- Spec §8.4 item 3 and §13 items 4 and 6 are updated accordingly.

## Amendments

- **2026-11-19, F3.4 — no raw key events.** The `KeyInput` event of the
  extension table is replaced by `KEY_PRESSED`, fired only for the key
  bindings a mod registered, only to that mod, and only while no screen is
  open. A raw key stream would let any mod read what the player types in
  the chat, on signs or in a server's login prompt.
- **2026-11-19, F3.3 — disable is a flag.** `ModHost::disable` only flips the
  mod's flag; `umi_mod_shutdown` runs once, in `shutdown_all`, after every
  dispatching thread has stopped (`docs/tasks/f3/README.md`, quiescence rule).
- **2026-11-19, F3.5 — packets are read and dropped, not rewritten.** The two packet
  events are cancellable but not mutable: most clientbound bodies are decoded only when
  the session handles them and a bad one ends the session, so a replacement could not
  be validated first. A mod changes an outgoing packet by cancelling it and injecting
  the replacement through `packet_send`, which the client's own decoder validates.
  `Set Compression` and `Disconnect` are never dropped. Server-compatible mode is one
  client switch (`multiplayer.mod_server_compat`), not a per-server setting.
- **2026-11-19, F3.6 — world queries without a snapshot.** Mods read the world through
  host functions that answer only on the game thread inside one of their callbacks
  (game-event intercept handlers, commands, physics layers); elsewhere they are
  `UNSUPPORTED`. A `#[repr(C)]` snapshot for the workers is deferred until a mod needs
  one. Service layers are registered per service with `register_physics` (one typed
  function per service, not a generic string-keyed registry), and a replacing layer
  always sits inside every wrapper.
