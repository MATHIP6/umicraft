# W0 — lead, integration and acceptance

## Role

Own the M4 contract and the integration. W0 is not a feature agent; it keeps
`main` green and merges the workstreams in order.

## Owned files

- root `Cargo.toml`, `Cargo.lock`
- `.github/workflows/**`
- `docs/**`, `README.md`, `UMICRAFT_SPEC.md`
- `crates/umicraft-ui/Cargo.toml`

## Duties

1. Keep `main` at the setup commit until W1/W2/W4 report.
2. Review each branch against the ownership map in ADR-0006 Appendix A before
   merging. A diff outside the map is sent back.
3. Merge in order **W1 → W2 → W3 → W4 → W5**, running at least
   `cargo check --workspace` after each merge.
4. Resolve only shared files. Never rewrite a feature agent's module.
5. Run the final gates:
   `cargo fmt --all --check`,
   `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`,
   `cargo deny check licenses bans`.
6. Run the acceptance evidence: the headless `--ui-demo` report, the offscreen
   screenshot with the debug screen, and the windowed F2 smoke test.
7. Update `README.md` (M4 status and crate table), `docs/threading.md` (the UI
   boundary) and `docs/sources.md` (the font metrics citation) before closing
   M4.
8. Only after M4 is accepted, create the M5 worktrees.

## Acceptance checklist

- [ ] The debug screen shows a button, an editable text field with a visible
      caret, and a scrollable list.
- [ ] The same screen runs in the windowed and the headless path.
- [ ] Layout/focus/measure tests pass.
- [ ] Real 1.8 font is used when the user's client jar is present; the
      original 3×5 fallback works without it.
- [ ] No Mojang asset is tracked; the `assets-guard` job is green.
- [ ] `cargo deny` is green.
- [ ] ADR-0006, README and threading docs reflect the shipped behavior.
