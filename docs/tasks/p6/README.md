# P6 — Combat & PvP: plan

Status: **P6a and P6b are done.** The decision record is
`docs/decisions/0029-p6-combat.md`.

| Step | Content | State |
|---|---|---|
| P6a | `combat::{hitbox, target}`, entity heights and widths, block/entity priority | done |
| P6b | `combat::{interact, sprint}`, `Use Entity` attack / interact / interact-at, `ClientCommand::UseEntity` | done |
| P6c | hurt camera tilt, heart flash, absorption / regeneration / poison / wither hearts (`umicraft-render/src/hud/hearts.rs`) | todo |
| P6d | bow charge FOV zoom, fishing line, projectile tracking | todo |
| P6e | death message from `Combat Event`, critical and sharpness effects (drawn in P8) | todo |
| P6f | parity register, testkit 1v1 scenario | todo |

Each step adds a module; `game.rs` only gets wiring.
