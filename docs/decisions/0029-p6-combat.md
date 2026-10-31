# ADR-0029 — P6: Combat module layout

**Status:** accepted
**Date:** 2026-10-29
**Milestone:** P6 (ROADMAP.md)

## Context

P6 adds the entity ray cast, attacks and interactions, the sprint reset,
damage feedback, bow zoom, the fishing line and the death message.
`umicraft-client/src/game.rs` already has about 4 400 lines, so none of this
may land there beyond a few lines of wiring.

## Decision

### Game layer: `umicraft-game/src/combat/`

Pure rules, no I/O and no clock, tested against the MCP formulas.

| File | Owns |
|---|---|
| `hitbox.rs` | `hitbox(entity)`: the box the ray tests (size, baby, slime size, item frame, marker stand) plus the 0.1 border |
| `target.rs` | `pick_target`: MCP `getMouseOver`, the block/entity priority, the 3-block survival reach |
| `interact.rs` | `attack_packets`, `interact_packets`: the `Use Entity` sequences and whether the client consumes a click |
| `sprint.rs` | `sprint_reset`: motion x 0.6 and sprint off after a hit |

Later steps add `damage_view.rs` (hurt tilt, heart flash), `bow.rs` (FOV
zoom), `effects.rs` (critical hits) and `death_message.rs`.

### Client: `umicraft-client/src/combat.rs`

Gathers the inputs from `Game` (`pointed`, `after_attack`) and keeps `game.rs`
down to wiring. `Game::target()` is now `pointed().block()`: an entity nearer
than the block hides the outline and the digging, like `objectMouseOver`.

### Wiring

`AttackTick::entity` makes a click attack instead of dig; `UseTick::entity`
makes a right click send the entity packets before the use of the item.
`Outgoing` gains `Attack`, `Interact` and `InteractAt`; the client maps them
onto `ClientCommand::UseEntity`.

## Findings that differ from the roadmap text

- **Sprint reset is local only against players.** MCP applies it when
  `target.attackEntityFrom` answers true. On the client world a mob answers
  false (`EntityLivingBase.attackEntityFrom` returns early when `isRemote`)
  and `EntityOtherPlayerMP.attackEntityFrom` answers true. The server resets
  the sprint it tracks for mobs.
- **Entity beyond 3 blocks hides the block.** In survival an entity nearer
  than the block but farther than 3 turns the target into a miss.
- **Item frames** report the block as position; their box is centred on it
  and pushed 0.46875 against the wall.

## Consequences

- `interact_packets` knows only a few entities that consume a click on the
  client (item frames, boats, minecarts, horses, adult villagers, armour
  stands). Cows with a bucket, sheep with shears, wolves with bones and name
  tags still fall through to the item use (recorded in `docs/parity.md`).
- Spectators do not attack or interact (the early return of `run_attack` is
  kept); the spectator packets of vanilla are a P10 concern.
- Critical-hit and sharpness particles are emitted as typed effects in P6 and
  drawn in P8 (accepted deviation).
