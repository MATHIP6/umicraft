# W4 — renderer UI pass

## Role

Turn the `umicraft-ui::DrawList` into overlay vertices and upload the font
atlas, without breaking the existing HUD overlay or the screenshot path.

## Workspace

- Branch: `feat/m4-render-ui`
- Worktree: `../.worktrees/m4-render-ui`
- Base: the M4 setup commit on `main`
- Depends on: the frozen `DrawList`/`FontAtlas` types only; start immediately
  with synthetic data.

## Read first

- `UMICRAFT_SPEC.md` §3, §12 (M4), §14
- `docs/decisions/0006-m4-ui-foundation.md`
- `crates/umicraft-render/src/overlay.rs`, `renderer.rs`, `font.rs`
- `docs/threading.md`

## Owned files

- `crates/umicraft-render/src/overlay.rs`
- `crates/umicraft-render/src/renderer.rs`
- `crates/umicraft-render/src/lib.rs`
- new `crates/umicraft-render/src/ui_pass.rs`
- `crates/umicraft-render/Cargo.toml`
- `crates/umicraft-render/tests/**`

## Do not touch

- `crates/umicraft-ui/**`, `crates/umicraft-client/**`, root manifests,
  `Cargo.lock`, docs, CI
- the terrain pipeline or the HUD layout

## Deliverables

- A UI pass that consumes a `DrawList` and emits overlay vertices in command
  order.
- Clip scopes: `PushClip`/`PopClip` must clip the following text and rects,
  including nested scopes. Use a scissor rect or a CPU-side clip intersection;
  document which one and why.
- Font atlas upload: an R8 texture created from `FontAtlas::pixels`, with the
  atlas UVs and the white texel used for rectangles.
- The existing `Overlay::text` HUD API keeps working, backed by the fallback
  atlas, so M3 screenshots and the debug overlay do not regress.
- No new thread and no blocking call. The draw list is built before the GPU
  submission; the renderer only uploads and draws.
- Add `umicraft-ui.workspace = true` to the render manifest.

## Tests

- Vertex generation for rect/text/clip in command order.
- Nested clip intersection (including an empty intersection).
- UV mapping of a synthetic atlas and of the white texel.
- The existing renderer smoke test still passes (opt-in, needs an adapter).
- The screenshot path still produces terrain and the debug overlay.

## Commands

```sh
cargo fmt --all --check
cargo clippy -p umicraft-render --all-targets -- -D warnings
cargo test -p umicraft-render
RUSTDOCFLAGS="-D warnings" cargo doc -p umicraft-render --no-deps
```

## Definition of done

- Targeted gates green.
- Every public item documented; no `unsafe`; no `unwrap()` on external data.
- Report: SHA, files, commands and results, deviations, open questions.
