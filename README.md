# Umicraft

An independent Rust client for Minecraft: Java Edition 1.8.x servers
(protocol 47). Umicraft is not affiliated with Mojang Studios or Microsoft.

Status: **M7 complete** (First-run asset setup & 1.8 resource packs).
The client logs in offline-mode, decodes chunk/block packets into a compact
world, runs the player simulation on a game thread, meshes chunks on worker
threads, and renders them with wgpu either in a window or to an offscreen PNG.
M4 added the backend-agnostic `umicraft-ui` crate, the custom widget layer, the
F2 debug screen, and runtime loading of the user's real 1.8 font with an
original fallback. M5 adds the 1.8 blockstate/model parser, the block registry,
the real block/item texture array, the model mesher, and the server-less
`--block-demo` acceptance path. M6 adds smooth block/sky lighting and
per-vertex ambient occlusion, the `Time Update`-driven sky/sun/moon/cloud/fog
pass with a signed world clock, and a farthest-first translucent pass for
water, glass and leaves. M7 adds the official first-run asset setup with
SHA-1 verification and resume support, 1.8 resource pack layering (folder
or .zip), and hot reloading via `F3 + T` / `F4` (ADR-0014).

## Workspace

| Crate | Responsibility |
|---|---|
| `umicraft-core` | Module registry, typed services, event bus, fixed-timestep scheduler, config |
| `umicraft-protocol` | Protocol 47 packets, framing, compression, encryption |
| `umicraft-net` | Tokio connection, login, ping, streaming play session |
| `umicraft-assets` | Official asset transport, verified cache, client jar and atlas |
| `umicraft-model` | 1.8 blockstates/models, parent chains, block registry, baked quads |
| `umicraft-world` | Block states, paletted chunk columns, world updates |
| `umicraft-mesh` | Cube/model chunk mesher and texture-layer mapping |
| `umicraft-render` | wgpu renderer, camera, textures, HUD and UI draw-list pass |
| `umicraft-ui` | Immediate-mode UI: layout, focus, widgets, font atlas, draw list |
| `umicraft-client` | Windowed play, headless session, screenshots, UI demo and block demo |
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

At startup the client reads the user's official 1.8.8 client jar once, bakes
the blockstate/model set and uploads the real block/item texture array. If the
installation is missing, the jar is corrupt or a model is unsupported, it logs
one warning and keeps the original procedural terrain path; real model meshes
and the fallback texture array are never mixed. No Mojang asset is stored in
this repository or in a release.

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

## Block acceptance demo

`--block-demo` exercises the complete M5 path without a server:

```sh
cargo run -p umicraft-client -- --block-demo
```

It builds a synthetic stone platform with the acceptance blocks (common cubes,
every ore, all 16 wool colours and the slab/stairs/fence/pane/torch/shape row),
meshes it with the real model mesher and prints the ADR-0007 counts:

```text
blockstates       : 340
source models     : 1080
block textures    : 382 (373 png + 9 mcmeta)
item textures     : 229 (227 png + 2 mcmeta)
texture layers    : 601
atlas             : 32x32 RGBA8, 2,461,696 bytes
block states      : ...
biome tints       : on
quads             : ...
fallback variants : ...
```

`biome tints : on` means the jar provided both 256×256 colormaps and the demo
columns use plains (id 1); the report prints `off` when a custom jar omits them
and the terrain keeps its previous grey/white output. The colormaps are read
from the verified user jar at runtime and never stored in the repository or a
release.

The report needs no GPU and no server. Add `--screenshot` to upload the real
texture array, render one offscreen frame and write a 1280×720 PNG:

```sh
cargo run -p umicraft-client -- --block-demo --screenshot /tmp/m5-blocks.png
```

Unlike the other modes, `--block-demo` needs the official resources and exits
with a clear typed error when they are unavailable.

## Render an offscreen screenshot

```sh
cargo run -p umicraft-client -- 127.0.0.1 25599 --name UmicraftM2 \
  --view-distance 8 --screenshot /tmp/umicraft.png \
  --collect-secs 8 --width 1280 --height 720
```

Screenshot mode collects world packets for the requested duration, meshes all
loaded chunks through the worker pool, renders one frame, and writes an RGBA
PNG. It does not need a window or a display server. When the official resources
are available the workers use the model mesher and the renderer samples the
real texture array; otherwise the screenshot falls back to the procedural cubes
and textures as a whole.

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
