# P6 — Combat & PvP: plan and architecture

Status: **P6a and P6b are done and checked in game** (2026-10-29: attacking
mobs and players works, the entity wins over the block behind it). **P6c to
P6e are done** (2026-11-02, unit tests, not yet seen in game). P6f, the
scripted 1v1 against a live server, remains. The decision record is `docs/decisions/0029-p6-combat.md`; MCP sources
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
| `damage_view.rs` | `hurt_roll` (camera roll of `hurtCameraEffect`), `HeartsFlash` (the counters of `renderPlayerStats`) | done |
| `view.rs::bow_zoom` | bow FOV zoom (`AbstractClientPlayer.getFovModifier`), in `umicraft-game/src/view.rs` rather than a new module | done |
| `effects.rs` | `HitEmitter` (`EntityParticleEmitter`: 3 bursts), `burst`, `local_stars` (own critical and Sharpness stars against a player) | done |
| `death_message.rs` | not needed: the vanilla client shows nothing from `Combat Event` | dropped |

### Client — `umicraft-client/src/combat.rs`

Gathers the inputs from `Game` (`pointed`, `after_attack`, `log_click`).
`Game::target()` is `pointed().block()`, so an entity nearer than the block
hides the outline and the digging.

### Wiring (done)

`AttackTick::entity` (attack instead of dig), `UseTick::entity` (entity packets
before the item use), `Outgoing::{Attack, Interact, InteractAt}`,
`ClientCommand::UseEntity`, `GameState::reset_sprint`.

### Render and HUD (done)

`umicraft-render/src/hud_hearts.rs`: `HeartsView` (what the game thread says)
and `layout` (the sprites, the armour row, the shake). The game thread looks at
the health once per tick (`tick_hearts` in `game.rs`) and publishes the view
with the HUD only when it changes. The hurt roll goes through
`ViewEffects::with_hurt`, the bow zoom through `target_fov_modifier`. The
fishing line already exists from P5 (`entity_render::fishing`). The client
side of the stars is `combat::{emit_stars, tick_emitters}`.

## What remains

| Step | Content | Notes |
|---|---|---|
| P6c | hurt camera tilt, heart flash, absorption / regeneration / poison / wither hearts | done |
| P6d | bow FOV zoom; fishing line; projectiles (snowball, egg, pearl, splash potion: only the use packet is sent, which is vanilla) | done; nothing else to draw |
| P6e | critical and sharpness stars | done through `Animation` 4 / 5 and the local prediction against players; the death message is dropped (not vanilla) |
| P6f | Umicraft side recorded 2026-11-02 against a still golem (`runtime/umicraft-combat.umcapt`); a first hand-played vanilla capture matches the packet shapes (and showed the missing `TCP_NODELAY`, fixed); the scripted vanilla run and the knockback comparison remain. Scripted 1v1 in `umicraft-testkit` (melee with sprint resets, bow, rod, pearls, splash) compared with vanilla within the P0 tolerance | Needs the local PandaSpigot server (`runtime/panda-1.8.8`, `docs/parity-harness.md`) and a vanilla session to compare with: not done. The anti-cheat check on a public network waits for P12. |

## Known gaps of P6a / P6b

- `interact.rs` knows the clicks the client consumes: item frames, boats,
  minecarts, horses, adult villagers, armour stands, a bucket on a cow, a bowl
  or shears on a mushroom cow, the food of each animal, a saddle on a pig and
  a dye on a sheep, the lead, a named name tag, a spawn egg, the wolf's and
  ocelot's items, and the minecart types that answer true. An adult animal is
  taken as ready to breed (the client does not see the cooldown), a villager
  as not trading, a cart as free; a cured zombie villager is not consumed.
  Recorded in `docs/parity.md`.
- The ray meets boats, minecarts, crystals, large fireballs, TNT, falling
  blocks, paintings and knots with their own boxes (they had the player-sized
  fallback after P5 added their kinds).
- A player who follows another entity (spectator camera) does not attack or
  interact (`run_attack` returns early on `spectating()`); a free flying
  spectator does, as in the reference. The reference does not guard the click
  in the followed view: unchecked.
- The two diagnostic logs of `run_attack` are now `debug`.
- The stars of a hit on a mob come only from the server (`Animation` 4 / 5), as
  in the reference; the local prediction covers players.
- The hearts of a ridden mount and the air bubbles are not drawn (P9).
- Not compared with a vanilla session on a live server yet.

## Working note

Two checkouts exist on the owner's machine with the same `origin`:
`/home/mathip/dev/rust/mc-project/umicraft` (P6 work) and
`/home/mathip/repo/umicraft` (stale at P3). Run and build from the first, or
sync them, otherwise the client runs without the P6 code.
