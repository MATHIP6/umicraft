<div align="center">

<img src="docs/assets/logo.png" width="96" alt="Umicraft logo">

# Umicraft

An independent Rust client for Minecraft: Java Edition 1.8.x servers
(protocol 47). Umicraft is not affiliated with Mojang Studios or Microsoft.

> **Work in progress.** Umicraft is under active development and is not ready
> for everyday play. Expect missing features, bugs and breaking changes.
>
> **Anti-cheat warning.** Umicraft is not the vanilla client. We try to make
> its packets, timings and movement as close to vanilla as possible, but the
> match is not perfect yet, so server anti-cheats can still flag it, kick you
> or ban you. **Do not connect to large public servers.** Use it on your own
> server or on servers where you have permission, and at your own risk.

**Community:** join the [Discord server](https://discord.gg/7EwBaNDag2) to chat,
report bugs and follow progress.

## Status

The project follows [`ROADMAP.md`](ROADMAP.md), whose goal is parity with the
vanilla 1.8.8 client. We are roughly at **F2**, the performance pass (started
early, see ADR-0058 and `docs/tasks/f2/README.md`), while the remaining parity
phases (P-series) are still being completed.

Already working:

- Offline-mode login and a streaming play session.
- Chunk and block decoding, with chunk meshing on worker threads.
- Real 1.8 block models, textures, smooth lighting and ambient occlusion.
- Sky, sun, moon, clouds, fog and translucent water, glass and leaves.
- Official asset setup with SHA-1 verification, plus resource pack layering and
  hot reloading.
- Player simulation, block breaking and placing, and chat.
- Player skins and first entity models (partial, being reworked in P5).
- A custom UI layer with a title screen and a server list.

Still to come: items and inventory, full entity support, combat, particles,
HUD and options screens, audio and online-mode login. See the roadmap for the
full list.

## Workspace

| Crate | Responsibility |
| --- | --- |
| `umicraft-core` | Module registry, typed services, event bus, fixed-timestep scheduler, config |
| `umicraft-protocol` | Protocol 47 packets, framing, compression, encryption |
| `umicraft-net` | Tokio connection, login, ping, streaming play session |
| `umicraft-assets` | Official asset transport, verified cache, client jar and atlas |
| `umicraft-model` | 1.8 blockstates/models, parent chains, block registry, baked quads |
| `umicraft-world` | Block states, paletted chunk columns, world updates |
| `umicraft-mesh` | Cube/model chunk mesher and texture-layer mapping |
| `umicraft-render` | wgpu renderer, camera, textures, HUD and UI draw-list pass |
| `umicraft-ui` | Immediate-mode UI: layout, focus, widgets, font atlas, draw list |
| `umicraft-client` | Windowed play, headless session, screenshots and demos |
| `umicraft-testkit` | Fake server, packet capture/replay, test utilities |

## Prerequisites

- A current stable Rust toolchain (edition 2021).
- A working wgpu backend: Vulkan on Linux, Metal on macOS, DX12 on Windows.
  A software Vulkan implementation (llvmpipe/Lavapipe) works for the offscreen
  screenshot path.
- An offline-mode 1.8.x server to connect to.

## Download the official assets (first run)

Umicraft ships no Mojang asset. Before the first launch, download and verify
the official 1.8.8 client jar and assets once (SHA-1 checked, resumable):

```sh
cargo run --release -p umicraft-client -- --setup-assets
```

They are stored in the local Umicraft cache and reused on every later run. If
an official `.minecraft/versions/1.8.8` installation already exists, it is used
first and this step is not needed. Without assets the client falls back to
original procedural textures and font. No Mojang asset is stored in this
repository or in a release.

## Run the graphical client

```sh
cargo run --release -p umicraft-client
cargo run --release -p umicraft-client -- 127.0.0.1 25565 --name Umicraft --view-distance 8
```

Without a host and port the window opens on the title screen, where
Multiplayer lists your saved servers. With a host and port it connects at
start. Always use `--release`: a debug build is several times slower.

Controls:

- **WASD** move, **Space** jump, **Shift** sneak, **Ctrl** sprint.
- **Left click** capture the mouse, then break a block; **right click** place
  a block; **1-9** select the hotbar slot; **T** open the chat.
- **Esc** release the mouse, or quit when the mouse is not captured.

## Other modes

```sh
# Render one offscreen frame to a PNG (no window needed)
cargo run --release -p umicraft-client -- 127.0.0.1 25565 --screenshot out.png

# Protocol-only client (no GPU)
cargo run -p umicraft-net --bin umicraft-headless -- 127.0.0.1 25565 --stay-secs 60
```

The `--ui-demo`, `--block-demo` and `--entity-demo` modes run server-less
acceptance scenes used during development.

## Development

Quality gates before any change:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

GPU smoke tests are opt-in because they need a working adapter:
`cargo test -p umicraft-client -- --ignored`.

## Documentation

- [`ROADMAP.md`](ROADMAP.md) — the path to parity with the vanilla client.
- `UMICRAFT_SPEC.md` — requirements and clean-room rules.
- `docs/threading.md` — thread boundaries and queue sizes.
- `docs/sources.md` — provenance for protocol and behavior facts.
- `docs/perf-log.md` — benchmark baselines.
- `docs/decisions/` — architecture decision records.
- `docs/backlog.md` — known gaps and follow-ups.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
