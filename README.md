# Umicraft

An independent Rust client for Minecraft: Java Edition 1.8.x servers
(protocol 47). Umicraft is not affiliated with Mojang Studios or Microsoft.

Status: **M4 complete; M5 (1.8 block models and texture atlas) next**. The
client logs in offline-mode, decodes chunk/block packets into a compact world,
runs the player simulation on a game thread, meshes chunks on worker threads,
and renders them with wgpu either in a window or to an offscreen PNG. M4 adds
the backend-agnostic `umicraft-ui` crate, the custom widget layer, the F2
debug screen, and runtime loading of the user's real 1.8 font with an original
fallback.

## Workspace

| Crate | Responsibility |
|---|---|
| `umicraft-core` | Module registry, typed services, event bus, fixed-timestep scheduler, config |
| `umicraft-protocol` | Protocol 47 packets, framing, compression, encryption |
| `umicraft-net` | Tokio connection, login, ping, streaming play session |
| `umicraft-assets` | Official asset transport, verified cache and client-jar access |
| `umicraft-world` | Block states, paletted chunk columns, world updates |
| `umicraft-mesh` | Face-culling chunk mesher and texture-layer mapping |
| `umicraft-render` | wgpu renderer, camera, textures, HUD and UI draw-list pass |
| `umicraft-ui` | Immediate-mode UI: layout, focus, widgets, font atlas, draw list |
| `umicraft-client` | Windowed play, headless session, screenshots and UI demo |
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
- **F2** toggle the M4 debug screen: a text field, a button, a scrollable list
  and a visible caret. The game keeps running while it is open; the mouse is
  released and keyboard/mouse input goes to the UI.
- **Esc** release the mouse, or quit when the mouse is not captured.

The overlay shows FPS/frame time, position/rotation, chunk/mesh/face counts,
chat/block-update/packet counters, and a frame-time graph.

## UI acceptance demo

The M4 debug screen also runs without a window, a server or a GPU. It drives
the real `umicraft-ui` screen through the same `UiInput` vocabulary as the
windowed client and prints a stable report:

```sh
cargo run -p umicraft-client -- --ui-demo
```

Add `--screenshot` to render the same final state offscreen to a PNG:

```sh
cargo run -p umicraft-client -- --ui-demo --screenshot /tmp/umicraft-ui.png
```

If a local official Minecraft installation is found, the real
`assets/minecraft/textures/font/ascii.png` is read from its 1.8.8 client jar at
runtime. Otherwise the original 3×5 fallback font is used and a warning is
printed. No Mojang asset is stored in this repository or in a release.

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
cargo deny check licenses bans
```

The GPU smoke tests are opt-in because they need a working adapter:

```sh
cargo test -p umicraft-render offscreen_smoke -- --ignored --nocapture
cargo test -p umicraft-render --test offscreen_ui -- --ignored
cargo test -p umicraft-client -- --ignored
```

The real-font decoder test is also ignored by default and reads the path in
`UMICRAFT_FONT_PNG` when it is set:

```sh
UMICRAFT_FONT_PNG=/path/to/ascii.png \
  cargo test -p umicraft-ui --test text -- --ignored
```

## Documentation

- `docs/threading.md` — network/mesh/render thread boundaries and queue sizes.
- `docs/sources.md` — provenance for protocol and behavior facts.
- `docs/perf-log.md` — benchmark baselines.
- `docs/decisions/` — architecture decision records.
- `UMICRAFT_SPEC.md` (repository root) — requirements and roadmap.
