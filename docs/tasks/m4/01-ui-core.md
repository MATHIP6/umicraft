# W1 — UI core

## Role

Implement the immediate-mode core, the layout maths, the focus and the hit
testing of `umicraft-ui`.

## Workspace

- Branch: `feat/m4-ui-core`
- Worktree: `../.worktrees/m4-ui-core` (from `umicraft/`)
- Base: the M4 setup commit on `main` (the lead gives the exact SHA)

## Read first

- `UMICRAFT_SPEC.md` §3, §12 (M4), §14
- `docs/decisions/0006-m4-ui-foundation.md` (Appendix A and B)
- `docs/threading.md`

## Owned files

- `crates/umicraft-ui/src/lib.rs`
- `crates/umicraft-ui/src/geom.rs`
- `crates/umicraft-ui/src/id.rs`
- `crates/umicraft-ui/src/input.rs`
- `crates/umicraft-ui/src/draw.rs`
- `crates/umicraft-ui/src/context.rs`
- `crates/umicraft-ui/tests/**` except `text.rs` and `widgets.rs`

## Do not touch

- `crates/umicraft-ui/Cargo.toml`, root manifests, `Cargo.lock`, CI, `docs/**`
- `crates/umicraft-ui/src/text.rs`, `widgets/**`, `debug_screen.rs`
- any other crate

## Deliverables

- Layout helpers for horizontal/vertical stacks, padding and a scroll
  viewport, all unit-tested against known pixel values.
- Focus model: Tab/Shift+Tab traversal, click-to-focus, stable widget IDs,
  focus surviving across frames.
- Hit testing that respects overlapping panels and the first-click-wins rule.
- `Ui::begin_frame`/`Ui::end_frame` reuse their allocations and leave the
  draw list and responses in a consistent state.
- Keep the existing public signatures from ADR-0006 Appendix B; refine types
  only if no other workstream has to change.

## Tests

- Layout: rows, columns, padding, empty children, insets, scroll clamping.
- Focus: forward/backward traversal, click focus, click on empty space.
- Hit testing: overlap, outside viewport, pointer outside a clip.
- ID: stability across frames, distinct scopes.
- `DrawList`: command order, clip balance, clear/reuse.

## Commands

```sh
cargo fmt --all --check
cargo clippy -p umicraft-ui --all-targets -- -D warnings
cargo test -p umicraft-ui
RUSTDOCFLAGS="-D warnings" cargo doc -p umicraft-ui --no-deps
```

## Definition of done

- Targeted gates green.
- Every public item documented; no `unsafe`; no `unwrap()` on external data.
- Commit(s) with `feat(ui):`/`test(ui):` messages.
- Final report: SHA, files, commands and results, deviations, open questions.
