# Umicraft

An independent Rust client for Minecraft: Java Edition 1.8.x servers
(protocol 47). Umicraft is not affiliated with Mojang Studios or Microsoft.

Status: **M2 — world data and first pixels**. The client logs in offline-mode,
decodes chunk/block packets into a compact world, meshes chunks on worker
threads, and renders them with wgpu either in a window or to an offscreen PNG.

## Workspace

| Crate | Responsibility |
|---|---|
| `umicraft-core` | Module registry, typed services, event bus, fixed-timestep scheduler, config |
| `umicraft-protocol` | Protocol 47 packets, framing, compression, encryption |
| `umicraft-net` | Tokio connection, login, ping, streaming play session |
| `umicraft-world` | Block states, paletted chunk columns, world updates |
| `umicraft-mesh` | Face-culling chunk mesher and texture-layer mapping |
| `umicraft-render` | wgpu renderer, camera, procedural textures, debug overlay |
| `umicraft-client` | Client binary: windowed play or offscreen screenshot |
| `umicraft-testkit` | Fake server, packet capture/replay, test utilities |

## Prerequisites

- A current stable Rust toolchain (edition 2021).
- A working wgpu backend: Vulkan on Linux, Metal on macOS, DX12 on Windows.
  A software Vulkan implementation (llvmpipe/Lavapipe) works for the offscreen
  screenshot path.
- An offline-mode 1.8.x server for live testing. The development notes use a
  Paper/PandaSpigot 1.8.8 server on `127.0.0.1:25599` with an FLAT world.

## Run the graphical client

```sh
cargo run -p umicraft-client -- 127.0.0.1 25599 --name Umicraft --view-distance 8
```

Controls:

- **WASD** move, **Space/Shift** up/down, **Ctrl** fast.
- **Right-drag** capture the mouse and look; **arrow keys** look without capture.
- **F1** toggle the debug overlay, **R** reset the camera to spawn.
- **Esc** release the mouse, or quit when the mouse is not captured.

The overlay shows FPS/frame time, position/rotation, chunk/mesh/face counts,
chat/block-update/packet counters, and a frame-time graph.

## Render an offscreen screenshot

```sh
cargo run -p umicraft-client -- 127.0.0.1 25599 --name UmicraftM2 \
  --view-distance 8 --screenshot /tmp/umicraft.png \
  --collect-secs 8 --width 1280 --height 720
```

Screenshot mode collects world packets for the requested duration, meshes all
loaded chunks through the worker pool, renders one frame, and writes an RGBA
PNG. It does not need a window or a display server.

## Headless protocol client

The M1 acceptance driver stays available for checking login/keepalive behavior
without a GPU:

```sh
cargo run -p umicraft-net --bin umicraft-headless -- 127.0.0.1 25599 \
  --name Umicraft --stay-secs 60
```

## Quality gates

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

The GPU smoke test is opt-in because it needs a working adapter:

```sh
cargo test -p umicraft-render offscreen_smoke -- --ignored --nocapture
```

## Documentation

- `docs/threading.md` — network/mesh/render thread boundaries and queue sizes.
- `docs/sources.md` — provenance for protocol and behavior facts.
- `docs/perf-log.md` — benchmark baselines.
- `docs/decisions/` — architecture decision records.
- `UMICRAFT_SPEC.md` (repository root) — requirements and roadmap.
