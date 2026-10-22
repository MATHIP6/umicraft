# M4 workstream cards

These cards are the launch prompts for the M4 workstreams. The contract they
compile against is `docs/decisions/0006-m4-ui-foundation.md`.

| Card | Workstream | Branch / worktree | Depends on |
|---|---|---|---|
| `00-lead.md` | W0 — contract, integration, acceptance | `main` | — |
| `01-ui-core.md` | W1 — UI core | `feat/m4-ui-core` | contract |
| `02-text-font.md` | W2 — text & font | `feat/m4-text-font` | contract |
| `03-widgets-debug-screen.md` | W3 — widgets | `feat/m4-widgets` | W1, W2 |
| `04-render-ui.md` | W4 — renderer UI pass | `feat/m4-render-ui` | contract |
| `05-client-acceptance.md` | W5 — client & acceptance | `feat/m4-client-ui` | W1–W4 |

Wave plan:

1. W0 freezes the contract on `main`.
2. Wave 1: W1, W2 and W4 run in parallel (maximum three agents).
3. Wave 2: W3 starts once W1 and W2 are merged; W5 starts its input plumbing.
4. Wave 3: W5 finishes the headless/screenshot acceptance; W0 integrates.

Each agent must:

- work only in its worktree and branch;
- run targeted `cargo fmt`, `cargo clippy` and `cargo test` commands;
- stop and report if the contract must change;
- never commit a Mojang asset, never add an unapproved dependency, never use
  `unwrap()`/`expect()` on external data and never use `unsafe`;
- report the commit SHA, the files changed, the commands run and any open
  question.
