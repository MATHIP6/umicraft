# P6 — Combat & PvP: plan and architecture

Status: **P6a and P6b are done and checked in game** (2026-10-29: attacking
mobs and players works, the entity wins over the block behind it). P6c to P6f
remain. The decision record is `docs/decisions/0029-p6-combat.md`; MCP sources
are in `docs/sources.md` (rows dated 2026-10-29, "Entity ray cast" onwards).
The roadmap entry is `ROADMAP.md` §P6.

## Architecture

One rule per step: **a step adds a module, `game.rs` (4 400 lines) only gets
wiring.** Modules stay under about 400 lines, hold pure functions and test
their maths against the MCP formulas.

### Game layer — `umicraft-game/src/combat/`

| Module | Owns | State |
|---|---|---|
| `hitbox.rs` | box the ray tests: sizes, babies, slimes, item frames, marker stands, 0.1 border | done |
| `target.rs` | `pick_target` = MCP `getMouseOver`, block/entity priority, 3-block survival reach | done |
| `interact.rs` | `attack_packets`, `interact_packets`, which clicks the client consumes | done |
| `sprint.rs` | `sprint_reset` (motion x 0.6, sprint off), local only against players | done |
| `damage_view.rs` | hurt camera tilt, heart flash from `hurtTime` / `hurtResistantTime` | P6c |
| `bow.rs` | bow charge and FOV zoom (`EntityPlayerSP.getFOVMultiplier`) | P6d |
| `effects.rs` | typed `CombatEffect` (critical, sharpness) from the hit and `Animation` 4 / 5 | P6e |
| `death_message.rs` | `Combat Event` (`EntityDied`) to translated text | P6e |

### Client — `umicraft-client/src/combat.rs`

Gathers the inputs from `Game` (`pointed`, `after_attack`, `log_click`).
`Game::target()` is `pointed().block()`, so an entity nearer than the block
hides the outline and the digging.

### Wiring (done)

`AttackTick::entity` (attack instead of dig), `UseTick::entity` (entity packets
before the item use), `Outgoing::{Attack, Interact, InteractAt}`,
`ClientCommand::UseEntity`, `GameState::reset_sprint`.

### Planned, not started

- Render: `umicraft-render/src/hud/hearts.rs` (extracted from `hud.rs`, 1 127
  lines): normal, absorption, regeneration, poison and wither hearts, flash.
  The FOV zoom goes through `camera.rs`. The fishing line is a new
  `fishing_line.rs`.
- Parity register rows and a testkit 1v1 scenario.

## What remains

| Step | Content | Notes |
|---|---|---|
| P6c | hurt camera tilt (`view.rs` already has a `hurtCameraEffect` start), heart flash, absorption / regeneration / poison / wither hearts | Decision of 2026-10-29: hearts are done in P6 although `parity.md` listed "air, absorption, effect hearts" under P9; update that row. |
| P6d | bow FOV zoom, fishing line between hand and bobber, projectile tracking (snowball, egg, pearl, splash potion: only the use packet is sent, which is vanilla) | The bobber and the rod models already exist (P3, P5). |
| P6e | death message from `Combat Event`, critical and sharpness effects | The particles are drawn in P8: P6 emits the typed `CombatEffect`, P8 renders it. Record it as an accepted deviation (like ADR-0028). |
| P6f | `docs/parity.md` rows, `docs/sources.md`, scripted 1v1 in `umicraft-testkit` (melee with sprint resets, bow, rod, pearls, splash) compared with vanilla within the P0 tolerance | Needs the local PandaSpigot server (`runtime/panda-1.8.8`, `docs/parity-harness.md`). The anti-cheat check on a public network waits for P12. |

## Known gaps of P6a / P6b

- `interact.rs` knows only item frames, boats, minecarts, horses, adult
  villagers and armour stands as clicks the client consumes. Entities that
  react to the held item (cow and bucket, sheep and shears, wolf and bone,
  name tag, lead) fall through to the item use. Recorded in `docs/parity.md`.
- A spectator neither attacks nor interacts (`run_attack` returns early).
- Two `info` logs remain in `run_attack` (`attack click`, `attack click
  ignored`), added to diagnose a wrong-binary mix-up; lower them to `debug`
  or remove them when P6f is closed.
- Not compared with a vanilla session on a live server yet.

## Working note

Two checkouts exist on the owner's machine with the same `origin`:
`/home/mathip/dev/rust/mc-project/umicraft` (P6 work) and
`/home/mathip/repo/umicraft` (stale at P3). Run and build from the first, or
sync them, otherwise the client runs without the P6 code.
