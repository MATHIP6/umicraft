# Slot change, drop and pick-block: vanilla parity fix (phase 3)

Date: 2026-11-11. Scope: finding A3 of `vanilla_parity_audit.md`. Phase 1 (yaw), phase 2 (FIFO) and phase 2.1
(congestion) code is untouched. No claim that this resolves the Minemen ban or any anti-cheat alert; the
effect on a server is untested.

## Vanilla behaviour, action by action (read in `src`)

The three actions do **not** behave alike.

**Hotbar slot** (`Minecraft.java:2049-2064`, `PlayerControllerMP.java:345-382`)
- A hotbar key only sets `inventory.currentItem` inside `runTick`'s keyboard stage. No packet is built there.
- The C09 comes from `PlayerControllerMP.syncCurrentPlayItem`, which sends it only when `currentItem !=
  currentPlayerItem`. It is called (a) by `updateController` at the very start of every `runTick`
  (`Minecraft.java:1736`) and (b) inside each controller call that sends an action: `attackEntity`,
  `interactWithEntitySendPacket`, `func_178894_a`, `clickBlock`, `onPlayerDamageBlock`, `onPlayerRightClick`,
  `sendUseItem` (`PlayerControllerMP.java:283,347,388,460,493,507,514,564`).
- So presses are **merged**: only the final `currentItem` counts, and a press back to the slot the server knows
  sends nothing. With no click in the tick, the packet leaves at the start of the **next** tick (before that
  tick's movement packet); with a click, just before the click's controller packet.
- `clickMouse` (`:1500-1535`) calls `swingItem` (C0A) first and only then the controller, so the order is
  C0A, C09, then the attack/dig packet. `rightClickMouse`: C09 before the C08.
- The server's `S09` only sets `currentItem` (`NetHandlerPlayClient.java:598-606`), so vanilla would announce it
  back at the next sync.

**Drop** (`Minecraft.java:2080-2086`, `EntityPlayerSP.java:279-284`)
- `while (keyBindDrop.isPressed())` in `runTick`: **one C07 per press**, built immediately (no merging, no
  prediction), after the hotbar stage and before the clicks. It does **not** call `syncCurrentPlayItem`: a slot
  pressed in the same tick is announced after the drop.
- Skipped for a spectator.

**Pick-block** (`Minecraft.java:2108-2136, 2474-2610`)
- After the attack and the use in `runTick`, one `middleClickMouse` per press. While an item is in use the
  presses are read and discarded.
- Creative: `setCurrentItem` then `sendSlotPacket` (C10), which does **not** sync the slot. Survival: only
  `setCurrentItem` (a local slot change). Either way C09 follows at the next sync.

## Umicraft before the change (confirmed)

`handle_input` (event time, between ticks) selected the slot **and queued C09 immediately** (one packet per
press, no merging), queued the C07 of a drop immediately, and ran `pick_block` (C10 and C09) immediately. The
loop flushed them at once. So all three left up to 50 ms away from the movement packet of their tick, a slot
press made a packet per press, and a pick ran against the target of the event, not of the tick. Attack and use
were already tick-aligned and did not sync the slot.

## The change (`crates/umicraft-client/src/game.rs` only)

- New state: `applied_slot` (vanilla `currentItem` as the keys stage left it), `synced_slot`
  (`currentPlayerItem`), `pending_drops` (one entry per press), `pending_picks` (a count).
- `handle_input` only records: the selected slot changes at once for the player (HUD and hand react as
  before), but no packet is queued; drops and picks go to the pending state.
- `tick()` now follows `runTick`: `sync_held_item` (updateController), `applied_slot = selected_slot` (keys),
  the pending drops in order, `run_attack` then `sync_held_item_before_click`, `run_use` then
  `sync_held_item_before_click`, `run_pick_blocks`. The click sync inserts the C09 before the first packet of
  the click that is not the swing; a click that only swung syncs nothing.
- The server's `S09` handler also sets `applied_slot` and `synced_slot`, so it is **not** announced back (as
  Umicraft did before; see the limitations).
- Pick-block presses made while an item is in use are discarded, as in vanilla.
- Untouched: the FIFO, `PENDING_LIMIT`/`congested()`, the yaw line, `run_attack`/`run_use` bodies.

The exact production hunks and the new test block are in `reports/slot_action_vanilla_parity_fix.diff`
(`game.rs` has other uncommitted changes of yours, so a plain `git diff` cannot isolate this phase).

## Tests

New, all in `game.rs` `mod tests`, each checking the packets actually queued and their order (12):

1. `a_slot_press_leaves_in_the_tick_after_the_keys_stage_before_its_movement` (nothing at the event; tick N
   `[move]`, tick N+1 `[slot2, move]`, then once)
2. `several_presses_between_two_ticks_make_one_packet_with_the_last_slot` (and a press back to the synced slot
   sends nothing)
3. `a_drop_leaves_in_its_tick_before_the_movement_and_without_a_slot_sync`
4. `every_drop_press_is_a_packet`
5. `a_survival_pick_leaves_no_packet_in_its_tick_and_the_slot_in_the_next`
6. `a_creative_pick_sends_the_slot_packet_in_its_tick_and_the_sync_in_the_next`
7. `a_slot_before_an_attack_is_announced_between_the_swing_and_the_attack_packet`
8. `a_slot_before_a_use_is_announced_before_the_use_packet`
9. `the_actions_of_one_tick_keep_the_run_tick_order` (`drop, swing, slot2, dig, move`)
10. `a_movement_and_a_slot_of_different_ticks_keep_the_order_of_the_ticks`
11. `a_slot_chosen_by_the_server_is_not_announced_back`
12. `actions_pressed_while_congested_leave_once_after_the_resume` (the real `run` loop on its own thread: chat
    lines fill the queue, three slot presses and a drop arrive during the congestion; after the resume the
    chats keep their order, the drop leaves once, the slot packet once with the last slot and after the drop)

Changed (they encoded the old event-time behaviour): `the_selected_slot_is_announced_once`,
`q_and_ctrl_q_send_the_drop_packets`, `the_middle_button_selects_the_hotbar_slot_of_the_block`. The phase 2.1
loop harness now congests with chat lines (`spam_chats`) instead of slot presses, because presses between ticks
now make one packet and can no longer fill the queue; its assertions are the same ones on the chat lines.

**Check against the old behaviour:** I restored the event-time slot sync and the event-time drop in
`handle_input` for one run (then removed them): **9 tests failed** (new tests 1, 2, 3, 7, 9, 10, 12 and the changed
`the_selected_slot_is_announced_once` and `q_and_ctrl_q_send_the_drop_packets`). Not mutation-checked: the click
sync insertion position and the pick-while-using discard.

## Results

- `cargo check -p umicraft-client -p umicraft-game -p umicraft-net -p umicraft-protocol`: finished, no error.
- `cargo test -p umicraft-game -p umicraft-client -p umicraft-net -p umicraft-protocol --no-fail-fast`, run
  twice: every suite ok, 0 failed. Client lib **1113 passed**, 27 ignored (1101 + 12 new).
- Non-regression: the 8 yaw tests (phase 1), the FIFO tests (phase 2) and the five congestion tests (phase 2.1)
  are in that count and pass. The `rem_euclid(TAU)` yaw line is still absent and
  `catches < MAX_CATCH_UP_TICKS && !queue.congested()` is still in place (grep).

## Files and preservation

Modified: **1** (`crates/umicraft-client/src/game.rs`). Created in the untracked `reports/`: this report and
`slot_action_vanilla_parity_fix.diff`. `git status --short` still lists **113** paths; no reset, checkout, clean
or stash was used; no other file was touched.

## Limitations (not verified)

- Vanilla's keyboard stage runs the hotbar keys in index order (`for l in 0..9`), so with two number keys
  pressed in one tick the **higher** slot wins; the mouse wheel changes the slot per event. Umicraft gets one
  `select_slot` per update and keeps the latest. Equal for a single press, possibly different for two keys in
  one tick.
- Vanilla would echo the server's `S09` back with a C09 at the next sync; Umicraft keeps not doing so. Not
  changed because it adds a packet that did not exist; say if you want vanilla's echo.
- The slot state is not reset by `JoinGame`/`Respawn` (it was not before). A server switch keeps the old slot
  on both sides.
- The relative order of the swing and the placement inside `run_use` was not compared with `rightClickMouse`
  (only that the C09 precedes the placement).
- Hotbar keys while a screen is open, `leftClickCounter`, the creative Ctrl+pick with NBT and the item-in-use
  discard of the pick are not covered by a test (the discard is by code reading).
- Not run: `cargo clippy`, the other crates, a real socket, any PandaSpigot or GrimAC session.
- The loop-level tests use real time (windows of 0.3 to 1.5 s).
