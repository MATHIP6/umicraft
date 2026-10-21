# ADR-0001 — Reference policy for Minecraft 1.8 behavior details

- Status: Accepted
- Date: 2026-10-20
- Deciders: project owner (explicit session confirmation)

## Context

The spec originally required a clean-room method (§3 rule 1): no decompiled or
deobfuscated Mojang/Microsoft source may be consulted while implementing Umicraft.
The workspace contains a local MCP (Mod Coder Pack) 1.8 tree under `mcp/` with
deobfuscated class files, SRG mappings, decompiler outputs, and vanilla jars.

The owner reviewed the trade-off and chose to allow consulting these local sources
during development, accepting the associated EULA/DMCA exposure for this working copy.

## Options

1. Keep clean-room only (protocol docs, captures, black-box testing) — slower, safest to publish.
2. Allow `mcp/` as a local implementation reference, keep it out of git, record provenance per fact. *(chosen)*
3. Hybrid: docs first, MCP only to resolve ambiguities — a subset of option 2, used opportunistically.

## Decision

The local `mcp/` tree may be consulted as an implementation reference, with rules:

- `mcp/` is never committed (gitignored) — no version history or release artifact carries it.
- Facts learned from it are recorded in `docs/sources.md` with the consulted path tag.
- All remaining §3 constraints stay binding: no Mojang assets in the repo or releases,
  no asset mirroring, Umicraft branding only, license compatibility, EULA review before release.

## Consequences

- Faster convergence on protocol-47, 1.8 physics, and UI behavior details.
- Before any publication of the repo or binaries, the owner must re-review §3, the
  EULA/usage guidelines, and decide if/how the work may be released; publication of
  decompiled-code-*informed* code carries takedown/legal risk the owner accepted.
- No doc or test may claim clean-room provenance for knowledge sourced from `mcp/`.
