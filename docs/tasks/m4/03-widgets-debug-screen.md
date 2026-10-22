# W3 — widgets and the acceptance screen

## Role

Implement the button, the single-line text field, the scrollable list, the
panel and the pure debug screen that composes them.

## Workspace

- Branch: `feat/m4-widgets`
- Worktree: `../.worktrees/m4-widgets`
- Base: `main` after W1 and W2 are merged
- Depends on: W1 (`Ui`, focus, layout) and W2 (`FontAtlas`, metrics)

## Read first

- `UMICRAFT_SPEC.md` §3, §12 (M4), §14
- `docs/decisions/0006-m4-ui-foundation.md` (Appendix B)
- the W1 and W2 merged code

## Owned files

- `crates/umicraft-ui/src/widgets/**`
- `crates/umicraft-ui/src/debug_screen.rs`
- `crates/umicraft-ui/tests/widgets.rs`

## Do not touch

- any other file, including `lib.rs` (the module declarations already exist)
- the renderer, the client, manifests, docs

## Deliverables

- `panel`: background rectangle plus an explicit inset content area.
- `button`: hover/press/click response, centred label, disabled state if the
  response model supports it.
- `text_field` with a `TextFieldState`:
  - insertion of `InputEvent::Text`, including several characters;
  - Backspace/Delete, Home/End, left/right arrows;
  - caret index always on a `char` boundary and in `0..=len`;
  - visible caret when focused;
  - horizontal scroll or clipping so long text does not overflow the field;
  - `UiResponse::changed` when the text or the caret changes.
- `list` with a `ListState`:
  - wheel and arrow-key scrolling;
  - clipping of rows to the list viewport;
  - selection and `changed` response;
  - clamps the scroll offset after a resize or an item-count change.
- `debug_screen`: a pure screen with a title, a button, a text field and a
  list. It exposes a `show(&mut Ui)` method and keeps its state internally.
  It must not read files, time, network or global input.

## Tests

- Button: click, hover, click outside, overlap ordering.
- Text field: type, Backspace, Delete, Home/End, arrows, caret at Unicode
  boundaries, focus change, long text clipping.
- List: wheel, keyboard, selection, scroll clamp, clipping commands.
- Debug screen: a scripted sequence produces the expected responses and a
  non-empty draw list with a `PushClip`/`PopClip` pair around the list.

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
- Report: SHA, files, commands and results, deviations, open questions.
