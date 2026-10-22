# W5 — client integration and acceptance

## Role

Wire the UI into the windowed client, the screenshot path and the headless
driver, load the real font when the user has one, and produce the M4
acceptance evidence.

## Workspace

- Branch: `feat/m4-client-ui`
- Worktree: `../.worktrees/m4-client-ui`
- Base: `main` after W3 and W4 are merged (input plumbing may start earlier
  against the W1 contract)
- Depends on: W1–W4

## Read first

- `UMICRAFT_SPEC.md` §3, §11, §12 (M4), §14
- `docs/decisions/0006-m4-ui-foundation.md`
- `docs/threading.md`
- `crates/umicraft-client/src/{window,headless,screenshot,args,input}.rs`
- `crates/umicraft-assets/src/{install,manifest,client_jar}.rs`

## Owned files

- `crates/umicraft-client/**`
- `crates/umicraft-testkit/**`
- `crates/umicraft-client/Cargo.toml`

## Do not touch

- `crates/umicraft-ui/**`, `crates/umicraft-render/**`, root manifests,
  `Cargo.lock`, docs, CI

## Deliverables

- Add `umicraft-ui.workspace = true` and `umicraft-assets.workspace = true` to
  the client manifest.
- Translate winit keyboard, mouse and text events into `UiInput`; do not put
  UI state on the game thread.
- F2 toggles the debug screen in the windowed client. The game keeps running
  while it is open; the UI only reads the render snapshot.
- Font loading: try the user's official installation
  (`OfficialInstall::from_env()`), read `versions/1.8.8/1.8.8.json`, verify the
  client jar descriptor and call `ClientJar::open`; read
  `assets/minecraft/textures/font/ascii.png` and pass the bytes to
  `FontAtlas::from_png`. On any failure, use `FontAtlas::fallback()` and log at
  `warn`; never panic and never commit the asset.
- `--ui-demo`: a headless mode that drives the debug screen through a scripted
  input sequence without a server. It prints a report with the text field
  content, the caret position, the button click count and the list selection.
  It must be usable in CI with no GPU.
- `--ui-demo --screenshot PATH`: render the debug screen offscreen and write
  the PNG. Keep the existing terrain screenshot behavior.
- Add a test in `umicraft-testkit` or the client crate that runs the scripted
  sequence through `umicraft-ui` and asserts the report. Use the pure debug
  screen, not a fake UI.

## Tests

- Input translation: key down/up, text, pointer, wheel, modifiers.
- Scripted headless sequence: focus, type, Backspace, arrow, click, scroll.
- Fallback path: no client jar -> fallback atlas and a warning, no panic.
- Screenshot: `--ui-demo --screenshot` writes a non-empty PNG.
- Full acceptance: windowed manual smoke, headless report, screenshot.

## Commands

```sh
cargo fmt --all --check
cargo clippy -p umicraft-client -p umicraft-testkit --all-targets -- -D warnings
cargo test -p umicraft-client -p umicraft-testkit
cargo run -p umicraft-client -- --ui-demo
cargo run -p umicraft-client -- --ui-demo --screenshot /tmp/m4-ui.png
```

## Definition of done

- Targeted gates green.
- Every public item documented; no `unsafe`; no `unwrap()` on external data.
- The acceptance report and the screenshot path are recorded in the final
  report.
- Report: SHA, files, commands and results, deviations, open questions.
