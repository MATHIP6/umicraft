# Capes built-in mod: plan

Status: **phases 1 to 3 done** (branch `feat/capes`); the visual check in game and the MinecraftCapes answer of a cape owner remain. Design: ADR-0081.

## File ownership

| Phase | Owner | Files |
|---|---|---|
| 1 | lead | `docs/decisions/0081-capes.md`, this file, `docs/sources.md` |
| 2a | worker A | `crates/umicraft-assets/src/capes/**`, `crates/umicraft-assets/src/lib.rs` (one `pub mod capes;` line), `crates/umicraft-assets/Cargo.toml` |
| 2b | worker B | `crates/umicraft-builtin-mods/src/mods/capes.rs`, `crates/umicraft-builtin-mods/src/mods/mod.rs`, `crates/umicraft-assets/client-pack/umicraft/lang/{en_US,fr_FR}.lang`, the mod icon (`crates/umicraft-ui/assets/icons*`, `crates/umicraft-ui/src/icons.rs`, `crates/umicraft-ui/src/menus/mods/card.rs`, `docs/tools/gen_icons.py`) |
| 3 | lead | `crates/umicraft-model/src/entity/mod.rs` (`TextureSlot::CustomCape`), `crates/umicraft-client/src/mods_runtime.rs`, `crates/umicraft-client/src/entity_render/{capes,mobs,textures,skins}.rs`, wiring of the render loop, docs |

## Phases

1. ADR and plan (done).
2. In parallel:
   - 2a: providers, responses, image decoding (HD padding, frames,
     LabyMod placeholder), allowlist, `CapeResolver` worker. Unit tests for
     every pure function; the resolver tested against a local HTTP server.
   - 2b: the mod, its settings, its texts in both languages, its icon.
3. Client: `CapeSettings`, `CapeSet`, the cape layer, reset on setting
   change, visual check in third person (F5) with an OptiFine cape.

## Acceptance

- A player with an OptiFine cape shows it; an animated MinecraftCapes or
  Cosmetica cape cycles every 100 ms; an HD cape is sharp.
- Mod off: no request leaves the client, Mojang capes as before.
- Offline-mode players and NPCs are never looked up.
- `cargo fmt`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo test --workspace` pass.
