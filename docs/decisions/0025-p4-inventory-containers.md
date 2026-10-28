# ADR-0025 — P4: Inventory & containers

**Status:** accepted
**Date:** 2026-10-28
**Milestone:** P4 (finishes M9; amends ADR-0016)

## Context

ADR-0016 laid the window model but nothing sends `Click Window`, the local
prediction does not exist and `inventory.rs` is a single 700-line file.
P4 adds every click mode, prediction with rollback and all container
screens. The spec (§14 rule 11) asks that the interface is frozen before the
work is split.

## Decision

### Game layer: `umicraft-game/src/inventory/`

| File | Owns |
|---|---|
| `kind.rs` | `WindowKind`, `SlotTarget`, slot-index resolution |
| `stack.rs` | helpers on the wire `ItemStack` (equality, stack limits) |
| `slot.rs` | `SlotKind` (what a slot accepts, its limit) and the slot layout of every window |
| `player.rs` | `PlayerInventory` (45 slots) |
| `armor.rs` | armour points read from the armour slots |
| `space.rs` | `ClickSpace`: the whole window as one indexable slot list |
| `click/mod.rs` | `ClickRequest`, `ClickMode`, `slot_click` dispatcher |
| `click/normal.rs` | modes 0 (left/right), 3 (middle), 4 (drop) |
| `click/swap.rs` | mode 2 (number keys) |
| `click/double.rs` | mode 6 (collect) |
| `click/drag.rs` | mode 5 (drag split) and `drag_sequence` |
| `click/transfer.rs` | mode 1 (shift-click), `merge_item_stack` |
| `transactions.rs` | pending predicted clicks and rollback |
| `manager.rs` | `WindowManager`: server sync + `click` + `confirm_transaction` |
| `view.rs` | immutable `WindowView` snapshots for the render thread |

The port of MCP `Container.slotClick` works on a `ClickSpace` and never on
packets, so it is unit-testable without a server. `WindowManager::click`
returns a `ClickCommand` (the fields of the `Click Window` packet); the
client only maps it onto `ClientCommand::ClickWindow`.

### Deliberate deviations from vanilla (server corrects them)

- No recipe book on the client. The crafting result slot is server driven;
  taking it consumes one item of each grid slot and clears the result.
- No furnace fuel / smelting / brewing ingredient validation, and shift-click
  uses one generic plan (container → player, player → container, skipping
  slots that do not accept the item) for containers other than the player
  window, the crafting table and the plain inventories.
- Item equality compares id, damage and raw NBT bytes (the NBT is kept
  verbatim on the wire).

### Transactions

The action number is pre-incremented like `Container.getNextTransactionID`
(first click = 1) and restarts when a window opens. Every predicted click
keeps a snapshot of the state before it. `Confirm Transaction`:
accepted → drop the snapshots up to it; rejected → restore that snapshot
and drop every later one (the server then resends the window).

### UI layer: `umicraft-ui/src/screens/` (P4b/P4c)

One file per screen implementing a `ContainerScreen` trait (layout of the
slot rectangles, background and foreground drawing). `widgets/inventory.rs`
is replaced by `screens/player.rs` and `screens/layout.rs`.

### Client layer

`game.rs` and `window.rs` receive no new inventory logic: it lives in
`game/inventory_events.rs` and `window/inventory_input.rs` (P4b).

## Consequences

- `held_item` and `equipment` leave `Game`; the inventory is the only source
  of the held item and of the armour bar (supersedes the ADR-0016 amendment).
- P4d (anvil, beacon, merchant, horse, book, sign, enchanting) and P4e
  (creative) build on the same `SlotKind`/`ContainerScreen` interfaces.

## Addendum (2026-10-28, P4a-P4c delivered)

- Screens are data: `ScreenSpec` (texture, size, slots, background pieces) and
  `Decoration` sprites for dynamic parts (furnace flame and arrow). `ScreenKind`
  is `#[non_exhaustive]`; window kinds without a screen fall back to a chest
  layout until P4d. `gui_scale` moved to `screens::scale`; `Ui::pointer()` was
  added for the cursor item.
- Input is a pure state machine (`umicraft_client::inventory_input`) on the
  render thread; it emits `GameEvent::Inventory`. The game thread filters the
  drag slots (`WindowManager::drag_targets`) and falls back to the plain click
  when none accepts the stack.
- Tooltips: `inventory::tooltip` is language free; keys are resolved by the
  renderer from an English table until a language loader exists.
