# M5-W5 — renderer texture array

Owner worktree: `.worktrees/m5-renderer`
Branch: `feat/m5-renderer`
Depends on: W3 atlas API merged into `main` (the renderer only needs the frozen
`set_block_atlas` contract)

## Goal

Upload the real block/item texture array and replace the terrain binding, while
keeping the procedural fallback and all M4 UI behaviour.

## Owned files

- `crates/umicraft-render/**`

Do not edit mesh/assets/client/model.

## Required API

```rust
pub struct BlockAtlasError(/* typed */);
impl Renderer {
    pub fn set_block_atlas(
        &mut self,
        width: u32,
        height: u32,
        layers: u32,
        pixels: &[u8],
    ) -> Result<(), BlockAtlasError>;
}
```

Export `BlockAtlasError` from the crate root.

## Behaviour

1. Validate: non-zero width/height/layers; `pixels.len()` exactly
   `width*height*4*layers`; dimensions within
   `device.limits().max_texture_dimension_2d`; layers within
   `max_texture_array_layers`. Return typed errors, never panic.
2. Create a `Rgba8UnormSrgb` `D2Array` texture with the real layer count, write
   all layers once, create a `D2Array` view, and replace the terrain bind group.
   Store the sampler and bind-group layout needed to rebuild it.
3. On error the existing fallback 19-layer array and bind group stay usable.
4. Keep `fallback_texture_data`, `TEXTURE_LAYERS`, `Renderer::new_windowed`,
   `new_offscreen`, `read_pixels`, UI methods and mesh upload unchanged.
5. Terrain shader: add alpha cutout (`discard` when sampled alpha `< 0.5`).
   Do not add blending/sorting/fog; that is M6.
6. Add a typed `BlockAtlasError` with `#[error(...)]` messages for each
   rejection. No `unwrap`/`expect` on external data.

## Tests

- validation rejects zero layers, short/long buffers, oversized dimensions and
  too many layers with the expected variants;
- a small 2×2/3-layer synthetic atlas uploads successfully and a subsequent
  offscreen render still produces the expected frame size;
- after a rejected upload the fallback array still renders (existing smoke);
- `read_pixels`/UI smoke tests remain green.

The GPU tests stay `#[ignore]` and are run by the lead.

## Gates

```sh
cargo fmt -p umicraft-render -- --check
cargo clippy -p umicraft-render --all-targets -- -D warnings
cargo test -p umicraft-render
cargo test -p umicraft-render --test offscreen_ui -- --ignored
RUSTDOCFLAGS="-D warnings" cargo doc -p umicraft-render --no-deps
```

Report the commit SHA and test count. Do not merge.
