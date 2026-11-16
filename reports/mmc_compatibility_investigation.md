# Minemen Club compatibility investigation

Status: **initial. No MMC-specific claim is made, and none can be made from the source.** Date: 2026-11-11.

## What is known

- Umicraft was banned on Minemen Club. **The reason is unknown.** The ban is not attributed to GrimAC or to any
  movement check: nothing in the repository shows what MMC runs or what it flagged.
- The alerts below come from a **different** setup (a test server with GrimAC), recorded in the task brief:
  `AimModulo360 x20`, `Post x20`, `Simulation x217 offset .002242`, packet name `held item change v1.8`.
  No capture or log of that session is stored in the repository.

## What the source audit shows

Three behaviours differ from vanilla 1.8.9 and are the only candidates found so far. They are differences in
behaviour, not statements about any server's rules.

| Candidate | Difference | Report | Evidence level |
|---|---|---|---|
| Wrapped yaw on the wire | vanilla never wraps | `rotation_parity.md` R1 | confirmed difference, effect likely |
| Movement packets overwritten under catch-up | vanilla one per tick | `network_protocol_parity.md` N1 | confirmed in code, not observed |
| Slot change/drop sent between ticks | vanilla sends at the start of the tick | `network_protocol_parity.md` N2 | confirmed in code, effect likely |

Fixing these makes the client closer to vanilla. It does not allow the claim "compatible with MMC".

## What is not established

- Why the account was sanctioned. It may have no relation to movement: a server can refuse a non-vanilla client
  by its rules, by its brand or by a mod list, independent of how it behaves.
- Whether the three differences above were even present on the day of the ban (the tree has 113 uncommitted
  paths; the build that connected is not identified).
- Anything about MMC's internals. This investigation does not try to find out and does not try to hide that
  Umicraft is a custom client.

## Legitimate next steps

1. Ask MMC through its appeal or support channel for the sanction reason, and read the rules on custom
   clients. This is the only source for the reason.
2. Do not play public matchmaking with this client until that answer exists.
3. Fix the three candidates one at a time, each with a regression test, then run the 60 s movement script
   against a **private** PandaSpigot 1.8.8 server with GrimAC and read the alerts (procedure in
   `docs/parity-harness.md`).
4. Record a vanilla 1.8.8 session through `umicraft-packet-logger` and compare per tick with
   `umicraft-trace-diff` and `umicraft-trajectory-diff`.
5. Keep the client honest about what it is: no spoofed brand, no hidden behaviour.

## Tests

Executed: none against MMC or any server. Not executed: private PandaSpigot/GrimAC runtime test (no server is
available in this environment), vanilla capture comparison.
