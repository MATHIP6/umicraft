# ADR-0093 — P0 and P1 accepted on field use

**Status:** accepted
**Date:** 2026-11-22
**Milestone:** P0, P1 (ROADMAP.md)

## Context

ROADMAP principle 3 accepts a parity milestone with the parity harness. For P0
only the owner's decision on the ADR-0019 tolerances was left. For P1 one
harness run was left: a movement session started from a clear spot, so that the
moving ticks and the sprint `Entity Action` could be compared with vanilla. The
2026-10-26 run had the player blocked by terrain. The vanilla reference
capture of that run is no longer on the owner's machine either.

Several players, the owner among them, have since played with the client on
public 1.8 servers, and the traffic behaved normally there.

## Decision

1. The owner accepts **P1** on this field use instead of the last harness run.
   Everything else P1 asked for is met: the coverage test passes with the gap
   audit included, the fuzzers ran 600 s per target without a crash, and the
   2026-10-26 run matched vanilla on idle ticks, `Steer Vehicle` and the sneak
   transitions.
2. The owner accepts **P0**. The proposed tolerances of ADR-0019 become the
   accepted values: trajectory ≤ 0.01 block per tick with the same on-ground
   flags; screenshots ≤ 1 % of pixels differing by more than 8 per channel,
   with a mean absolute error ≤ 2.0; traces with identical tick shapes.

## Consequences

- ROADMAP open point §8.1 is closed, and P0 and P1 are marked done.
- The rows of `docs/parity.md` keep their state. A row still moves to "Matches
  vanilla" only with harness evidence, and P13 remains the place to run the
  harness scripts against vanilla.
- If a server or an anti-cheat later reports a traffic difference, it is fixed
  as a P1 regression with a test, as for any bug.
