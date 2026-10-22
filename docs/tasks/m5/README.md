# M5 workstream cards and launch guide

M5 is the 1.8 block-model and texture-atlas milestone (spec §12.2, ADR-0007).
This directory holds the launch cards for the workstreams. The contract they
compile against is
[`docs/decisions/0007-m5-block-models.md`](../../decisions/0007-m5-block-models.md).

## Workstreams

| Card | Workstream | Branch / worktree | Depends on |
|---|---|---|---|
| `00-integration.md` | W0 — contract, integration, acceptance | `main` | — |
| `01-model-core.md` | W1 — blockstate/model parser and face baker | `feat/m5-model-core` / `.worktrees/m5-model-core` | ADR-0007 |
| `02-registry.md` | W2 — 1.8 block-id registry | `feat/m5-registry` / `.worktrees/m5-registry` | ADR-0007 |
| `03-atlas.md` | W3 — client resources and texture atlas | `feat/m5-atlas` / `.worktrees/m5-atlas` | ADR-0007 |
| `04-mesher.md` | W4 — model mesher | `feat/m5-mesher` / `.worktrees/m5-mesher` | W1 + W2 merged |
| `05-renderer.md` | W5 — renderer texture array | `feat/m5-renderer` / `.worktrees/m5-renderer` | W3 API (or the frozen contract) |
| `06-client-acceptance.md` | W6 — client integration and acceptance | `feat/m5-client` / `.worktrees/m5-client` | W1–W5 merged |

## Wave plan

1. **Wave 1 — W1, W2, W3 in parallel.** The parser, the registry and the atlas
   are independent: W1 owns the model parser/geometry, W2 owns `registry.rs`,
   W3 owns the asset resources and atlas. Three agents maximum.
2. **Wave 2 — W4, W5.** W4 needs W1 and W2 merged; W5 only needs the frozen
   `set_block_atlas` signature and can start against synthetic data.
3. **Wave 3 — W6.** The client integration and `--block-demo` run after W1–W5
   are merged. The lead then runs the full acceptance gates.

Do not start a wave before the previous one has handed off. If an agent is
blocked, it reports the blocking API instead of editing another workstream's
files.

## Rules for every agent

- Work only in the assigned worktree and branch. Never merge to `main`.
- Start by bringing the worktree up to date with the current `main`:
  `git merge --ff-only main` when the branch has no commits of its own, or
  `git merge main` when it already has commits. Resolve only your own files;
  stop and report a conflict in a shared file.
- Keep the crate-local `target/` of the worktree. Concurrent worktrees MUST
  NOT share `CARGO_TARGET_DIR` (spec §14.12); do not export it globally.
- Run the crate-local gates from the card before handing off:
  `cargo fmt`, `cargo clippy -p <crate> --all-targets -- -D warnings`,
  `cargo test -p <crate>`, and
  `RUSTDOCFLAGS="-D warnings" cargo doc -p <crate> --no-deps`.
- Do not commit a Mojang asset. The client jar, blockstates, models and PNGs
  are read at runtime from the user's official installation; tests use
  synthetic JSON/PNG fixtures only. `mcp/` remains gitignored (ADR-0001).
- Every public item has a `///` doc comment. No `unwrap()`/`expect()` on
  external data and no `unsafe`.
- Do not edit shared files (`Cargo.toml`, `Cargo.lock`, CI, `docs/**`, root
  `README.md`) unless the card explicitly assigns them. Report the needed
  change to the lead.
- Report the commit SHA, the files changed, the exact commands and results,
  any deviation from ADR-0007 and any open question. Do not merge.

## Current handoff state

At the time this guide was written (2026-10-22):

| Workstream | State |
|---|---|
| Setup / W0 | `b3b2a15` scaffold; `main` has the later docs-only M5 commits |
| W1 model core | committed as `6b8cfee`, synced with `main`, crate gates green |
| W2 registry | at the setup base, synced to the current `main` docs |
| W3 atlas | two commits on `feat/m5-atlas` (`925b196`, `883390e`), synced with `main` |
| W4 mesher | at the setup base, synced to the current `main` docs |
| W5 renderer | at the setup base, synced to the current `main` docs |
| W6 client | at the setup base, synced to the current `main` docs |

The lead updates this table at each handoff. A worktree with uncommitted
changes belongs to its active agent; the lead does not reset or clean it.
W1 is ready for W4 once the lead merges it.

## Integration gates

The lead merges in the order **W1 → W2 → W3 → W4 → W5 → W6**, then runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check licenses bans
```

The acceptance commands are in `00-integration.md`; no Mojang asset may be
tracked after integration (`git ls-files` audit and the `assets-guard` job).
