# Movement packet scheduling fix (phase 2)

Date: 2026-11-11. Scope: finding A2 of `vanilla_parity_audit.md` only. The phase 1 yaw fix is untouched, and
slot change, drop and pick-block are untouched. **No claim is made that this fixes the Minemen ban or any
GrimAC alert**; its effect on a server is untested.

## Problem: confirmed

Re-checked in the code before changing anything.

- `CommandQueue` (`crates/umicraft-client/src/game.rs`) had two single slots, `position` (C03/C04/C05/C06) and
  `input` (C0C). `push` overwrote them; `flush` drained `ordered` first, then the position slot, then the input
  slot.
- `run` executes up to `MAX_CATCH_UP_TICKS = 3` ticks per pass and calls `queue.flush()` once afterwards.
  So with two or three ticks in a pass, tick 1's movement packet was replaced by tick 2's and never sent.
- `ServerView::tick` records a position as reported at the moment it produces the packet. Packets were
  therefore counted as sent though replaced: confirmed. The position reported later was measured from a point
  the server never received.
- Ordering: with several ticks before one flush, the clicks and actions of tick 2 (in `ordered`) were flushed
  **before** the movement packet of tick 1. Confirmed by reading `flush`.
- Capacity: the channel to the socket thread holds 64 (`network::COMMAND_CAPACITY`). `ordered` was unbounded.
- Refinement of the earlier audit: a catch-up pass needs the game thread to be at least one tick late
  (the loop counts them in `catch_up_ticks_total`); an ordinary pass runs one tick and was correct.

## Vanilla behaviour (Java 1.8.9, `src`)

- `Minecraft.runGameLoop` (`Minecraft.java:1096-1098`) runs `runTick()` once per elapsed tick, up to
  `elapsedTicks = 10` (`Timer.java:98-103`), each one reaching `EntityPlayerSP.onUpdate`.
- `EntityPlayerSP.onUpdateWalkingPlayer` (`:189-274`) adds the actions and exactly one movement packet to
  `sendQueue` every tick (the riding branch adds a C05 and a C0C, `:174-178`). `NetHandlerPlayClient.addToSendQueue`
  hands each to `NetworkManager.sendPacket`, a netty queue with no limit: **nothing is merged or replaced**.
- Clicks (`Minecraft.runTick`) happen before the entity update of that tick, so within a tick the order is
  clicks, actions, movement; across ticks the order is by tick.
- The horse jump (C0B `RidingJump`) is sent from `onLivingUpdate`, before the walking packets of the same tick.

## Files

Inspected: `umicraft-client/src/game.rs` (`CommandQueue`, `run`, `tick`, `report_movement`, `movement_command`,
`run_attack`, `run_use`, `select_slot`, tests), `network/command.rs`, `network/mod.rs`,
`umicraft-net/src/play/drive.rs`, `umicraft-game/src/traffic.rs`, `EntityPlayerSP.java`, `Minecraft.java`,
`Timer.java`.
Modified: **1 file**, `crates/umicraft-client/src/game.rs`. Created: this report (inside the already untracked
`reports/`). No other file was touched.

## The fix

1. `CommandQueue` has one FIFO, `ordered`; the `position` and `input` slots are gone, `push` appends. Every
   movement packet of every tick is kept, in the order the tick produced it, interleaved with the clicks and
   actions of its tick. `ServerView` is unchanged: pushing a packet to this queue now guarantees it is
   transmitted, in order, so "scheduled" is true when `ServerView` records it. `flush` drains the FIFO oldest
   first and keeps the unsent tail when the channel is full.
2. Bound: `PENDING_LIMIT = 128` commands (twice the channel). While `ordered` holds that many, `congested()` is
   true and `run` starts **no new tick** (an added condition on the catch-up loop; the existing clamp then
   waits one tick length). No packet is dropped, memory is bounded, and a socket that does not drain freezes the
   simulation instead of silently skipping reports. This is the only new behaviour; vanilla has no limit (its
   queue is unbounded and the connection times out).
3. `RidingJump` was pushed after `report_movement` and used to leave before the movement slot because
   `ordered` drained first. A new `push_before_movement` keeps it ahead of the report of its own tick (and
   after the actions), as before and as in vanilla.

Not done on purpose: no flush after every tick (the pass still flushes once, in order), no change to attack or
use code, no change to `ServerView`, `traffic.rs`, the network thread or `drive.rs`.

### Production diff

`game.rs` also holds your other uncommitted changes, so a plain `git diff` of the file cannot isolate this
phase. This is the exact set of edits made; long doc comments are abbreviated with `...`.

```diff
@@ struct CommandQueue
-/// The input packet and the position report are *coalesced*: only the newest
-/// one of each matters ... (PandaSpigot) ...
+/// Every command keeps its place: the movement packet of each tick, the
+/// clicks and the actions leave in the order they were queued ... nothing is
+/// coalesced or replaced.
+/// The queue cannot grow without bound: while it holds [`PENDING_LIMIT`]
+/// commands ..., [`CommandQueue::congested`] is true and the game loop does not
+/// start a new tick. No packet is dropped ...
 #[derive(Default)]
 struct CommandQueue {
-    /// The commands that keep their order (...).
-    ordered: VecDeque<ClientCommand>,
-    /// The newest input packet.
-    input: Option<ClientCommand>,
-    /// The newest position report.
-    position: Option<ClientCommand>,
+    /// The commands in the order they were queued.
+    ordered: VecDeque<ClientCommand>,
     sender: Option<CommandSender<PlayServerbound>>,
 }
+const PENDING_LIMIT: usize = 128;
+fn is_movement(command: &ClientCommand) -> bool {
+    matches!(command, Input{..} | Player{..} | PlayerPosition{..} | PlayerLook{..} | PositionLook{..})
+}
 impl CommandQueue {
     fn push(&mut self, command: ClientCommand) {
-        match command {
-            ClientCommand::Input { .. } => self.input = Some(command),
-            ClientCommand::Player { .. }
-            | ClientCommand::PlayerPosition { .. }
-            | ClientCommand::PlayerLook { .. }
-            | ClientCommand::PositionLook { .. } => self.position = Some(command),
-            other => self.ordered.push_back(other),
-        }
+        self.ordered.push_back(command);
     }
+    fn push_before_movement(&mut self, command: ClientCommand, since: usize) {
+        let mut at = self.ordered.len();
+        while at > since && is_movement(&self.ordered[at - 1]) {
+            at -= 1;
+        }
+        self.ordered.insert(at, command);
+    }
+    fn congested(&self) -> bool {
+        self.ordered.len() >= PENDING_LIMIT
+    }
     fn flush(&mut self) {
         ...
         while let Some(command) = self.ordered.pop_front() {
             if let Err(command) = try_send(sender, command) {
                 self.ordered.push_front(command);
                 break;
             }
         }
-        if let Some(command) = self.position.take() {
-            self.position = try_send(sender, command).err();
-        }
-        if let Some(command) = self.input.take() {
-            self.input = try_send(sender, command).err();
-        }
     }
 }
@@ fn run
     let mut queue = CommandQueue {
         ordered: VecDeque::new(),
-        input: None,
-        position: None,
         sender: Some(commands),
     };
@@ fn run (catch-up loop)
-            while now >= next_tick && catches < MAX_CATCH_UP_TICKS {
+            // A congested queue stops the ticks (see `PENDING_LIMIT`); the
+            // clamp below then waits one tick length before trying again.
+            while now >= next_tick && catches < MAX_CATCH_UP_TICKS && !queue.congested() {
@@ fn tick
+    let movement_start = queue.ordered.len();
     report_movement(game, &intent, queue);
...
-        queue.ordered.push_back(ClientCommand::RidingJump {
-            entity_id: game.entity_id,
-            strength,
-        });
+        // Ahead of the movement report of this tick, where the queue used
+        // to drain it before the movement slot.
+        queue.push_before_movement(
+            ClientCommand::RidingJump { entity_id: game.entity_id, strength },
+            movement_start,
+        );
```

The complete test code is in `game.rs` (module `tests`, from the comment `// Movement packet scheduling` on).

## Tests

Added: **13**, all in `game.rs` `mod tests`:

1. `a_normal_tick_keeps_its_movement_packet`
2. `three_catch_up_ticks_before_a_flush_keep_three_movement_packets`
3. `no_intermediate_position_is_lost` (8 ticks, strictly increasing, equal to the player's position per tick)
4. `a_movement_then_an_attack_leave_in_the_order_of_their_ticks`
5. `a_movement_then_a_use_leave_in_the_order_of_their_ticks`
6. `interleaved_actions_keep_one_report_per_tick_and_their_own_tick`
7. `a_full_channel_keeps_every_movement_packet_in_order` (capacity 2, five packets)
8. `a_flush_transmits_each_packet_once`
9. `what_the_server_view_reported_is_what_the_server_receives` (flushed wire packets replayed: the server's
   positions equal the player's per tick)
10. `an_idle_player_adds_nothing_but_the_bare_player_packet` (10 idle ticks give exactly 10 packets, no extras)
11. `the_backlog_is_bounded_by_pausing_the_ticks_and_never_by_dropping`
12. `the_input_packets_of_a_riding_player_are_kept_per_tick`
13. `a_riding_jump_leaves_ahead_of_the_report_of_its_tick_only`

Removed: 2 tests that asserted the old behaviour (`the_position_reports_are_coalesced`,
`the_input_packets_are_coalesced`); the new tests state the opposite.
Adapted (they read the removed slots): `an_idle_tick_sends_the_bare_player_packet_and_no_steer`,
`a_riding_tick_sends_look_then_steer`, the empty-queue assertion before them,
`a_full_channel_keeps_the_commands_for_the_next_flush`, and the phase 1 helper `turn_and_read_wire_yaw` (it now
takes the last queued command instead of the slot; its yaw assertions are unchanged).

Check against the old behaviour: the old code no longer exists, so I made `push` temporarily replace any queued
movement packet (the old slot semantics), ran the client lib tests, then removed that line. Result: **12 tests
failed** (11 of the new ones plus `a_riding_tick_sends_look_then_steer`); with the fix: 0 failed. This emulation
reproduces the replacement, not the old drain order, so the ordering tests 4-6 are only partly proven to
detect the old ordering problem.

## Results

- `cargo check -p umicraft-client -p umicraft-game -p umicraft-net -p umicraft-protocol`: finished, no error.
- `cargo test -p umicraft-game -p umicraft-client -p umicraft-net -p umicraft-protocol --no-fail-fast`: every
  suite ok, 0 failed. Client lib: **1096 passed**, 27 ignored (1085 + 13 − 2).
- Phase 1 intact: its 8 yaw tests are part of the 1096 and pass; the `rem_euclid` line is still gone.
- `git status --short` lists 113 paths, as before; no destructive Git command was used.

## Remaining limitations

- **Not run:** `cargo clippy`, the other workspace crates, a private PandaSpigot/GrimAC session, a vanilla
  capture comparison, a real catch-up under load (the tests call `tick` several times without a flush; the
  `run` loop itself, including the `congested()` guard, is not exercised by a unit test).
- The `PENDING_LIMIT` of 128 is my choice, not a vanilla value. A stalled socket stops the simulation after a
  few seconds of ticks instead of piling packets.
- `MAX_CATCH_UP_TICKS` is 3, vanilla 10. When the thread is more than 3 ticks late, `run` still skips time
  (the clamp), so the tick count can differ from vanilla then; a skipped tick sends nothing, it does not send a
  merged packet. Not changed here.
- Chat, dig and the other commands share the FIFO and count toward the 128 bound.
- Slot change, drop and pick-block are still sent at event time (finding A3, next phase, not started).
