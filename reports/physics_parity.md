# Physics parity: Umicraft against Java 1.8.9

Status: **initial read-only audit.** No code changed, nothing run. Date: 2026-11-11.
Files: `umicraft-game/src/physics.rs`, `collision.rs`, `shapes.rs`, `input.rs`. Reference: `Entity.java`,
`EntityLivingBase.java`, `EntityPlayer.java`, `MathHelper.java` in `src/minecraft`.

## Confirmed (structure)

- `Entity.moveEntity` is ported with the same order and conditions (`collision.rs:97-255` against
  `Entity.java:686-810`): collision list from the swept box, Y then X then Z, `flag1 = onGround || (dy != y && dy < 0)`,
  step-up with the high and the low path, "the path that travels further wins", the revert test
  `d11² + d8² >= x² + z²`, `isCollidedHorizontally`, `onGround = collidedVertically && dy < 0`.
- The sneak edge rule (0.05 steps, `-1.0` probe) follows `Entity.java:616-684`.
- Jump (`0.42`, sprint boost `0.2`, cooldown 10), gravity `0.08`, vertical damping `0.98`, input damping
  `0.98`, ground friction `slip*0.91`, `0.16277136 / f³`, air control `0.02`: spot-checked, same constants.
- The AABB offset functions in `shapes.rs` were rewritten (uncommitted, branch structure and `&` instead of
  `&&`). By inspection the result is identical to `AxisAlignedBB.calculateXOffset`; **no equivalence test
  exists for it**.

## Confirmed divergences

| # | Java | Umicraft | Size |
|---|---|---|---|
| P1 | `MathHelper.sin/cos`: 65536-entry `float` table, index `(int)(x*10430.378F) & 65535` (`MathHelper.java:30-40`); `moveFlying` and the sprint jump use it | exact `f32::sin_cos` (`physics.rs:452,860`) | Table quantisation is `2π/65536` rad, so up to about `5e-5` relative in the direction of the velocity each tick. **Not measured yet.** |
| P2 | `moveFlying`, friction, `moveStrafing *= 0.98F`, `getAIMoveSpeed` are `float` | `f64` throughout (`physics.rs:463-464,848-864`) | About `1e-8` relative per tick. **Not measured yet.** |
| P3 | `resetPositionToBB`: `posX = (minX+maxX)/2` after every move | `position += delta` | Last-bit differences. **Not measured yet.** |

None of P1-P3 is large enough on paper to explain a reported `Simulation` offset of `0.002242`; they are listed
because they are real differences and cheap to test. Do not treat them as the cause.

## Unverified

- **Stairs, slabs, inner and outer corners.** `BlockStairs.java` builds its collision list from
  `setBaseCollisionBounds` plus the corner logic; the Rust table (`shapes.rs`, 52 stair mentions) was not
  compared box by box. This is the first suspect for the reported stair rollbacks. Test plan below.
- Ladders, webs, liquids, soul sand, slime: ported with tests in `physics.rs`, not compared numerically.
- Knockback: handled by the server velocity packet; the client-side application was not read yet.
- `Player.speed_multiplier` and the attribute modifier cast to `float` (`getAIMoveSpeed`).

## Test plan (deterministic, nothing written yet)

1. **Trig table test.** Port `SIN_TABLE` in the test only; assert the maximum difference with the exact
   functions over all 65536 indices and over a sweep of yaw degrees. This measures P1 before any change.
2. **Tick-by-tick trajectory.** `umicraft-movement-probe --trajectory` writes the prediction; compare with a
   vanilla recording through `umicraft-trajectory-diff` for the 60 s script in `docs/parity-harness.md`.
3. **Stair table.** For every stair meta (0-7 facing/half, plus the four neighbour configurations of the
   corner logic) assert the list of boxes equals the `BlockStairs` result, then walk onto and off each shape
   in a harness world and assert `onGround` and `y` per tick.
4. **AABB equivalence.** Property test of the rewritten `calculate_*_offset` against a literal copy of the Java
   body.

## Tests

Executed: none specific. The existing unit tests in `physics.rs` (about 60) are included in the baseline run
recorded in `vanilla_parity_audit.md`. Not executed: everything in the plan above.
