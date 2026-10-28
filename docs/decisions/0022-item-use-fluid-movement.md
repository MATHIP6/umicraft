# ADR-0022 — P2: item use, pick block, drop and liquid movement

**Status:** accepted
**Date:** 2026-10-28
**Milestone:** P2 (ROADMAP.md)

## Context

P2 still lacked the right button (hold and release, use slowdown), the middle
button, the drop keys and movement in fluids. The fluid part was a surprise:
ADR-0017 says water, lava, ladders, cobwebs and soul sand were implemented, but
commit `05f0cc1` ("Fix somes issues and improve code structure") had removed
their code from `physics.rs`, the `media` module from `umicraft-game` and the
`fluid` module from `umicraft-world`, leaving `media.rs` and `fluid.rs` as
orphan files. No test noticed, because the tests were removed with the code.

## Decision

1. **Liquid movement is restored** from the history (`05f0cc1^`) onto the
   current physics: `travel` picks the flight / no-clip, water, lava or land
   branch. The restored tests come back with it, plus a Depth Strider test and
   a flight-in-water test. Depth Strider is read from the `ench` NBT of the
   boots (`WindowManager::boots`) once per tick.
2. **`umicraft_game::item_use::ItemUse`** owns the use state and the 4-tick
   repeat. Its `tick` gets the button state, the target and the held stack,
   and returns `Outgoing` packets (`Place`, the new `UseItem`, the release as
   `Dig`). It calls `Interaction::place_block`, so a click on a block still
   places; only a click the block did not consume falls through to the item
   use, like `rightClickMouse`.
3. **`Player Block Placement` has `face: Option<Facing>`**: `None` is the
   wire byte 255 with position (-1, -1, -1), which is what the vanilla client
   sends for `sendUseItem`.
4. **The slowdown lives in `RawInput::using_item`** (0.2 of the axes, after the
   sneak scale), so the sprint rules see the reduced forward input exactly as
   in the reference. It is off while riding.
5. **`umicraft_game::pick`** maps a block to its item and applies
   `InventoryPlayer.setCurrentItem` to the 36 main slots; the client applies the
   result to the local window and sends `Creative Inventory Action` for the
   selected slot in creative.
6. **Drop** is a `Player Digging` packet (status 4 or 3); the server answers
   with the slot.

## Consequences

- The item, food and block tables are placeholders for the P3 registry; the
  gaps are listed in `docs/backlog.md`.
- The input path carries three more fields (`use_item`, `pick_block`,
  `drop_item`); the headless script does not use them yet.
- Liquid movement is unit-tested against the reference formulas but has not been
  compared with vanilla on a live server (the `course` headless script and the
  parity harness should be run for that).
