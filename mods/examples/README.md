# Example mods

| Example | Shows |
|---|---|
| `hello` | The template to copy: logging, `observe` on `PreTick`, `intercept` on `PostTick` and on `ChatSend` (replaces `:shrug:` through `set_text`), a toggle and a colour setting, a HUD widget drawing the tick count in that colour, a key binding (default `KeyH`) that prints a local chat line, the command `/hello <name>`, and `shutdown`. |
| `packet_logger` | The packet API: `observe` on `PacketReceived` and `PacketSend`, per-id counts shared between threads, a once-per-second log line per packet id, a HUD widget with packets/s in and out, and `/swing`, which injects an `Animation` (0x0A) packet through `Packets::send`. Declares `network_packets = true`, the permission every packet event and injection needs; the client shows it as an "alters packets/movement" badge and turns the mod off in server-compat mode. |
| `physics_tweak` | The API 0.5 services: `wrap_physics` around the player's movement step (the "Fall speed" slider scales a falling player's downward velocity; at 1.0 it only calls `next` and the step stays bit for bit vanilla), and the world queries `World::block` and `World::entities`, run from a `PostTick` intercept (game thread) to show the block under you and the nearest player on a HUD widget. Declares `movement = true` and `wraps = ["player_physics"]`; the client shows the "alters packets/movement" badge and server-compatible mode holds it off. |
| `shader_pass` | The post-process API 0.6: `InitCx::post_pass` registers a WGSL fragment shader (`src/sepia.wgsl`, validated by the host with naga) that turns the picture of the world grey or sepia; a "Strength" slider and a "Tone" choice are pushed into `umi.params[0]` from a `PreTick` observer through `PostPass::set_param`. No permission is needed. Only the world picture is transformed: the HUD and menus stay sharp. Strength 0 leaves the picture unchanged. |
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
