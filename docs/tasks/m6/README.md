# M6 workstream cards and launch guide

M6 is the lighting, sky/fog and transparency milestone (spec §12.2, ADR-0008).
This directory holds the launch cards for the four workstreams. The contract
they compile against is
[`docs/decisions/0008-m6-lighting-sky-transparency.md`](../../decisions/0008-m6-lighting-sky-transparency.md).

The setup commit on `main` already contains the frozen shared types:
`LightLevel`, world light getters/setters, `VertexLight`, `Vertex.light`, the
`Mesh` transparent buffers, `RenderEnvironment`, `SkyState` and the
`SkyRenderer` hooks. Bodies are placeholders; the cards below fill them in.

## Workstreams

| Card | Workstream | Branch / worktree (under project root) | Depends on |
|---|---|---|---|
| `00-integration.md` | W0 — integration and acceptance lead | `main` | — |
| `01-lighting.md` | W1 — light sampling, smooth lighting and AO | `feat/m6-light` / `.worktrees/m6-light` | ADR-0008 |
| `02-terrain-renderer.md` | W2 — terrain lighting, transparency pass and fog | `feat/m6-render` / `.worktrees/m6-render` | ADR-0008 |
| `03-sky-time.md` | W3 — sky pass, sun/moon/clouds and world clock | `feat/m6-sky` / `.worktrees/m6-sky` | ADR-0008 |
| `04-acceptance.md` | W4 — synthetic lighting/transparency acceptance and perf | `feat/m6-acceptance` / `.worktrees/m6-acceptance` | ADR-0008; final run after W1–W3 |

Absolute worktree root: `/home/mathip/dev/rust/mc-project/.worktrees`.
The git repository itself is `/home/mathip/dev/rust/mc-project/umicraft`.

## Wave plan

1. **Wave 1 — W1, W2, W3 and the W4 harness in parallel.** The four worktrees
   edit disjoint files (ADR-0008 ownership table). W4 can build the synthetic
   day/night/cave scene and its report against the frozen types; it must not
   pretend the final screenshots are valid until W1–W3 are merged.
2. **Wave 2 — W4 final acceptance after W1/W2/W3 are merged.** The lead merges
   in the order W1 → W2 → W3, then W4 pulls `main`, runs the screenshots,
   records the Criterion numbers and writes the acceptance section.
3. **Lead closure.** Run the workspace gates, all opt-in GPU tests and the
   no-asset audit. Update ADR-0008 status, `README.md`, `docs/threading.md`
   and `docs/tasks/m6/README.md`, then close M6.

At most four agents may run at once. Do not start a fifth worktree or agent
without the lead splitting a new ADR.

## Rules for every agent

- Work only in the assigned worktree and branch. Never merge to `main`.
- Start by making sure the branch is current:
  `git merge --ff-only main` when it has no commits of its own, or
  `git merge main` when it already has commits. Resolve only your own files;
  if a shared file conflicts, stop and report it to the lead.
- Keep the crate-local `target/` of the worktree. Concurrent worktrees MUST
  NOT share `CARGO_TARGET_DIR` (spec §14.12); never export it globally.
- Run the crate-local gates from the card before handoff:
  `cargo fmt`, `cargo clippy -p <crate> --all-targets -- -D warnings`,
  `cargo test -p <crate>`, and
  `RUSTDOCFLAGS="-D warnings" cargo doc -p <crate> --no-deps`.
- Do not commit a Mojang asset. The client jar, models, blockstates and PNGs are
  read at runtime from the user's installation; tests use synthetic fixtures
  only. `mcp/` remains gitignored (ADR-0001).
- Every public item has a `///` doc comment. No `unwrap()`/`expect()` on
  external data and no `unsafe`.
- Do not edit shared files (`Cargo.toml`, `Cargo.lock`, CI, root `README.md`,
  `docs/decisions/**`, `docs/tasks/**`) unless the card explicitly assigns
  them. Report the needed change to the lead.
- Record non-obvious protocol/lighting behaviour in the code comment and tell
  the lead which `docs/sources.md` row to add.
- Report the commit SHA, the files changed, the exact commands and results,
  any deviation from ADR-0008 and any open question. Do not merge.

## Current handoff state

M6 is accepted on `main`, merged in the order W1 → W2 → W3 → W4:

| Workstream | State |
|---|---|
| Setup / W0 | complete: ADR-0008 and frozen interfaces on `main` |
| W1 lighting | accepted: `feat/m6-light` at `9536327`, merged in `bc9fe43` |
| W2 terrain renderer | accepted: `feat/m6-render` at `1837fa5`, merged in `807725d` |
| W3 sky & clock | accepted: `feat/m6-sky` at `841b7ba`, merged in `b3182ae` |
| W4 acceptance | accepted: `feat/m6-acceptance` at `d7faaea`, merged in `831c3b8` |

M5 was accepted at `a1bfe05`. The M6 acceptance evidence is the four
`--demo-view` screenshots (1280×720, llvmpipe), a measured AO gradient of
15/10/5/0 at the demo corner, day/night/cave mean luminances of
186.0/51.6/40.5, and the meshing proxy at 40.6–42.2 ms against the 125.7 ms
budget (rows in `docs/perf-log.md`). The worktrees were removed after the
merge; the four branches are retained.

## Integration gates

The lead runs after every merge:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check licenses bans
```

Opt-in GPU gates:

```sh
cargo test -p umicraft-render renderer::tests::offscreen_smoke_renders_sky_and_overlay -- --ignored
cargo test -p umicraft-render --test offscreen_ui -- --ignored
cargo test -p umicraft-render --test offscreen_terrain -- --ignored
cargo test -p umicraft-client ui_demo::tests::the_ui_demo_screenshot_writes_a_png -- --ignored
cargo test -p umicraft-client block_demo -- --ignored
cargo test -p umicraft-render block_atlas -- --ignored
```

Acceptance commands are in `00-integration.md` and `04-acceptance.md`.

## Legal / repository audit

```sh
git ls-files | grep -Ei '\.(jar|class|png|ogg|wav|mcmeta)$' || true
git status --short
```

No Mojang asset may be tracked. `mcp/` stays ignored.
