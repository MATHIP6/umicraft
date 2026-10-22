# M6-W0 — integration and acceptance lead

Owner: `main` (the lead), not an implementation worktree
Contract: `docs/decisions/0008-m6-lighting-sky-transparency.md`
Worktrees: `/home/mathip/dev/rust/mc-project/.worktrees/m6-*`

## Goal

Keep the four M6 workstreams disjoint, merge them in dependency order, run the
workspace and opt-in gates, and close M6 only when the acceptance evidence and
the performance proxy are recorded. Never merge a workstream before its card's
handoff is complete.

## Merge order

1. `feat/m6-light` (W1) — world light access, smooth light/AO, transparent mesh
   split.
2. `feat/m6-render` (W2) — terrain shader light/AO, transparent pass/sort, fog.
3. `feat/m6-sky` (W3) — sky/sun/moon/clouds and the world clock.
4. `feat/m6-acceptance` (W4) — demo scene, screenshots and perf rows. W4 must
   merge `main` (now containing W1–W3) before its final run; otherwise it cannot
   see the real transparent routing, sky pass or clock.

W1 and W2 are file-disjoint and may be merged back-to-back. W3 is also
file-disjoint but should follow W2 so the sky pipeline and terrain pass are
exercised together. Do not merge W4's harness commit before W1–W3 unless it is
only the independent report/args half; the final acceptance commit goes last.

## Setup state (2026-10-22)

The setup commit on `main` contains:

- `LightLevel`, world light getters/setters and the full-bright
  `BlockSource::light` default;
- `VertexLight`, `Vertex::light`, the packed bit layout and the `Mesh`
  transparent buffers/counts;
- `RenderEnvironment`, the public `SkyState` data and the crate-private
  `SkyRenderer` hooks with no-op bodies;
- the fifth terrain vertex attribute and WGSL input location 4;
- ADR-0008, this card set and the W1–W4 cards.

The worktrees are created from that commit. Bodies are placeholders; no M6
feature is implemented yet.

## Per-merge gates

Run from `main` after every merge:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check licenses bans
```

If a workstream touched the renderer, atlas or shadow path, also run the opt-in
GPU suite:

```sh
cargo test -p umicraft-render -- --ignored
cargo test -p umicraft-client -- --ignored
```

Do not merge a branch whose own card gates fail. A failing workspace gate on a
file the branch does not own is a lead problem: fix or revert on `main`, then
ask the owner to rebase, rather than editing outside ownership.

## Conflicts and interface changes

- Any change to the ADR-0008 names, layouts, ordering or file ownership needs a
  short ADR amendment before code is merged. Do not silently reinterpret the
  contract.
- Expected shared files are `Cargo.toml`/`Cargo.lock` (none of the cards should
  edit them), `docs/perf-log.md` (W4 only) and the task README (lead only).
  Treat a conflict there as a process signal, not a mechanical merge.
- If W2 needs a `SkyState` field W3 did not provide, stop and amend the ADR;
  do not let W2 edit `sky.rs`.
- If W3 needs a renderer hook that was not scaffolded, the lead changes
  `renderer.rs` with W2's handoff reviewed, or asks W2 to add the minimal hook
  before merging W3.
- Worktrees must not share `CARGO_TARGET_DIR`. Concurrent cargo processes on
  one target directory produce misleading gates and lock contention.

## Final acceptance commands

Build once, then run the four demo shots:

```sh
cargo build -p umicraft-client
target/debug/umicraft-client --block-demo --demo-view day \
  --time 6000 --screenshot /tmp/m6-day.png
target/debug/umicraft-client --block-demo --demo-view night \
  --time 18000 --screenshot /tmp/m6-night.png
target/debug/umicraft-client --block-demo --demo-view cave \
  --time 6000 --screenshot /tmp/m6-cave.png
target/debug/umicraft-client --block-demo --demo-view transparency \
  --time 6000 --screenshot /tmp/m6-transparency.png
```

The official 1.8.8 installation is `/home/mathip/.minecraft`; the demo must use
it at runtime and must not copy any asset into git. Inspect the PNGs manually
and compare with W4's PASS/FAIL notes.

Performance proxy:

```sh
cargo bench -p umicraft-mesh --bench mesh -- --quick
```

Acceptance is `mesh/flat_view_169_chunks ≤ 125.7 ms` (110 % of the M2 114.3 ms
baseline) and no unexplained regression in the single-chunk/dense rows. W4
appends the raw times and percentages to `docs/perf-log.md`. If the proxy
fails, profile before declaring M6 accepted; record the hot spot and the
follow-up.

Legal / repository audit:

```sh
git ls-files | grep -Ei '\.(jar|class|png|ogg|wav|mcmeta)$' || true
git status --short
git log --oneline main ^origin/main | head
```

No Mojang asset may be tracked. `mcp/` is intentionally ignored (ADR-0001).
`main` is many commits ahead of `origin/main`; pushing and branch retention are
the owner's decisions, not part of this card.

## Closure checklist

- [ ] W1–W4 merged with green per-merge gates.
- [ ] Four M6 screenshots inspected: day, night, cave, transparency.
- [ ] AO visible at the demo corner; night and cave are dark; water/glass/leaves
      are blended, not opaque or invisible.
- [ ] `docs/perf-log.md` has the M6 before/after rows and the 110 % result.
- [ ] ADR-0008 status changed to `Implemented` (or an amendment recorded for
      any deviation).
- [ ] Root `README.md` status and M6 summary updated.
- [ ] `docs/threading.md` documents the clock/environment channel and the
      transparent pass ordering.
- [ ] `docs/sources.md` has the M6 rows (smooth light/AO, lightmap, time/sky/
      fog/clouds, transparency sorting, `Time Update`).
- [ ] `docs/tasks/m6/README.md` handoff table marked accepted.
- [ ] `cargo fmt`, clippy, tests, doc, deny, opt-in GPU tests and no-asset audit
      all green.
- [ ] Worktrees left in place; do not delete or push without the owner's
      explicit decision.

## Handoff template

Report to the owner:

```
M6 state: accepted | blocked
Commits: <main SHA>, <W1 SHA>, <W2 SHA>, <W3 SHA>, <W4 SHA>
Gates: fmt/clippy/test/doc/deny ...
Perf: flat_view_169_chunks <ms> (<pct> of M2), single <µs>, dense <µs>
Screenshots: /tmp/m6-day.png ... (dimensions, PASS/FAIL)
Assets: no tracked jar/class/png/mcmeta; mcp ignored
Open decisions: push main? retain branches/worktrees?
```
