# W2 — text and font

## Role

Implement the text measurement and the font atlas pipeline: the original 3×5
fallback and the real 1.8 font loaded from the user's client jar.

## Workspace

- Branch: `feat/m4-text-font`
- Worktree: `../.worktrees/m4-text-font`
- Base: the M4 setup commit on `main`

## Read first

- `UMICRAFT_SPEC.md` §3 (especially rules 3 and 4), §12 (M4), §14
- `docs/decisions/0006-m4-ui-foundation.md`
- `docs/sources.md`
- `docs/decisions/0001-reference-source-policy.md`

## Owned files

- `crates/umicraft-ui/src/text.rs` (or `crates/umicraft-ui/src/text/**`)
- `crates/umicraft-ui/tests/text.rs`

## Do not touch

- any other file, including `crates/umicraft-ui/Cargo.toml`
- the renderer or the client
- `docs/**` (report the citations to the lead; W0 writes the docs)

## Deliverables

- `FontAtlas::fallback()`: the original 3×5 bitmap font, moved or recreated
  under a free license, with the same visual data as the current overlay font.
- `FontAtlas::from_png(bytes)`: decode `ascii.png` into an R8 coverage atlas
  and glyph records. Return a typed `FontError` (thiserror) instead of
  panicking on malformed input.
- A glyph table with advances and line height. For the 1.8 font, derive the
  widths from the reference or from the atlas and cite every non-obvious
  value in your report for `docs/sources.md`.
- `measure`, `advance`, `line_height` and the caret-index helpers used by W3:
  return/accept `char` indices, never byte offsets.
- Tests with a synthetic PNG (generated in the test): valid decode, bad
  signature, truncated data, missing glyph, multi-line measure, caret at the
  boundaries and in the middle of a multi-byte character.

## Legal rules

- Never commit `ascii.png` or any extracted Mojang asset.
- A test that reads a real client jar must be `#[ignore]` or gated on an
  environment variable, and must be documented in the test file.
- The fallback is original work; no copied Mojang glyph data.

## Commands

```sh
cargo fmt --all --check
cargo clippy -p umicraft-ui --all-targets -- -D warnings
cargo test -p umicraft-ui
RUSTDOCFLAGS="-D warnings" cargo doc -p umicraft-ui --no-deps
```

## Definition of done

- Targeted gates green.
- Every public item documented; no `unsafe`; no `unwrap()` on decoded data.
- Report: SHA, files, commands and results, the source citations for the
  metrics, deviations, open questions.
