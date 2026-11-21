# 0088. OptiFine as a behaviour reference

Status: accepted (2026-11-21, project owner)

## Context

Milestone F6 (`docs/tasks/optifine/README.md`) brings OptiFine-style video
settings, optimisations and resource-pack features (custom sky, custom entity
models, connected textures, ...) to Umicraft. A decompiled OptiFine for 1.8.9
sits outside the repository, in `../optifine/`. OptiFine is proprietary, so
ADR-0001, which covers only the MCP sources, does not apply to it.

The owner reviewed the trade-off on 2026-11-21 and allowed reading it. The
owner's reasoning was that most third-party clients are built on the same
knowledge.

## Decision

- `../optifine/` may be read as a reference for **behaviour and file
  formats**, on the same terms as ADR-0001: it is never committed, no code is
  copied, and everything is written in original Rust.
- For resource-pack formats, the **public OptiFine documentation**
  (OptiFineDoc) comes first. The decompiled tree resolves ambiguities, and
  covers what the documentation leaves out.
- The local tree is partial. It has no CEM loader, no per-entity part-name
  tables, no CEM entity variables, no Smart Animations and no Render Regions.
  For those features the public documentation and black-box testing with real
  packs are the only sources.
- Every fact taken from either source is recorded in `docs/sources.md`
  (section "OptiFine features"), with the class or document it came from.
- No OptiFine asset (textures, the default CTM tiles, language files) enters
  the repository. Tests use original packs made for them.

## Consequences

- Faster convergence on the exact formats that resource packs rely on.
- The publication risk of ADR-0001 now covers this knowledge as well. The
  legal review before any release must include it.
- The name "OptiFine" may appear in docs and settings tooltips only as a
  description ("OptiFine-compatible custom sky"), never as product branding
  (spec §3 rule 5).
