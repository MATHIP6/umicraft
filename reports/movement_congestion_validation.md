# Movement congestion validation (phase 2.1)

Date: 2026-11-11. Scope: only the `PENDING_LIMIT = 128` bound and the `congested()` guard of
`movement_packet_scheduling_fix.md`. The FIFO architecture, the phase 1 yaw fix, and slot change, drop and
pick-block code are untouched. No claim about the Minemen ban or any anti-cheat.

**Result: no defect found in production code; no production line was changed.** Only tests were added.

## Execution paths verified (read in `game.rs`, `run`)

When the queue holds 128 commands:

1. Step 1 still drains `inputs` and runs `handle_input` (keys, mouse, click flags, slot changes).
2. Step 2: `now >= next_tick`, but the `while` condition `!queue.congested()` fails, so no `tick()` runs.
   `next_tick <= now` then clamps `next_tick = now + TICK`, and `queue.flush()` runs anyway at the end of
   step 2.
3. Step 3 still processes up to 32 network events per pass and flushes again.
4. The loop then waits in `select!` on inputs, events or a timeout of at most one tick, so it wakes at least
   every 50 ms and retries `flush` even if nothing else happens. A flush moves what the channel (capacity 64,
   `network::COMMAND_CAPACITY`) accepts, oldest first, and keeps the rest.
5. When the socket drains, the next wake flushes, the queue falls below 128, and the next pass with
   `now >= next_tick` runs ticks again. Because `next_tick` was clamped to `now + TICK` on every congested
   pass, there is no burst of catch-up ticks after the recovery.
6. Nothing in the game thread blocks on the queue: `flush` uses `try_reserve`, so a full channel cannot
   deadlock it against the network thread, whose blocking `events.send` is served by step 3.

Findings that are not defects, to be aware of:

- **The bound is soft.** The guard is checked before each tick, and input commands (slot changes, drops, chat)
  are pushed outside it. The queue can exceed 128 by the commands of one pass: up to 3 ticks of reports plus
  input commands. It cannot grow while the socket drains, and a stalled socket stops the ticks.
- **Latency of a flush.** After the socket resumes, the queue is flushed at the next wake of the game thread
  (up to 50 ms), 64 commands at a time. With the real channel this empties 128+ commands in a few wakes.
- **Input during congestion.** `handle_input` keeps working: the view turns, key state is stored, a click sets
  `attack_pressed`/`use_pressed` (a flag, so two clicks before the next tick are one, as at any 20 Hz tick),
  and each slot change enqueues a `HeldItemChange` behind the backlog. Nothing is discarded.
- **`ServerView`.** It records a report when the tick pushes it to the FIFO (enqueued), not when the bytes
  leave the socket; the code cannot know the latter. In this design the two differ only if the connection
  dies, and then the session is rebuilt (`ServerView::default()` on the next `JoinGame`). While the connection
  lives, every enqueued report is sent exactly once and in order (tests below), so the server's last known
  position is the one `ServerView` assumes. `try_send` returns `Ok` and drops the command only when the
  network thread is gone (channel closed), which ends the session.

## Tests added (test code only, `crates/umicraft-client/src/game.rs`, `mod tests`)

The first four run the **real `run` loop on its own thread** with channels the test holds. The commands
channel has the production capacity (64) and is only read when the test reads it, which is how a stalled
socket looks to the game thread. The tick counter is the `RenderEvent::Frame` of each executed tick.

1. `the_real_loop_stops_ticking_while_congested_and_resumes_when_the_network_drains`: after 12 ticks, 214 slot
   changes fill the channel and the queue; for 1.1 s no tick runs; a mouse turn and a slot change sent during
   the congestion are accepted; the test then reads the socket side for 1.5 s: 10 to 35 ticks run (normal pace,
   no catch-up burst); the slot changes arrive exactly once each and in order; the falling player's reports
   have no gap and no duplicate; the turn made during the congestion reaches the wire as a yaw of 28.65°; the
   number of reports equals the number of ticks plus the one placement acknowledgement.
2. `several_congestions_and_recoveries_keep_the_order_and_lose_nothing`: three cycles of congestion and
   recovery; no tick during each stall, ticks again after each; FIFO and no gap over all three.
3. `a_congested_loop_keeps_serving_events_and_stops_on_request`: while congested, 1,000 events and inputs are
   all consumed (no deadlock), no tick runs for 400 ms after, and the loop stops within 1 s when asked.
4. `without_congestion_every_tick_sends_exactly_one_report`: the same loop with a draining socket: reports =
   ticks + 1 (the acknowledgement), no gap.
5. `a_full_queue_drains_through_a_small_channel_in_order` (queue level): 256 commands through a channel of 4;
   `congested()` goes from true to false; FIFO, no duplicate.

Harness findings while writing them (test setup, not production): an empty chunk with a zero section mask means
"unload" in 1.8 and my first world never loaded, and a channel of 2 drains too slowly to model the socket (the
flush runs once per wake). Both were fixed in the harness.

**Mutation check.** I removed `&& !queue.congested()` from the loop for one run: tests 1 and 2 failed
("no tick starts while the queue is full"). Test 3 passed that run because its "no tick" window was too short;
I lengthened it (400 ms) afterwards but did not repeat the mutation run for it. The guard was restored
(verified by grep at `game.rs:1337`).

## Results

- New tests: **5**, all pass, 3 consecutive runs of the group (4.9 s each) with no failure.
- `cargo check -p umicraft-client -p umicraft-game -p umicraft-net -p umicraft-protocol`: finished, no error.
- `cargo test -p umicraft-game -p umicraft-client -p umicraft-net -p umicraft-protocol --no-fail-fast`: every
  suite ok, 0 failed. Client lib: **1101 passed**, 27 ignored (1096 + 5).
- Phase 1 and 2 intact: their tests are in that count; the `rem_euclid(TAU)` line of the yaw is still absent.

## Files and preservation

- Files modified: **1**, `crates/umicraft-client/src/game.rs`, test module only (plus this report in the
  untracked `reports/`). Production lines changed: **0**.
- `git status --short` still lists **113** paths; no reset, checkout, clean or stash was used. Your uncommitted
  changes are preserved. A `sed -i` limited to my own new test lines changed a chunk mask and harness
  arguments; I checked that only my harness line carries `primary_bit_mask: 1`.

## Remaining limitations

- Timing tests use real time (50 ms ticks, windows of 0.4 to 1.5 s). The bounds are wide, but a heavily
  loaded machine could make them flaky.
- Not run: `cargo clippy`, the other crates, a real socket (`drive` against a stalled TCP peer), a PandaSpigot
  or GrimAC session.
- `PENDING_LIMIT = 128` remains my choice, not a vanilla value; the bound is soft by up to one pass of commands.
- Ticks are skipped (time stops) while congested; the simulation does not catch up afterwards by design.
