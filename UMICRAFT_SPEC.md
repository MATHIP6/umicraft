# Umicraft — Project Specification & Roadmap

> **Audience:** this document is written for human contributors and autonomous coding agents.
> Read it fully before writing code. Sections marked **MUST / MUST NOT / SHOULD** are normative.

---

## 1. Overview

**Umicraft** is an independent, from-scratch, multiplayer-focused client compatible with **Minecraft: Java Edition 1.8.x servers (protocol version 47)**. It is written in **Rust**, designed for **maximum performance** and **multithreading**, and exposes a **deep native modding system** based on dynamic libraries.

Umicraft is **not affiliated with, endorsed by, or connected to Mojang Studios or Microsoft**.

### 1.1 Vision

A fast, modern, hackable client for people who play on 1.8 servers (PvP, minigames, private servers) and want:

- far better frame times and lower latency than the official client,
- the ability to change almost anything through mods,
- fast account switching without restarting the client.

### 1.2 Goals

1. Connect to and play on standard 1.8 servers (Paper/Spigot/vanilla, behind BungeeCord/Velocity proxies).
2. Be the fastest 1.8-compatible client we can build (see §7).
3. Be fully multithreaded with a clear, documented threading model.
4. Provide a powerful, versioned native modding API (dynamic libraries).
5. Provide a multi-account system (Microsoft/premium and offline accounts) with live switching.
6. Never redistribute Mojang/Microsoft assets or code (see §3).

### 1.3 Non-goals

- Singleplayer, integrated server, world generation, mob AI, redstone simulation. The server owns game logic.
- Supporting Minecraft versions other than 1.8.x (protocol 47) in v1.
- Bedrock Edition.
- Being a cheat client. Umicraft MUST NOT ship with built-in cheats (see §10).
- Reproducing the official client's code structure. Umicraft is a reimplementation of *behavior*, not of source.

---

## 2. Glossary

| Term | Meaning |
|---|---|
| Protocol 47 | Network protocol version used by Minecraft 1.8.x |
| Chunk | 16×16×256 column of blocks, split in 16 sections of 16×16×16 |
| Meshing | Converting block data into GPU vertex data |
| Online-mode server | Server that verifies the player's account with Mojang/Microsoft session servers |
| Offline-mode server | Server that does not verify accounts (LAN, private servers, dev servers) |
| Premium account | Microsoft account that owns Minecraft: Java Edition |
| Offline account | Local profile defined only by a username; usable only on offline-mode servers |
| Core | The minimal Umicraft runtime that loads modules and provides services |
| Module | A unit of functionality loaded by the core (built-in modules and mods use the same mechanism) |
| Mod | A third-party module distributed as a dynamic library |

---

## 3. Legal & Clean-Room Constraints (MUST READ)

> **Amended 2026-10-20 by ADR-0001** (`docs/decisions/0001-reference-source-policy.md`):
> the project owner has explicitly waived rule 1 (decompiled sources as reference) for
> local development with the MCP 1.8 tree. Every other constraint in this section —
> no Mojang assets in repo or releases, no asset mirroring, branding discipline,
> license compatibility, EULA review before any release — remains binding.

These rules are **hard constraints** for every contributor and agent.

1. **No Mojang/Microsoft code.** Do **not** decompile, deobfuscate, copy, translate or paraphrase the official client/server code. Do not use decompiled sources as a reference.
2. **Clean-room method.** Behavior must be derived from: public protocol documentation maintained by the community, observation of network traffic, black-box behavior testing against a running server, and original design. Document the source of any non-obvious behavior in code comments or `docs/sources.md`.
3. **No Mojang assets in the repository or in releases.** No textures, sounds, fonts, language files, shaders, models, or logos from Minecraft. The repository MUST contain only original assets (e.g. a minimal built-in fallback texture set and UI font under a free license).
4. **Assets are downloaded by the user** at first run, from Mojang's official distribution endpoints (see §6.2). Umicraft never mirrors or re-hosts them.
5. **Naming.** The product is called **Umicraft**. Do not use "Minecraft" as part of the product name, binary name, or logo, and do not use Mojang logos/branding. Descriptive references ("compatible with Minecraft 1.8 servers") are acceptable.
6. **Authentication.** Use only official, documented authentication flows. Never bypass, spoof or weaken account verification on online-mode servers.
7. **Licenses.** Every dependency's license MUST be compatible with the project license (decided in §13). Run `cargo deny` in CI.
8. **Review the current EULA and Usage Guidelines** before each public release; they can change at any time.

> This section is an engineering policy, not legal advice. A legal review is recommended before any public or commercial release.

---

## 4. Technology Stack

| Area | Choice | Notes |
|---|---|---|
| Language | **Rust** (stable, edition 2021+) | `unsafe` allowed only when justified and documented |
| Async networking | `tokio` | Dedicated network runtime, separate from game/render threads |
| Rendering | `wgpu` | Vulkan / Metal / DX12 / GL backends |
| Windowing/input | `winit` | |
| Math | `glam` | |
| Compression | `flate2` (or `libdeflater` if benchmarks justify) | zlib for packet compression |
| Encryption | `aes` + `cfb8` | AES-128/CFB8 as required by the protocol |
| NBT | own parser or `fastnbt` | Benchmark before choosing |
| Serialization | `serde` (+ `serde_json`, `toml`) | Configs, manifests, JSON chat |
| HTTP | `reqwest` (rustls) | Auth + asset download |
| Audio | `rodio` or `kira` | Decide in M11 |
| ECS | `hecs` or `bevy_ecs` (standalone) | Decide in M0 by prototype/benchmark |
| GUI | Custom immediate-mode layer or `egui` for menus | Decide before M4; in-game HUD should be custom and cheap |
| Dynamic loading | `libloading` | |
| Stable ABI helpers | `abi_stable` or `stabby`; `cbindgen` for C headers | Decide in M15 |
| Secret storage | `keyring` crate (OS keychain) | Account tokens |
| Profiling | `tracy-client` / `puffin`, `criterion` for benchmarks | |
| Logging | `tracing` | Structured, per-module spans |
| Testing | `cargo test`, `proptest`, integration tests with a local Paper 1.8 server | |

---

## 5. Architecture

### 5.1 Principle: minimal core + modules

The core only provides: module loading, a **service registry**, an **event bus**, a **scheduler**, and configuration. **All features, including built-in ones (chunk rendering, physics, chat, inventory UI), are modules** registered through the same mechanisms exposed to mods. Mods can therefore **replace, wrap, or extend** built-in behavior.

### 5.2 Cargo workspace layout (target)

```
umicraft/
├─ Cargo.toml                 # workspace
├─ crates/
│  ├─ umicraft-core/          # module loader, service registry, event bus, scheduler, config
│  ├─ umicraft-protocol/      # protocol 47: packets, VarInt, framing, compression, encryption
│  ├─ umicraft-net/           # connection management (tokio), login flow, keepalive
│  ├─ umicraft-auth/          # Microsoft/Xbox/Minecraft auth, offline accounts, account store
│  ├─ umicraft-assets/        # asset downloader, indexes, resource pack loading, fallback assets
│  ├─ umicraft-world/         # chunks, block states, palettes, lighting data, world cache
│  ├─ umicraft-mesh/          # chunk meshing (greedy/face culling), mesh pipeline
│  ├─ umicraft-render/        # wgpu renderer, render graph, culling, GPU buffers
│  ├─ umicraft-game/          # entities, player physics, interactions, game state
│  ├─ umicraft-ui/            # GUI: menus, HUD, chat, inventory
│  ├─ umicraft-audio/         # audio engine
│  ├─ umicraft-modapi/        # stable C ABI definitions + Rust helper crate for mod authors
│  ├─ umicraft-client/        # binary: wires everything together
│  └─ umicraft-testkit/       # fake server, packet capture/replay, test utilities
├─ mods/examples/             # example mods (HUD, physics tweak, packet logger)
├─ docs/                      # architecture, protocol notes, sources, modding guide
├─ benches/                   # criterion benchmarks
└─ tools/                     # dev tools (packet inspector, asset inspector)
```

### 5.3 Threading model (target)

| Thread / pool | Responsibility |
|---|---|
| **Main/render thread** | Window events, input, GPU submission. Never blocks on I/O, mods, or meshing |
| **Game thread** | Fixed 20 TPS logic tick, physics prediction, entity state, applying world updates |
| **Network runtime** (tokio) | Socket I/O, framing, decrypt/decompress, packet decode → bounded channels to game thread |
| **Mesh worker pool** | Chunk meshing, prioritized by distance and view direction |
| **Light worker(s)** | Incremental lighting updates |
| **Asset/IO pool** | Downloads, decoding textures/audio, disk cache |
| **Audio thread** | Mixing/playback |
| **Mod worker(s)** | Mod hooks that are not required to run on game/render thread |

Rules:
- Communication via **bounded** channels and double-buffered/snapshot state. No shared locks on hot paths.
- The render thread consumes immutable snapshots/GPU-ready buffers and **never waits** for mods or workers.
- Every cross-thread boundary is documented in `docs/threading.md`.

### 5.4 Service registry & events

- **Services**: named, versioned interfaces (e.g. `ChunkMesher`, `PlayerPhysics`, `PacketPipeline`, `HudRenderer`). A module can **provide**, **replace**, or **wrap** a service.
- **Events**: typed, ordered, optionally **cancellable/mutable** (e.g. `PacketReceived`, `ChatMessage`, `PreTick`, `PostTick`, `PreFrame`, `PostFrame`, `KeyInput`, `AccountChanged`).
- Conflict policy: if two mods replace the same service without declaring compatibility, loading fails with a clear error; `wrap` chains are ordered by declared priority.

---

## 6. Functional Requirements

Priority: **P0** = required for first usable release (v0.1), **P1** = required for v1.0, **P2** = later.

### 6.1 Networking & protocol (protocol 47)

| ID | Requirement | Priority |
|---|---|---|
| NET-1 | Server list ping (status) with MOTD, players, latency | P0 |
| NET-2 | Handshake + login (offline-mode servers) | P0 |
| NET-3 | Packet framing: VarInt length prefix, compression threshold handling | P0 |
| NET-4 | Encryption handshake (AES-128/CFB8) and online-mode login | P0 |
| NET-5 | Play state: decode all clientbound packets needed for rendering and gameplay; encode all required serverbound packets | P0 |
| NET-6 | KeepAlive, disconnect handling with readable reason | P0 |
| NET-7 | Chunk data, multi-block change, block change, bulk chunk, unload | P0 |
| NET-8 | Entity packets (spawn, move, metadata, equipment, destroy, velocity) | P1 |
| NET-9 | Plugin messages (incl. client brand `vanilla`-style or configurable brand) and client settings packet | P0 |
| NET-10 | Proxy compatibility (BungeeCord/Velocity server switching, reconnect handling) | P1 |
| NET-11 | Resource-pack-from-server prompt (accept/deny, SHA-1 check) | P2 |
| NET-12 | Packet capture/replay tooling for tests | P0 |
| NET-13 | Robust parsing: malformed or unexpected packets MUST NOT panic; connection is closed cleanly with a log | P0 |

### 6.2 Accounts & authentication

| ID | Requirement | Priority |
|---|---|---|
| ACC-1 | **Multi-account manager**: add, remove, rename, list accounts | P0 |
| ACC-2 | **Live account switching without restarting the client.** Switching while connected disconnects from the current server (a switch never silently alters an existing session) and the new account is used for the next connection | P0 |
| ACC-3 | **Offline accounts**: user chooses a username; UUID derived deterministically as the standard offline UUID (`UUID v3` of `"OfflinePlayer:<name>"`). Usable on **offline-mode servers only**. Online-mode servers will reject them and Umicraft MUST display a clear, honest message; it MUST NOT attempt to bypass verification | P0 |
| ACC-4 | **Premium accounts** via official Microsoft → Xbox Live → XSTS → Minecraft Services flow, followed by profile fetch (name, UUID, skin/cape) | P0 (offline first, premium by M13) |
| ACC-5 | Online-mode join: session server `join` request with the server hash, then encryption handshake | P0 |
| ACC-6 | Token refresh in the background; expired tokens trigger a re-login prompt, not a crash | P1 |
| ACC-7 | Secure storage: refresh/access tokens in the OS keychain (or encrypted file fallback). Never write tokens in plain text, logs, crash reports, or mod-visible memory | P0 |
| ACC-8 | Account UI: quick-switch dropdown in main menu and in-game pause screen, showing skin head, name, type (Premium/Offline) | P1 |
| ACC-9 | Per-account settings (optional): default server, skin cache | P2 |
| ACC-10 | The auth module is isolated (separate process or at minimum separate crate with no mod API access) so that **mods can never read tokens** | P1 |

Notes for implementers:
- Microsoft authentication requires a registered Azure application with the appropriate Minecraft API access; check current requirements in Mojang/Microsoft documentation before implementing. Support the **device-code flow** first (simpler, no embedded browser).
- Auth endpoints and flow details MUST be verified against current official documentation at implementation time; do not rely on memory.

### 6.3 Assets & resources

| ID | Requirement | Priority |
|---|---|---|
| AST-1 | **First-run asset setup**: the user triggers a download of the required game assets from Mojang's official endpoints (version manifest → version JSON → asset index → asset objects + client resources). Show progress, verify SHA-1 hashes, resume on failure | P0 |
| AST-2 | Alternative: **point to an existing official installation** (`.minecraft` directory) and read assets from there without copying | P0 |
| AST-3 | Local cache in a Umicraft data directory; re-validate by hash | P0 |
| AST-4 | Built-in **original fallback assets** (simple textures/font) so the client starts and renders even before assets are downloaded | P0 |
| AST-5 | Resource pack support (zip/folder), ordered priority, hot reload | P1 |
| AST-6 | Texture atlas construction (block textures), mipmaps, animated textures | P0/P1 |
| AST-7 | Block model/blockstate loading for 1.8 format | P0 |
| AST-8 | Language files, sound index, fonts | P1 |
| AST-9 | Player skins/capes fetched via official profile/texture endpoints; cached; offline accounts use default skins | P1 |

> Implementation note: some assets (e.g. block textures) are shipped inside the version's client package, while others (sounds, language files) are referenced through the asset index. The asset module must handle both. Verify current URLs and formats from official sources at implementation time.

### 6.4 World & rendering

| ID | Requirement | Priority |
|---|---|---|
| RND-1 | Chunk storage with palette + bit-packed sections (or equivalent compact representation) | P0 |
| RND-2 | Chunk meshing on worker threads with face culling between opaque neighbors; greedy meshing where it pays off | P0 |
| RND-3 | Frustum culling per chunk section; occlusion culling (visibility-graph or equivalent) | P1 |
| RND-4 | GPU-driven rendering: large shared vertex/index buffers, indirect draws, minimal draw calls | P1 |
| RND-5 | Block types: full cubes, slabs, stairs, fences, doors, crops, liquids, plants, rails, etc. via 1.8 model rules | P0/P1 |
| RND-6 | Lighting: sky + block light with smooth lighting and ambient occlusion | P1 |
| RND-7 | Transparency (water, glass, leaves) with correct ordering | P1 |
| RND-8 | Sky, sun/moon, clouds, fog, weather | P1 |
| RND-9 | Entity rendering: players (skins), items, common mobs, armor, held items | P1 |
| RND-10 | Particles | P2 |
| RND-11 | Configurable render distance, FOV, vsync, FPS cap, quality presets | P0 |
| RND-12 | Render graph architecture so mods can add/replace passes and shaders | P1 |
| RND-13 | Shaderpack-like extensibility through mods | P2 |

### 6.5 Gameplay (client side)

| ID | Requirement | Priority |
|---|---|---|
| GAM-1 | Player movement with **client-side prediction matching 1.8 movement rules exactly at 20 TPS** (walking, sprinting, sneaking, jumping, swimming, ladders, ice, etc.) and server reconciliation | P0 |
| GAM-2 | Collision with block AABBs | P0 |
| GAM-3 | Block breaking and placing (ray casting, digging packets, animations) | P0 |
| GAM-4 | Attack/interact entities, item use, hotbar | P1 |
| GAM-5 | Inventory and container GUIs (window click protocol) | P1 |
| GAM-6 | Chat: send/receive, JSON chat components, colors, commands, tab-complete | P0 (basic) / P1 |
| GAM-7 | Player list (tab), scoreboard, titles, boss bars, action bar | P1 |
| GAM-8 | Smooth, frame-rate-independent camera and interpolated rendering between ticks | P0 |

### 6.6 UI

| ID | Requirement | Priority |
|---|---|---|
| UI-1 | Main menu: server list (add/edit/delete/ping), direct connect, account switcher, settings | P0 |
| UI-2 | In-game HUD: hotbar, health/food/armor/XP, crosshair, chat, debug overlay | P0/P1 |
| UI-3 | Settings: video, controls (rebindable), audio, mods, accounts | P1 |
| UI-4 | Debug overlay: FPS, frame time graph, TPS, chunk/mesh queue sizes, per-thread timings, per-mod timings | P0 |
| UI-5 | Gamepad support | P2 |
| UI-6 | Localization-ready (no hard-coded user-facing strings outside a string table) | P1 |

### 6.7 Audio

| ID | Requirement | Priority |
|---|---|---|
| AUD-1 | Sound effect playback with 3D positioning, volume categories | P1 |
| AUD-2 | Music/ambient playback | P2 |

---

## 7. Non-Functional Requirements & Performance

Targets are **goals to validate with benchmarks**, not guarantees. Establish a baseline in M2 and track regressions in CI.

| Area | Target |
|---|---|
| Frame time | Stable, no hitches > 2× median frame time during normal play; uncapped FPS option |
| Startup | Main menu usable in < 2 s on a typical SSD (assets cached) |
| Chunk meshing | Mesh a chunk section in sub-millisecond to low-millisecond range on one worker; scale across cores |
| Network | Packet decode off the render thread; no allocation per packet on the hot path where avoidable |
| Memory | Compact chunk storage; bounded caches; no unbounded queues |
| Input latency | Input sampled and applied with minimal buffering; render-ahead limited |
| Stability | No panics from server-provided data; fuzz the packet decoder |
| Portability | Windows, Linux, macOS (x86_64 and aarch64) |

### 7.1 Required optimization techniques

- Compact chunk storage (palettes, bit-packing), section-level dirty flags.
- Multi-threaded meshing with priority queue (view direction + distance), cancel stale jobs.
- Face culling, greedy meshing (benchmarked), optional AO baked into vertices.
- Frustum + occlusion culling; LOD-free but distance-limited work.
- Single large GPU buffers with sub-allocation; indirect/multi-draw; texture atlas or array textures.
- Persistent mapped / staged uploads, upload budget per frame.
- Arena/pool allocators; avoid per-frame heap allocation in game loop and render loop.
- Cache-friendly data layouts (SoA where it matters), avoid `Arc<Mutex<_>>` in hot paths.
- Fixed-timestep game loop with interpolation for rendering.
- Zero-copy or low-copy packet parsing where practical.
- Profiling hooks (spans) everywhere important; **measure before and after every optimization**, record results in `docs/perf-log.md`.

---

## 8. Modding System (Dynamic Libraries)

### 8.1 Design goals

- Mods can modify **nearly everything**: replace/wrap services, hook events, intercept/modify/inject packets, add render passes and shaders, add UI, override data and assets.
- **Native speed**: mods are compiled shared libraries (`.dll` / `.so` / `.dylib`).
- Stable, versioned **C ABI** so mods survive client updates within an API major version and can be written in Rust, C, C++, Zig, etc.

### 8.2 Mod package layout

```
my_mod/
├─ mod.toml                  # manifest
├─ bin/
│  ├─ windows-x86_64/my_mod.dll
│  ├─ linux-x86_64/libmy_mod.so
│  ├─ macos-aarch64/libmy_mod.dylib
│  └─ ...
└─ data/                     # optional declarative data: models, textures, lang, configs
```

`mod.toml` (example):

```toml
[mod]
id = "example_hud"
name = "Example HUD"
version = "0.1.0"
authors = ["Someone"]
api_version = "1"            # Umicraft Mod API major version required
license = "MIT"

[dependencies]
other_mod = ">=0.2, <1.0"

[load]
priority = 100               # lower loads earlier
replaces = []                # service ids this mod replaces
wraps = ["hud_renderer"]     # service ids this mod wraps

[permissions]                # declared, shown to the user, not a security boundary
network_packets = false      # may read/modify/inject packets
filesystem = false
network_http = false
```

### 8.3 ABI contract (sketch)

```rust
// umicraft-modapi (repr(C), no Rust-only types across the boundary)
#[repr(C)]
pub struct UmiApi {
    pub api_version: u32,
    pub size: usize,                     // for forward-compatible struct growth
    pub log: extern "C" fn(level: u32, msg: *const u8, len: usize),
    pub register_event: extern "C" fn(event_id: u32, cb: *const c_void, prio: i32) -> i32,
    pub register_service: extern "C" fn(/* id, vtable ptr, mode */) -> i32,
    pub get_service: extern "C" fn(/* id */) -> *const c_void,
    pub alloc: extern "C" fn(size: usize, align: usize) -> *mut u8,
    pub free: extern "C" fn(ptr: *mut u8, size: usize, align: usize),
    // ... grows over time; only append, never reorder within a major version
}

// Exported by every mod:
//   int32_t umi_mod_init(const UmiApi* api);
//   void    umi_mod_shutdown(void);
```

Rules:
- Only `repr(C)` types, primitive types, pointers + lengths across the boundary. **No** `String`, `Vec`, trait objects, or generics (unless wrapped by `abi_stable`/`stabby` and agreed in M15).
- **No panic may cross the FFI boundary**: wrap every entry/exit in `catch_unwind`; a panicking mod is disabled and reported.
- Memory ownership: memory allocated by one side is freed by the same side (or through the provided `alloc`/`free`).
- Provide generated C headers (`cbindgen`) and a Rust helper crate `umicraft-modapi` with safe wrappers and a `umicraft_mod!` macro.
- API versioning: semver on the **API**, check at load time, refuse incompatible mods with a clear message.

### 8.4 Mod capabilities (extension points)

1. **Events** (cancellable/mutable): input, tick, frame, chat, packets in/out, world/chunk updates, GUI open/close, account change.
2. **Services**: replace or wrap built-in services (physics, mesher, HUD, packet pipeline…).
3. **ECS**: register components and systems, control ordering (`before`/`after`).
4. **Rendering**: add/replace render graph passes, shaders, vertex formats, post-processing.
5. **UI**: add screens, widgets, HUD elements.
6. **Data/assets**: override models, textures, lang, configs by priority.
7. **Commands & keybinds**: register client commands and rebindable keys.

### 8.5 Performance rules for mods

- Hot paths (meshing inner loops, per-block, per-vertex, per-packet-byte) MUST NOT call mod code. Mods customize hot paths through **declarative data** or by **replacing the whole service** with native code.
- Per-frame hooks have a **time budget**; over-budget hooks are logged and can be auto-disabled.
- Batched events (arrays per tick) are preferred to one call per item.
- Mods run on a dedicated thread unless they hook the game/render thread explicitly; the render thread never blocks on a mod.
- **Per-mod profiler** (time per hook, memory) visible in the debug overlay.

### 8.6 Safety model (native mods are NOT sandboxed)

Native dynamic libraries run with the user's privileges. Therefore:

- Auth/token handling is isolated (ACC-10); mods cannot read it through any API.
- Mods are **signed** when distributed through an official index; unsigned mods require explicit user confirmation showing the file hash.
- Declared permissions are displayed at install time (informational, not enforced).
- The client **never auto-downloads or auto-loads mods** requested by a server.
- A **safe mode** (`--safe-mode` or after a crash) starts with all mods disabled.
- **Server-compat mode**: option to disable mods that alter movement/packets, so users can comply with server rules.
- Crash reports list loaded mods and versions.

*(Future option: Wasm-based "safe mods" for low-trust scripting, alongside native mods.)*

---

## 9. Configuration & Data Locations

- Config: `config/umicraft.toml` plus per-module files. Human-editable, validated, with defaults.
- Data dir (platform-specific, via `directories` crate): accounts metadata, asset cache, logs, crash reports, mods.
- Logs: `tracing` with rotation; secrets are redacted by construction (types holding tokens do not implement `Debug`/`Display` with contents).

---

## 10. Security & Fair-Play Policy

- Umicraft ships **no cheating features**. Features whose primary purpose is gaining an unfair advantage (x-ray, aimbot, kill-aura, etc.) MUST NOT be part of the core or official example mods.
- The mod API is powerful by design; the project documents that using mods against a server's rules may get accounts banned. The UI shows which loaded mods affect movement/packets.
- The client sends a **realistic, configurable client brand** and standard settings packets and follows standard packet ordering, to behave like a normal client on servers.
- Fuzz and test the decoder; treat all server data as untrusted.

---

## 11. Testing & Quality

- **Unit tests** for VarInt, framing, compression, encryption, NBT, chunk palettes, physics steps.
- **Property tests** (`proptest`) and **fuzzing** (`cargo fuzz`) for packet decoding and chunk parsing.
- **Integration tests**: CI spins up a local Paper 1.8 server (offline-mode) and runs scripted scenarios (login, receive chunks, move, chat). A headless mode of the client is required for this.
- **Packet capture/replay** from `umicraft-testkit` for regression tests.
- **Physics conformance tests**: recorded trajectories compared with expected positions from a real server.
- **Benchmarks** (`criterion`) for meshing, decoding, chunk storage; CI warns on regressions.
- **CI**: `cargo fmt`, `clippy -D warnings`, tests, `cargo deny`, builds for Win/Linux/macOS.
- **Docs**: every public crate API documented; `docs/` kept in sync with behavior.

---

## 12. Roadmap

Each milestone has **deliverables** and **acceptance criteria**. Do not start a milestone's optimization work before its functional criteria pass.

### 12.1 Milestone order

The milestones are **not** executed in the order of their numbers. The visual
work comes first, because the interface work and the entity work both depend
on it, and the account work comes last, because it depends on the interface
and blocks nothing else.

| Order | Milestone | Was | Depends on |
|---|---|---|---|
| 1 | M0 — Foundations | M0 | — |
| 2 | M1 — Protocol & offline login | M1 | M0 |
| 3 | M2 — World data & first pixels | M2 | M1 |
| 4 | M3 — Player movement & interaction | M3 | M2 |
| 5 | **M4 — Interface foundation** | new | M3 |
| 6 | **M5 — The 1.8 model system & the texture atlas** | M6 (1st part) | M4 |
| 7 | **M6 — Lighting, sky, fog & transparency** | M6 (2nd part) | M5 |
| 8 | **M7 — First-run asset setup & resource packs** | M6 (3rd part) | M4, M5 |
| 9 | **M8 — Entities** | M7 (1st part) | M5 |
| 10 | **M9 — Inventory & container interfaces** | M7 (2nd part) | M4, M5 |
| 11 | **M10 — Chat, scoreboard, settings & keybinds** | M7 (3rd part) | M4 |
| 12 | **M11 — Audio** | M7 (4th part) | M4 |
| 13 | **M12 — Accounts, account manager & main menu** | M4 | M4 |
| 14 | **M13 — Premium authentication & online-mode servers** | M5 | M12 |
| 15 | M14 — Performance pass | M8 | M13 |
| 16 | M15 — Modding API v1 | M9 | M14 |
| 17 | M16 — Hardening & v1.0 release | M10 | M15 |

**Rationale.** The 1.8 block models and the texture atlas (M5) are the
foundation of the entity rendering (M8) and of the item textures of the
inventory (M9). The interface foundation (M4) is shared by the first-run
setup (M7), the inventory (M9), the settings (M10), the audio menus (M11) and
the main menu (M12), so it is written once and reused. The menus (M12) are
built on the real 1.8 font and the real assets, so they look like a game
rather than a debug tool. Premium authentication (M13) is the milestone with
the most external risk and blocks nothing else, so it goes last.

The old identifiers are kept in the table above for traceability: the
requirement tables of §6 refer to the milestone numbers of the original plan.

### 12.2 Milestones

### M0 — Foundations (workspace & decisions)
- Create workspace, CI, lint config, license files, `docs/` skeleton, `docs/sources.md`.
- Prototype/benchmark and decide: ECS (`hecs` vs `bevy_ecs`), GUI approach, NBT approach.
- Implement `umicraft-core`: module loader (static modules first), service registry, event bus, scheduler, config.
- **Accept:** `cargo test` + CI green; a dummy module can register a service and an event handler.

### M1 — Protocol & offline login
- `umicraft-protocol`: VarInt, framing, compression, packet definitions for handshake/status/login/play (subset).
- `umicraft-net`: server ping, offline-mode login, KeepAlive, clean disconnect.
- Headless client mode; packet logger tool; packet capture/replay in testkit.
- **Accept:** headless client connects to a local Paper 1.8 offline server, stays connected for 10 minutes, shows server MOTD in ping; fuzz tests on decoder run clean.

### M2 — World data & first pixels
- `umicraft-world`: chunk parsing (chunk data + bulk), block changes, compact storage.
- `umicraft-render`: window, wgpu init, camera, fallback textures, naive mesher (face culling).
- Debug overlay (FPS, frame time).
- **Accept:** player joins a flat world and sees terrain with free camera; baseline benchmarks recorded in `docs/perf-log.md`.

### M3 — Player movement & interaction
- Player physics with prediction/reconciliation (GAM-1/2/8), input, interpolated rendering.
- Block break/place, basic chat send/receive, basic HUD.
- **Accept:** can walk, jump, sprint, place/break blocks on a Paper 1.8 server without rubber-banding or kicks over a 30-minute session.

### M4 — Interface foundation
- An immediate-mode widget layer over the render overlay: buttons, text fields with a caret and editing, scrollable lists, panels, focus and hit testing.
- The real 1.8 font (`assets/minecraft/textures/font/ascii.png` of the downloaded client jar) with the built-in 3×5 font as the fallback used before the assets are installed. The font is **downloaded by the user** like every other asset (§3 rule 4) and is never committed (§3 rule 3).
- The layer is usable from the windowed and from the headless path.
- **Accept:** a debug screen exercises a list, a text field and a button in both paths; the text field edits text with a visible caret; the layout maths are unit-tested; `cargo deny` stays green (no new dependency outside the allow list).

### M5 — The 1.8 model system & the texture atlas
- Parse the 1.8 client jar: the 340 `blockstates` (`variants`) and the 1080 `models/block` entries, with the `parent` chain, `textures`, `ambientocclusion` and the `elements` format used by 252 of them.
- Implement the parent templates: `cube_all`, `cube`, `cube_column`, `cube_bottom_top`, `cross`, `crop`, `half_slab`, `upper_slab`, `stairs` and their inner/outer variants, the eight panes, `carpet`, `fence`, `fence_gate`, `iron_bars`, `plant`, `torch`, `rail`, `ladder`, `vine`, `snow_layer`, `cactus`, `water`, `leaves`.
- The block id → model name table. 1.8 ships no registry file, so it is derived from the reference implementation and every entry is cited in `docs/sources.md` (ADR-0001).
- The texture atlas: the 382 block textures and the 229 item textures of the client jar into the renderer's texture array; the mesher takes its texture lookup from the atlas instead of the hard-coded fallback table.
- **Accept:** the common 1.8 blocks (stone, dirt, grass, planks, cobblestone, sandstone, bricks, glass, leaves, water, ores, obsidian, glowstone, wool, pumpkin, melon, netherrack, quartz) render with their real textures and their real shapes; a slab is no longer a full cube and a stair has its two steps; the procedural fallback still renders when no asset is installed; the atlas build and the model resolution are unit-tested.

### M6 — Lighting, sky, fog & transparency
- Use the block and sky light nibbles the world already stores: smooth lighting and ambient occlusion (RND-6).
- The sky, the sun and the moon, the clouds and the fog (RND-8).
- The transparency with the correct ordering for the water, the glass and the leaves (RND-7).
- **Accept:** the world reads correctly at night and inside a cave; the water and the glass are transparent and correctly sorted; the ambient occlusion is visible at the block corners; the frame time does not regress by more than 10 % against the M2 baseline in `docs/perf-log.md`.

### M7 — First-run asset setup & resource packs
- The first-run setup: the version manifest, the asset index, the client jar, the download with SHA-1 verification, the resume on failure, the progress and the cache in the data directory (AST-1/2/3). The transport exists since the M3 round; this milestone adds the user-facing flow.
- Resource packs: the zip or the folder, the ordered priority, the hot reload (AST-5).
- **Accept:** a fresh data directory downloads the 1.8 assets, verifies every SHA-1, resumes an interrupted download, and the client renders with them; a resource pack overrides a block texture and can be reloaded without restarting; the CI guard that fails when a Mojang asset is committed stays green.

### M8 — Entities
- The remaining entity packets (NET-8): spawn object, spawn mob, the typed entity metadata model, entity properties.
- The entity rendering: the players with their skins (AST-9), the items, the common mobs, the armour and the held items (RND-9).
- The animations.
- **Accept:** the mobs and the players of a live server appear with their real models and skins and move smoothly; the entity metadata is decoded into a typed model instead of raw watchers; the world rendering does not regress.

### M9 — Inventory & container interfaces
- The window protocol: open and close window, window items, set slot, window property, click window, confirm transaction, the creative inventory action (GAM-5).
- The inventory and container GUI rendering.
- **Accept:** the inventory opens with the real slots, the items can be moved, the container GUIs of a server open and close cleanly, and the transaction ids are acknowledged so the server does not desynchronise.

### M10 — Chat, scoreboard, settings & keybinds
- The full chat: the JSON components, the colours, the commands, the tab completion (GAM-6).
- The player list, the scoreboard, the titles, the boss bars and the action bar (GAM-7).
- The settings screens: video, controls with rebindable keys, audio, mods, accounts (UI-3).
- **Accept:** the chat renders the formatted components, the tab completion answers, the scoreboard and the titles display, and a key can be rebound and the rebinding survives a restart.

### M11 — Audio
- The audio engine (the crate is chosen in its ADR), the sound index, the 3D positioning and the volume categories (AUD-1), the music and the ambient sounds (AUD-2).
- **Accept:** the footsteps, the block breaks and the interface clicks play at the right volume and are positioned in 3D; the audio thread never blocks the game or the render thread.

### M12 — Accounts, account manager & main menu
- `umicraft-auth`: the account store, the offline accounts with the standard offline UUID (UUID v3 of `OfflinePlayer:<name>`), the multi-account manager (ACC-1/2/3).
- The main menu: the server list with the ping, the direct connect, the account switcher (UI-1, ACC-8).
- The in-game pause screen with the quick account switch.
- **Accept:** three offline accounts can be added, renamed, removed and switched without restarting; each connects with its own username and UUID; switching while connected disconnects cleanly and the next connection uses the new account; an offline account refused by an online-mode server produces a clear message and no attempt to bypass the verification (ACC-3).

### M13 — Premium authentication & online-mode servers
- A short spike first: it validates the current Microsoft endpoints, the device-code flow and the licence of the secret-storage dependency before the milestone commits to them.
- The Microsoft device-code flow → Xbox Live → XSTS → Minecraft Services → profile (ACC-4).
- The encryption handshake and the session join (ACC-5, NET-4); the token refresh, the keychain storage, the auth isolation (ACC-6/7/10).
- **Accept:** a premium account joins an online-mode 1.8 server; the premium and the offline accounts coexist in the manager and swap live; the tokens never appear in the logs; the mods cannot read them.

### M14 — Performance pass
- Profile-driven work: multi-threaded meshing tuning, culling (frustum + occlusion), GPU-driven rendering, upload budgeting, allocation audit.
- Add regression benchmarks to CI.
- **Accept:** documented before/after numbers in `docs/perf-log.md`; targets in §7 met or deviations explained.

### M15 — Modding API v1
- `umicraft-modapi` (C ABI + headers + Rust helpers), dynamic loader (`libloading`), manifest parsing, dependency/priority resolution, conflict detection.
- Convert built-in features to modules loaded through the same path (as far as practical).
- Per-mod profiling, panic isolation, safe mode, mod manager UI, signing/hash confirmation.
- Example mods: custom HUD, physics tweak (wrap service), packet logger (permission-gated), shader pass.
- **Accept:** example mods load/unload, a crashing mod is isolated and disabled, a mod can replace a built-in service, API docs + template repo published.

### M16 — Hardening & v1.0 release
- Proxy compatibility (BungeeCord/Velocity), reconnect flows, long-session stability, memory leak hunting.
- Packaging/installers, auto-update (opt-in), crash report tooling, documentation site.
- Legal review checklist (§3) completed; license decided.
- **Accept:** release checklist in `docs/release.md` fully ticked.

### Post-1.0 ideas
Wasm "safe mods"; more languages for mod templates; Bedrock-style data-driven UI; replay recording; additional protocol versions behind a protocol abstraction.

---

## 13. Open Decisions (to be recorded in `docs/decisions/`)

1. Project license (permissive MIT/Apache-2.0 vs copyleft GPL) — affects mod ecosystem and trust. *(Decided: ADR-0002.)*
2. ECS crate choice. *Blocks M8 (the entity rendering).* *(Decided: ADR-0015 — typed `EntityStore` with immutable render snapshots).*
3. GUI approach (`egui` for menus vs fully custom). *Blocks M4 (the interface foundation).* *(Decided: ADR-0010 — immediate-mode `umicraft-ui`).*
4. ABI helper (`abi_stable` vs `stabby` vs pure C ABI). *Blocks M15.*
5. Audio crate. *Blocks M11.*
6. Distribution channel for mods (official index, signing authority). *Blocks M15.*
7. Whether auth runs in a separate process (preferred) or a hardened module. *Blocks M13.*

Each decision: write a short ADR (context, options, decision, consequences).

---

## 14. Working Rules for Agents & Contributors

1. **Read §3 first.** If a task would require looking at Mojang's code or bundling their assets, stop and flag it instead.
2. Work **milestone by milestone**; keep PRs small and focused; each PR includes tests and docs updates.
3. **No `unwrap()`/`expect()` on external data.** Server and file input is untrusted.
4. **Never log or serialize secrets.** Token-holding types must not implement `Debug`/`Display` revealing contents.
5. Keep the **hot paths allocation-free** and mod-free; add a benchmark for anything claimed as an optimization.
6. Document non-obvious protocol/physics behavior with its **source** (public doc, observed traffic, test).
7. Keep the public mod API **minimal, versioned, and append-only** within a major version.
8. When a decision is not covered here, write an ADR proposal rather than guessing silently.
9. Prefer correctness against real servers (Paper 1.8) over theoretical elegance; verify with the integration suite.
10. Keep this document updated when requirements or decisions change.
11. **Freeze the interface before splitting the work.** A milestone that several contributors share starts with a short ADR that fixes the public types, the file ownership and the acceptance criteria; the work is then split along those boundaries so that two contributors never edit the same file. A milestone that is too large for that is split into sub-milestones (§12.1).
12. **One build directory per checkout.** Concurrent worktrees MUST NOT share a `CARGO_TARGET_DIR`: the fingerprints collide and cargo links stale artifacts, which fails in confusing ways.

---

## 15. Definition of Done (v1.0)

- Connects to and plays on standard 1.8 servers (offline and online mode) for extended sessions without kicks or crashes.
- Multi-account manager with live switching (offline + premium), tokens safely stored and isolated from mods.
- Assets downloaded by the user at first run (or read from an existing install); repository and releases contain no Mojang assets.
- Performance targets validated and documented.
- Modding API v1 documented, with example mods, safe mode, per-mod profiling, and crash isolation.
- CI green on Windows, Linux, macOS; fuzzing and integration tests in place.
- Legal/clean-room checklist completed.
