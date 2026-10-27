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
