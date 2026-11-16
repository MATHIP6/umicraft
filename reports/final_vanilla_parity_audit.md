# Final vanilla parity audit (phase 3.1)

Date: 2026-11-11. Scope: the four points left open by `slot_action_vanilla_parity_fix.md`. The yaw (phase 1),
FIFO (phase 2), congestion (phase 2.1) and the tick scheduling of phase 3 are preserved. No claim that this
resolves the Minemen ban or any anti-cheat alert; nothing was run against a server.

Summary: **3 divergences confirmed and fixed (points 1, 2, 3), 1 point cleared (point 4: the packet order was
already faithful).** Production files modified: **4**. Tests added: **16**.

## Point 1: several digit keys in one tick

**Vanilla (read in `src`).**
- The wheel is handled first, in the `Mouse.next()` loop (`Minecraft.java:1813-1856`): `changeCurrentItem(j)`
  steps from the current slot, direction only, wrapping.
- The keyboard loop (`:1879-1886`) does `KeyBinding.onTick(key)` for each key event: `pressTime++`
  (`KeyBinding.java:24-34`). The slot is not touched there.
- The hotbar stage (`:2049-2064`) is `for l in 0..9 { if keyBindsHotbar[l].isPressed() { currentItem = l } }`.
  `isPressed()` (`KeyBinding.java:101-112`) takes **one** press per call. So per tick: each pressed key gives up
  one press, keys are visited in **index order** (the higher key wins whatever the physical order), a key
  pressed twice keeps a press for the next tick (it selects again then, after that tick's wheel), and the wheel
  only matters when no key was pressed.

**Umicraft before.** The window kept one `Option<u8>` shared by the digit keys and the wheel
(`window/binding_input.rs`, `window/hotbar_scroll.rs`): the last physical event won, and a repeated key was one
press. **Divergence confirmed** (keys 3 then 1 gave slot 0; vanilla gives slot 2).

**Fix.** `InputUpdate.hotbar_keys: [u8; 9]` carries the presses per key from the window (`binding_input.rs`
counts them, `window/mod.rs` sends and resets them, `unpress_all` clears them like `unPressAllKeys`). The game
accumulates them in `hotbar_presses` and `run_hotbar_keys` (called by `tick()` at the keyboard stage) gives up
one press per key in index order. The wheel keeps `select_slot`. For the player's view the highest pending key
is selected at once.

**Tests (game level, packets).** `two_digit_keys_in_one_tick_give_the_higher_key_whatever_the_order` (3,1 and
1,3), `three_or_more_keys_give_the_highest_key` (four orders), `keys_pressed_in_different_ticks_each_count_in_
their_tick`, `a_key_pressed_twice_selects_again_at_the_next_tick_over_the_wheel`,
`a_key_wins_over_the_wheel_in_the_same_tick_whatever_the_order`, `two_updates_with_the_same_key_between_two_
ticks_count_twice`.

**Not verified.** The window part (`App` needs a window) has no unit test; it is a counter increment. The
spectator hotbar (a menu in vanilla) is not modelled. The wheel's step is computed in the window from the
slot shown by the HUD, as before.

## Point 2: slot chosen by the server

**Vanilla.** `handleHeldItemChange` (`NetHandlerPlayClient.java:598-606`) sets only
`inventory.currentItem`. `PlayerControllerMP.currentPlayerItem` (`:62`) changes only inside
`syncCurrentPlayItem` (`:375-383`), which sends C09 when the two differ. `updateController` runs it at the start
of the next `runTick` (`Minecraft.java:1736`). So the client **announces the server's slot back** at the next
tick (nothing if it equals the slot the server was last told).

**Umicraft before.** The handler also set `synced_slot`, so nothing was sent back. **Divergence confirmed.**

**Fix.** The handler sets `selected_slot` and `applied_slot` and leaves `synced_slot`: the next tick start
sends the C09.

**Tests.** `a_slot_chosen_by_the_server_is_announced_back_at_the_next_tick` (nothing at the event, then
`[slot5, move]`, once), `a_server_slot_equal_to_the_known_one_sends_nothing`,
`a_key_after_a_server_slot_is_announced_after_the_servers_slot` (`[slot5, move, slot2, move]`).

**Not verified.** How a given server reacts to the echo. It is what the 1.8.9 client does.

## Point 3: JoinGame and Respawn

**Vanilla.** `handleJoinGame` (`NetHandlerPlayClient.java:277-292`) creates `new PlayerControllerMP`
(`currentPlayerItem` 0) and `loadWorld` creates the player when there is none (`Minecraft.java:2380-2385`),
with `InventoryPlayer.currentItem` 0. `handleRespawn` (`:1056-1073`) calls `setDimensionAndSpawnPlayer`
(`Minecraft.java:2412-2432`), which makes a **new player** (inventory slot 0) but keeps the **controller**:
`currentPlayerItem` stays, so the next sync sends C09 0 when it was not 0.

**Umicraft before.** Neither handler touched `selected_slot` (`WindowManager::new()` on join did reset the
window's own slot, so the two disagreed). **Stale state confirmed** after a join into a live `Game` or a
respawn.

**Fix.** `JoinGame`: `selected_slot = applied_slot = synced_slot = 0`. `Respawn`: `selected_slot =
applied_slot = 0` and the window's slot to 0, `synced_slot` untouched. Nothing else was reset.

**Tests.** `a_join_starts_both_sides_on_slot_zero` (no C09 after a join, even from slot 5),
`a_respawn_goes_back_to_slot_zero_and_tells_the_server_once` (slot 5 synced, then respawn: `[slot0]` once, none
before the next tick), `a_respawn_on_slot_zero_sends_nothing`.

**Not verified / not done.** In vanilla, a `JoinGame` that finds an existing player keeps its inventory slot
(`loadWorld` only creates a player when there is none); Umicraft resets to 0, which is the case of a fresh
connection. The window contents are not cleared on respawn (unchanged; the server resends them).

## Point 4: full order of `rightClickMouse` and `clickMouse`

**Vanilla (read in full).**
- `clickMouse` (`Minecraft.java:1500-1535`): `swingItem` (C0A) first; then, for an entity, `attackEntity`
  (sync, C02 attack); for a non-air block, `clickBlock` (sync, C07); on air nothing: **no sync**.
- `rightClickMouse` (`:1544-1606`): entity: `func_178894_a` (sync, C02 interact-at); if not consumed,
  `interactWithEntitySendPacket` (C02 interact); block: `onPlayerRightClick` (sync, then the C08 placement unless
  `canPlaceBlockOnSide` fails first; the swing C0A comes **after**, only when it returned true); if nothing
  consumed the click and the hand is not empty, `sendUseItem` (sync, C08 item use) with **no swing**.
- The held stack in the C08 is the stack of the current slot (after a pending change).

**Umicraft.** `run_attack` queues the swing then the controller packet; `run_use` queues the
`item_use` packets (`[interact-at, interact]` for an entity, `[place]` for a block, `[place, use]` when the item
does not place, `[use]` otherwise) and then the swing only when the click placed or activated; the tick's
`sync_held_item_before_click` puts the C09 ahead of the first non-swing packet. **No divergence found:** the
sequences match.

**Tests (exact sequences, with a pending slot change).**
- attack on a mob: `[swing, slot2, attack, move]`
- attack on air: `[swing, move]`, then `[slot2, move]` the next tick (no sync without a controller call)
- use on a block: `[slot2, place, swing, move]` (the swing after the placement)
- use of food in the air: `[slot2, use, move]` (no swing)
- iron sword on a block: `[slot2, place, use, move]` (no swing)
- right click on a mob, empty hand: `[slot2, interact_at, interact, move]` (no swing, no use)

**Not verified.** Spectator right click on a block (vanilla sends the C08 and swings), the
`canPlaceBlockOnSide` early return (no C08 placement, straight to the item use), the world border check, and
`Interaction::place_block` when the player stands in the way were not compared packet by packet with
`onPlayerRightClick`.

## Results

- `cargo check -p umicraft-client -p umicraft-game -p umicraft-net -p umicraft-protocol`: **finished, no error.**
- `cargo test -p umicraft-game -p umicraft-client -p umicraft-net -p umicraft-protocol --no-fail-fast`, run
  twice: every suite ok, 0 failed. Client lib **1129 passed**, 27 ignored (1113 + 16).
- Mutation check: with the key loop reversed, the server echo removed and the join/respawn resets removed, **6
  tests failed** (`two_digit_keys…`, `three_or_more_keys…`, `a_slot_chosen_by_the_server_is_announced_back…`,
  `a_key_after_a_server_slot…`, `a_join_starts_both_sides_on_slot_zero`, `a_respawn_goes_back_to_slot_zero…`);
  all three were then restored. Point 4 changed no code, so it has no mutation run.
- Previous phases intact: their tests are in the 1129 and pass (yaw 8, FIFO 13, congestion 5, phase 3 tests);
  grep confirms the `rem_euclid(TAU)` yaw line is still absent, `game.yaw += update.look_yaw` and
  `catches < MAX_CATCH_UP_TICKS && !queue.congested()` are in place.
- **Not run / known limit:** `cargo check` with `--tests` or `--benches` on `umicraft-client` fails inside
  `umicraft-render` (`boot_log`/`profile_scope` not found in `umicraft_core`); `cargo check -p umicraft-render
  --all-targets` and `cargo test` of the client both succeed, so this looks like a pre-existing feature/cache
  quirk of the uncommitted tree, not something these edits touch. It means the one-line change to the bench
  `benches/client_hotpaths.rs` (a new struct field) was **not compiled**. Also not run: clippy, the other
  crates, any real socket, PandaSpigot or GrimAC.

## Files and preservation

- Production files modified: **4**: `crates/umicraft-client/src/game.rs`, `src/input.rs`,
  `src/window/mod.rs`, `src/window/binding_input.rs`. One bench file got one line
  (`benches/client_hotpaths.rs`). Created in the untracked `reports/`: this report and
  `final_vanilla_parity_audit.diff` (exact production hunks and the extracted new tests).
- Tests added: **16** (the names are in the diff file). Existing tests changed: 2 and one helper
  (`a_slot_chosen_by_the_server_is_not_announced_back` renamed and inverted,
  `a_slot_before_a_use_is_announced_before_the_use_packet` now asserts the whole sequence, `tags()` splits
  placement/use and the three entity actions).
- `git status --short` still lists **113** paths; no reset, checkout, clean or stash. Your uncommitted changes
  are preserved.
