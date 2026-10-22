# M5 integration and acceptance

Lead-owned. The workstream table, the wave plan and the per-agent rules live in
[`README.md`](README.md). Runs only after W1–W6 are merged in order (or
deviations recorded in ADR-0007).

## Merge order

`W1 model core → W2 registry → W3 atlas → W4 mesher → W5 renderer → W6 client`

Wave 1 is W1/W2/W3, wave 2 is W4/W5, wave 3 is W6. The lead reviews each diff
against its ownership map and runs the stream's gates before merging.

## Lead gates

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check licenses bans
```

Opt-in GPU:

```sh
cargo test -p umicraft-render renderer::tests::offscreen_smoke_renders_sky_and_overlay -- --ignored
cargo test -p umicraft-render --test offscreen_ui -- --ignored
cargo test -p umicraft-client ui_demo::tests::the_ui_demo_screenshot_writes_a_png -- --ignored
cargo test -p umicraft-render block_atlas -- --ignored
```

Acceptance commands:

```sh
cargo run -p umicraft-client -- --ui-demo
cargo run -p umicraft-client -- --ui-demo --screenshot /tmp/m4-regression.png
cargo run -p umicraft-client -- --block-demo
cargo run -p umicraft-client -- --block-demo --screenshot /tmp/m5-blocks.png
```

Then inspect the PNG, verify the report counts, and run a windowed F2/terrain
smoke if the environment permits. Record the limitation if input injection is
still unavailable.

## Legal / repository audit

```sh
git ls-files | grep -Ei '\.(jar|png|ogg|wav|class)$' || true
git status --short
```

No Mojang asset may be tracked. `mcp/` stays ignored.

## Closure

- update ADR-0007 status and implementation notes;
- update README/threading if not already done;
- close M5 only after all gates and the model screenshot pass;
- do not start M6 before M5 acceptance.
