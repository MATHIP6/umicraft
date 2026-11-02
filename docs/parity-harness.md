# Parity harness — procedure

How to compare Umicraft with the vanilla 1.8.8 client (ROADMAP.md P0,
ADR-0019). The results go into [`parity.md`](parity.md).

## Tools (`umicraft-testkit`)

| Binary | Compares | Exit code |
|---|---|---|
| `umicraft-packet-logger` | — records a session through a proxy into a `.umcapt` | — |
| `umicraft-trace-diff` | serverbound packets per tick of two captures | 0 same tick shapes, 1 different, 2 error |
| `umicraft-trajectory-diff` | per-tick reported positions (`.umcapt` or `.csv`) | 0 within tolerance, 1 outside, 2 error |
| `umicraft-screenshot-diff` | a Umicraft PNG against a vanilla reference PNG | 0 within threshold, 1 outside, 2 error |
| `umicraft-movement-probe --trajectory F.csv` | writes the client-side prediction as a trajectory | — |

Build them with `cargo build -p umicraft-testkit --bins`.

## 1. Server

A local **offline-mode** 1.8.8 server (PandaSpigot or Paper), e.g. on port
25599. A proxied online-mode session is encrypted and is refused by the
tools. Recommended for movement scripts: `difficulty=0` (a mob killed the
first recorded run), a flat world, and start each run from the same spot
(`/tp`) so absolute trajectories line up.

## 2. Record the vanilla session (owner)

```sh
umicraft-packet-logger 127.0.0.1:25600 127.0.0.1:25599 --out vanilla-movement.umcapt
```

Connect the vanilla 1.8.8 client to `127.0.0.1:25600` and play the timeline
below, then disconnect and stop the logger with `Ctrl+C` (it saves on exit).
One play session per file.

**Movement script (60 s)** — the same timeline as
`umicraft-client --headless --script movement` (`movement_script` in
`crates/umicraft-client/src/input.rs`; keep both in sync):

| From | To | Action |
|---|---|---|
| 0 s | 5 s | stand still |
| 5 s | 13 s | walk forward (W) |
| 13 s | 21 s | walk forward sneaking (W + Shift) |
| 21 s | 29 s | sprint forward (W + Ctrl) |
| 29 s | 37 s | walk forward and jump twice a second |
| 37 s | 42 s | look down a little, turning right then left every second |
| 42 s | 47 s | look up a little, turning right then left every second |
| 47 s | 55 s | walk forward |
| 55 s | 60 s | stand still |

## 2b. Combat script (owner, for P6)

The 1v1 timeline of `umicraft-client --headless --script combat`
(`combat_script` in `crates/umicraft-client/src/input.rs`; keep both in
sync). Set up on the same offline server, on flat ground, `difficulty=0`,
the same spot and the same opponent for both sides. A player walks through a
mob (the collision is soft in 1.8), so the opponent needs a barrier in front:
a row of `cobblestone_wall` (collision 1.5, so a jump does not clear it, but
the cursor ray passes above it) two blocks short of an invulnerable iron
golem that does not move. Console commands, as run on 2026-11-02 (player at
`-1080.5 4 868.5`, facing south, `+z`):

```
tp NAME -1080.5 4 868.5 0 0
fill -1086 4 874 -1075 4 874 cobblestone_wall
summon VillagerGolem -1080.5 4 876.2 {NoAI:1,PersistenceRequired:1,Invulnerable:1}
replaceitem entity NAME slot.hotbar.0 iron_sword 1
replaceitem entity NAME slot.hotbar.1 bow 1
replaceitem entity NAME slot.hotbar.2 ender_pearl 16
replaceitem entity NAME slot.hotbar.3 potion 3 16421
replaceitem entity NAME slot.inventory.0 arrow 64
```

Refill the items before each run (a run throws a pearl and a splash potion).
An invulnerable golem takes no damage and no knockback: this run compares the
serverbound sequence, not the knockback trajectories (those need a second
player). Hotbar: sword in slot 1, bow (with arrows) in slot 2, ender pearls in
slot 3, a splash potion in slot 4.
The script selects the slots itself; the vanilla side presses 1 to 4 at the
same times, a moment after letting go of the use button.

| From | To | Action |
|---|---|---|
| 0 s | 5 s | select slot 1 (sword), stand still |
| 5 s | 9 s | sprint forward (W + Ctrl) to the opponent |
| 9 s | 19 s | sprint and click the attack button five times a second |
| 19 s | 22 s | select slot 2 (bow), stand still |
| 22 s | 27 s | hold the use button (draw the bow) |
| 27 s | 30 s | select slot 1 (sword), stand still |
| 30 s | 40 s | sprint and click the attack button five times a second |
| 40 s | 50 s | walk forward and jump twice a second, clicking the attack button five times a second |
| 50 s | 53 s | hold the use button (sword block) |
| 53 s | 54 s | select slot 3 (pearl), stand still |
| 54 s | 55 s | press the use button once (throw the pearl) |
| 55 s | 56 s | select slot 4 (splash potion), stand still |
| 56 s | 57 s | press the use button once (throw the potion) |
| 57 s | 60 s | stand still |

Record the vanilla side as in step 2 into `vanilla-combat.umcapt`, the
Umicraft side as in step 3 with `--script combat` into
`umicraft-combat.umcapt`, and compare with `umicraft-trace-diff`. The
acceptance signal is the same tick shapes (an attack is `0x0A` then `0x02`,
a use of the item `0x08`, a held-slot change `0x09`, a sword block `0x08` then `0x07` (release), a sprint reset `0x0B`). A hand-played side only
approximates the clicks: the script presses the attack button every 200 ms,
because the vanilla client attacks an entity on each press and a held button
only keeps digging a block. A right click on the golem sends interact-at and
interact (`0x02` twice) before the item use (`0x08`): that is the vanilla order
and it is expected in both captures.

Umicraft side recorded on 2026-11-02 (`runtime/umicraft-combat.umcapt`, not in
git): slot 2, `0x02 0x02 0x08` (bow drawn), `0x07` (release), slot 1, about 150
attacks (`0x0A` then `0x02`), `0x02 0x02 0x08` and `0x07` (sword), slot 3 and
`0x02 0x02 0x08` (pearl), slot 4 and `0x02 0x02 0x08` (potion). The vanilla
side was played by hand on 2026-11-02 (`vanilla-movement.umcapt`, a free
session: sword blocks, then hits and a block while swinging, not the
timeline). Its tick shapes match: interact-at, interact, item use and release
are `0x02` (16 bytes), `0x02` (4), `0x08` (19), `0x07` (11); an attack is
`0x0A` then `0x02` (4) in the same tick. The comparison found one difference:
the attack followed the swing by 40 ms in Umicraft because the socket had
Nagle enabled; `Connection::connect` and the packet logger now set
`TCP_NODELAY` like the vanilla client (146 of 147 pairs at 0 ms after the fix).
A scripted vanilla run of the timeline above is still to record.

### Recording a public server

The logger forwards to `HOST:PORT` (the port is required) and the client
connects to the logger, so the handshake would carry `127.0.0.1`, which many
public servers and their proxies refuse. `--handshake-host` writes the real
name instead:

```sh
umicraft-packet-logger 127.0.0.1:25600 play.example.net:25565 --out session.umcapt &
umicraft-client 127.0.0.1 25600 --handshake-host play.example.net
# play, disconnect, then Ctrl+C in the logger
```

A server that encrypts the session (online mode) cannot be recorded.

## 3. Record the Umicraft session

```sh
umicraft-packet-logger 127.0.0.1:25600 127.0.0.1:25599 --out umicraft-movement.umcapt &
umicraft-client 127.0.0.1 25600 --headless --script movement --play-secs 62
kill -INT %1
```

## 4. Compare

```sh
umicraft-trace-diff vanilla-movement.umcapt umicraft-movement.umcapt --show 40
umicraft-trace-diff vanilla-movement.umcapt umicraft-movement.umcapt --dump   # every tick
umicraft-trajectory-diff vanilla-movement.umcapt umicraft-movement.umcapt --relative
```

The tick-shape verdict is the acceptance signal. The aligned-tick list and the
trajectory diff are exact only when both sides follow the same input tick for
tick; for a hand-played session use them to read the differences, not as a
pass/fail result.

## 5. Screenshots

Take the vanilla reference with F2 at a fixed resolution and GUI scale, store
it **outside the repository** (it contains Mojang assets, spec §3), render the
same view with Umicraft (`--screenshot`), then:

```sh
umicraft-screenshot-diff ~/umicraft-references/village.png out/village.png \
    --out out/village-diff.png --mask 0,0,300,120
```

## 6. Packet coverage

```sh
cargo test -p umicraft-net --test packet_coverage              # consistency (CI)
cargo test -p umicraft-net --test packet_coverage -- --ignored # gap audit, fails until P1
```

## First run (2026-10-26, Umicraft only)

`--script movement` against the local PandaSpigot 1.8.8 server through the
proxy: 842 ticks with a movement packet in 62 s. The tick shapes show the P1
traffic defects directly: `0x0C 0x04` on every moving tick (Steer Vehicle sent
on foot, up to 20 per tick while the chunks load), no `0x0B` on the sneak and
sprint transitions, and no `0x03` on idle ticks (Umicraft sends nothing when
vanilla sends `Player`). Diffing the capture against itself returns
`SAME SHAPES` / `WITHIN TOLERANCE`. No vanilla capture exists yet: recording
it (step 2) is the remaining owner step of the P0 acceptance.

## Vanilla comparison (2026-10-26)

The owner recorded `vanilla-movement.umcapt` with the vanilla 1.8.8 client,
following the timeline by hand (approximate timing), on the same server in
peaceful difficulty. Umicraft was re-recorded on the same server.

**Serverbound traffic** (`umicraft-trace-diff`, verdict `SHAPES DIFFER`):

| Behaviour | Vanilla | Umicraft | Owner |
|---|---|---|---|
| Idle tick | `0x03` 19 ticks out of 20, `0x04` every 20th | nothing, `0x04` every 20th | P1 |
| `Steer Vehicle` 0x0C on foot | never | every tick, before the movement packet; bursts of up to 20 before the world loads | P1 |
| Sneak / sprint transitions | one `0x0B` start and stop each, in the tick of the change, before `0x04` | none | P1 |
| Moving tick | `0x04` every tick (`0x06` when the look also changed) | same | matches |
| Look-only tick | `0x05` | same | matches |
| Keep-alive answers | outside the ticks | same | matches |

**Ground speed per phase** (median, from the reported positions):

| Phase | Vanilla | Umicraft |
|---|---|---|
| walk | 0.2159 block/tick | 0.2159 |
| sneak | 0.0648 | 0.0000 before the fix, 0.0648 after |
| sprint | 0.2806 | 0.2806 |

The sneak row found a bug: the sneak edge guard of
`umicraft_game::collision::move_entity` returned its unused X argument for
the Z axis, so a sneaking player never moved along Z (yaw 0 walks towards
+Z). Fixed with regression tests; a new live run confirms 1.30 block/s while
sneaking, as vanilla.

Script difference, not a defect: in the jump phase the vanilla player had
stopped sprinting (`0x0B:stop_sprint` before it), while the Umicraft script
goes from sprint straight to jump with W held, so Umicraft keeps sprinting
(vanilla does the same when W stays held). The jump phase speeds therefore
differ (3.9 vs 6.1 block/s) and are not compared.

## P1 run (2026-10-26)

Umicraft re-recorded with the P1 traffic (`umicraft_game::traffic`) on the
owner's offline server, compared with `vanilla-movement.umcapt`:

| Behaviour | Vanilla | Umicraft (P1) |
|---|---|---|
| Idle tick | `0x03`, `0x04` after 20 bare ticks | same |
| `Steer Vehicle` 0x0C on foot | never | never |
| Before the column loads | nothing | nothing |
| Sneak transitions | `0x0B` start/stop in the tick of the change, before the movement packet | same order |
| Sprint transitions | `0x0B` start/stop | none in this run |

The `umicraft` player started where an earlier run left it, against terrain
at (-51.7, 65.6, 237.3): it walked 16 ticks and was then blocked for the
rest of the script, so its ticks are `0x03` where vanilla's are `0x04`, and
a blocked player never starts sprinting. The packet ids and their order
within a tick match vanilla; the moving and sprint ticks still need a run
from a clear spot (`/tp umicraft …` by an operator before recording).
