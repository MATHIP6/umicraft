# ADR-0019 — Parity harness

**Status:** accepted for the tooling; the tolerances are **proposed**, pending owner approval
**Date:** 2026-10-26
**Milestone:** P0 (ROADMAP.md)

## Context

ROADMAP.md makes the vanilla 1.8.8 client the specification and accepts every
parity milestone against vanilla on a live server (principle 3). P0 needs a
harness that compares what the vanilla client and Umicraft do for the same
scripted session: serverbound packets per tick, trajectories, screenshots.
ROADMAP §8.1 leaves the tolerances open "to fix in P0".

## Decision

1. **Capture through the existing MITM proxy.** Both clients are recorded
   with `umicraft-packet-logger` against a local offline-mode 1.8.8 server
   (PandaSpigot/Paper). The proxy needs no change to either client, and
   offline mode means no encryption, so the frames can be decoded. One play
   session per capture file; status pings before it are skipped.
2. **Tick boundaries from the vanilla rule.** The serverbound stream has no
   tick marker. Every on-foot tick of the vanilla client ends with exactly one
   movement packet (0x03–0x06), a riding tick with 0x05 + 0x0C (MCP
   `EntityPlayerSP.onUpdate` / `onUpdateWalkingPlayer`), so a tick is "the
   packets up to and including the next movement packet". Keep-alive answers
   are sent from the network thread and excluded. Ticks start after the
   answer to the initial placement. (`umicraft_testkit::trace`.)
3. **Two comparison views.** A session played by hand is not tick-exact, so
   `umicraft-trace-diff` compares the *set of tick shapes* (verdict and exit
   code) and lists the aligned ticks as a reading aid. `Entity Action` and
   `Player Digging` are compared with their sub-code.
4. **Trajectories from the movement packets**, or from a CSV written by
   `umicraft-movement-probe --trajectory`. They are only meaningful for
   deterministic input (scripted on both sides, or a simple held-key run).
5. **Screenshots compared outside the repository.** References contain
   Mojang assets (spec §3): they live in a directory the owner chooses and
   are never committed. Tests use synthetic images.
6. **The packet-coverage test** (`crates/umicraft-net/tests/packet_coverage.rs`)
   runs its consistency checks in CI; the gap audit is `#[ignore]`d so CI
   stays green, fails today, and P1 removes the `#[ignore]` when it passes.

## Proposed tolerances (ROADMAP §8.1) — owner approval needed

| Check | Proposal | Why |
|---|---|---|
| Trajectory | ≤ 0.01 block position error on every tick; on-ground flags equal | The 1.8 physics is deterministic in `double`; for the same input a faithful port should be off by rounding only. 0.01 block is a third of the vanilla 0.03-block report threshold, so any rule difference shows within a few ticks. |
| Screenshot | ≤ 1 % of pixels differing by more than 8 per channel, and mean absolute error ≤ 2.0 (0–255), at the same resolution and GUI scale; HUD regions that show time-dependent data may be masked | Allows driver and filtering noise, catches any wrong texture, shape or colour. |
| Trace | identical sets of tick shapes for the scripted session | P1's "same packet ids in the same order as vanilla". |

The values are constants in the code (`trajectory::PROPOSED_TOLERANCE`,
`screenshot::Threshold::PROPOSED`) and flags of the tools, so changing them
after the owner's decision is a one-line change plus this ADR.

## Consequences

- P1–P12 acceptance runs use the same three tools; their evidence is linked
  from `docs/parity.md`.
- The vanilla side is recorded by the owner (procedure in
  `docs/parity-harness.md`); the harness cannot launch the vanilla client.
- A capture of an online-mode server is refused (`TraceError::Encrypted`).
