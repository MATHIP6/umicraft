# vanilla-agent (phase E.2)

A Java 8 `-javaagent` that drives the **vanilla 1.8.9 client** through a tick-driven key script and logs the
player's internal state every tick. It is a measuring tool for the parity harness, outside the Umicraft
production code. It never writes a position, a motion or a rotation and sends no packet of its own.

```sh
sh build.sh                                   # JDK 8 -> build/e2-agent.jar
java -javaagent:build/e2-agent.jar=scenario=scenarios/e1.txt,log=out/vanilla-e1-state.csv \
     -cp <1.8.9 classpath> net.minecraft.client.main.Main ...
```

## How it works

- At class load the agent rewrites `bew` (obfuscated `EntityPlayerSP`) **in memory** with the JDK-internal ASM:
  `Hook.before(this)` at the start of `t_()V` (`onUpdate`) and `Hook.after(this)` before each `RETURN`.
  The `1.8.9.jar` on disk is never modified.
- `before` counts client ticks, decides the scenario phase, and sets the `pressed` flag of the seven
  `KeyBinding`s (W, A, S, D, jump, sneak, sprint) found by their `keyDescription`. While the script runs every
  key is forced to the scripted state (a stray key cannot leak in); before and after it keys are left alone.
- `after` appends a CSV line read from the player after the physics step. Format:
  `tick,script_tick,phase,x,y,z,mx,my,mz,yaw,pitch,on_ground,sprinting,sneaking,keys_applied,keys_pressed_WASDJSC`
  (`motion*` are `Entity.motionX/Y/Z`; `Double.toString`/`Float.toString`, so values round-trip exactly).
- The script starts after `settle` consecutive on-ground ticks, ends after the last phase, and exits the JVM
  `exit_after` ticks later (`Runtime.halt`, log flushed first).
- Scenario: `scenarios/e1.txt`, shared with `crates/umicraft-client/src/e1.rs` (a test keeps them equal).

## Names it depends on

Obfuscated 1.8.9 names: `bew` (EntityPlayerSP, method `t_`), `pk` (Entity: `s t u` pos, `v w x` motion, `y z`
yaw/pitch, `C` onGround, `av()` isSneaking, `aw()` isSprinting), `ave` (Minecraft), `avh` (GameSettings), `avb`
(KeyBinding: `d` keyDescription, `h` pressed). They were read with `javap` on the 1.8.9 jar
(SHA-1 `3870888a6c3d349d3771a3e9d16c9bf5e076b908`). Another version fails loudly: the agent disables itself and
prints `[e2-agent] disabled: ...`.

## Interference risks

- Keys are forced at tick start, so a real key pressed during the script is overridden; do not use the
  keyboard or mouse while it runs. The mouse is not touched: a moving mouse on a focused window changes yaw.
- The hook adds two static calls per tick (microseconds); it does not change the tick order, but it runs inside
  `onUpdate`, so a tick that returns early is logged with the state it had.
- `JAVA_TOOL_OPTIONS` / `_JAVA_OPTIONS` must be empty for the process (another agent in the machine's
  environment would also instrument the client).
- The log holds `after` of `EntityPlayerSP.onUpdate`, not the packet payload; the packet is compared separately
  from the capture.
- Sprint needs food above 6; a hungry player gives a different run.
- Uses `jdk.internal.org.objectweb.asm` of JDK 8 (compiled with `-XDignore.symbol.file`); it does not exist
  with the same name on JDK 9+.
