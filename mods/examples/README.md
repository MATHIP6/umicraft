# Example mods

| Example | Shows |
|---|---|
| `hello` | A richer starting point (the smallest one is `mods/template`): logging, `observe` on `PreTick`, `intercept` on `PostTick` and on `ChatSend` (replaces `:shrug:` through `set_text`), a toggle and a colour setting, a HUD widget drawing the tick count in that colour, a key binding (default `KeyH`) that prints a local chat line, the command `/hello <name>`, and `shutdown`. |
| `packet_logger` | The packet API: `observe` on `PacketReceived` and `PacketSend`, per-id counts shared between threads, a once-per-second log line per packet id, a HUD widget with packets/s in and out, and `/swing`, which injects an `Animation` (0x0A) packet through `Packets::send`. Declares `network_packets = true`, the permission every packet event and injection needs; the client shows it as an "alters packets/movement" badge and turns the mod off in server-compat mode. |
| `physics_tweak` | The API 0.5 services: `wrap_physics` around the player's movement step, which calls `next` and only measures (horizontal speed in blocks/s and the height of the last jump), changing nothing so the step stays bit for bit vanilla (spec §10: an official example gives no advantage), and the world queries `World::block` and `World::entities`, run from a `PostTick` intercept (game thread) to show the block under you and the nearest player. A HUD widget shows the four lines. It still declares `movement = true` and `wraps = ["player_physics"]` because wrapping needs them; the client shows the "alters packets/movement" badge and server-compatible mode holds it off. |
| `shader_pass` | The post-process API 0.6: `InitCx::post_pass` registers a WGSL fragment shader (`src/sepia.wgsl`, validated by the host with naga) that turns the picture of the world grey or sepia; a "Strength" slider and a "Tone" choice are pushed into `umi.params[0]` from a `PreTick` observer through `PostPass::set_param`. No permission is needed. Only the world picture is transformed: the HUD and menus stay sharp. Strength 0 leaves the picture unchanged. |
| `depth_fog` | The scene depth of API 1.1 in a post-process pass (`api_version = "1.1"`): `src/fog.wgsl` calls `umi_linear_depth(uv)` to get each pixel's distance in blocks and fades what lies beyond a start distance toward a fixed fog colour. A "Fog starts at" slider and a "Strength" slider are pushed into `umi.params[0]` from a `PreTick` observer. No permission is needed. It only hides detail, so it gives no advantage; the hand reads as very near and stays clear. |
| `world_marker` | API 1.1 annotations: a key binding (default `KeyM`) drops a marker at the player's feet. A `Frame` intercept projects it with `Frame::world_to_screen`, using the exact camera of the frame. A widget anchored with `Anchor::Screen` then draws `Hud::outline`, a short `Hud::segment` and the distance there. The box stays on the point while the camera moves, bobs or zooms. No permission is needed, and the mod shows nothing the player could not see. |
| `free_cam` | The input capture of API 1.3 (`api_version = "1.3"`): a key binding (default `KeyV`) starts a free camera. A `render_camera` wrapper calls `Camera::next`, then flies its own eye with the movement keys (jump up, sneak down) and turns it with the mouse, at a "Speed" set by a slider, and calls `Camera::capture_input` on every frame, so the player stands still and does not turn. Pressed again, the camera returns to the player. Declares `camera = true` and `wraps = ["render_camera"]`. |
| `suite` | The mods menu of API 1.2 (`api_version = "1.2"`): `InitCx::category` creates a "Suite" tab, `InitCx::module` declares a "Tick counter" module in it and a "Greeter" module under "External" (off by default). `InitCx::card` puts a slider and a "Reset" button on the counter's card and attaches its widget, which the host hides while the module is off. The greeter prints a line when it is switched on (`ModuleToggled`) and has a "Wave" button; the mod's own card has an "About" button (`ButtonPressed`). No permission is needed. |
| `camera_orbit` | The `render_camera` service of API 1.1: `InitCx::wrap_camera` lets the client place its camera (`Camera::next`), then, once the `KeyO` binding is pressed, moves the eye on a circle around that point and turns it to look back. A "Distance" slider sets the radius, and the angle advances with `CameraInput::frame_seconds`. Declares `camera = true` and `wraps = ["render_camera"]`. The player, the packets and the built-in mods are untouched. |
| `entity_list` | The entity snapshot of API 1.1 (`api_version = "1.1"`): a `Frame` intercept remembers the camera eye, and a `TopRight` widget lists the ten nearest entities with `World::snapshot` (render thread only): the name or the kind, the distance from the camera to where the frame draws the entity (`render_position`) and, for living ones, a health bar made of two `Hud::tick` rectangles. A debugging view, as the owner decided in ADR-0094: it shows what the client already knows (spec §10). No permission is needed. |
| `entity_glow` | The facts, world layers and entity mask of API 1.4 (`api_version = "1.4"`): a world layer reads `World::local_player` and `World::snapshot` and marks, with `WorldCx::highlight` and always `Depth::Tested`, the living entities within a "Range" of the player: blue for a team mate (`Entity::teammate`), red while hurt (`Entity::is_hurt`), else the team colour (`Entity::team_color`) or white. Its post pass (`src/glow.wgsl`) reads `umi_entity_mask` and draws an outline or a glow around them ("Style" and "Width" settings). Only what the player sees is marked, so it shows nothing more than the screen does (spec §10). No permission is needed. |
| `panicking` | A mod whose `init` panics. The host reports `PANICKED` and keeps running. A test fixture, not a template. |

## Build and install

```sh
cargo build -p umicraft-example-hello --release
```

Create the mod folder inside the client's mods directory (`<data dir>/mods`),
named after the mod id, and copy the manifest and the library:

```
<data dir>/mods/hello/mod.toml
<data dir>/mods/hello/bin/linux-x86_64/libhello.so
```

The platform folder is `<os>-<arch>` (`linux-x86_64`, `windows-x86_64`,
`macos-aarch64`, ...); the file is `libhello.so`, `hello.dll` or
`libhello.dylib`, from `target/release/`.

## Trust confirmation

Native mods run with the client's full rights. The first time a library loads,
the client shows its SHA-256 and asks you to confirm; the hash is then kept in
the trust list. A rebuilt library has a new hash and asks again.

## Reference

- [ADR-0086](../../docs/decisions/0086-modding-api-architecture.md): architecture of the modding API.
- [ADR-0087](../../docs/decisions/0087-modding-abi-contract.md): the C ABI contract.

The installed folder must be named after the mod id, like the source folder: `packet_logger` installs as `<data dir>/mods/packet_logger/` with `libpacket_logger.so`.

`crates/umicraft-modhost/tests/real_mods.rs` builds the examples and loads them
for real.
