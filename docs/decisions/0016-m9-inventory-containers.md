# ADR-0016 — M9: Inventory & Container Interfaces

**Status:** accepted  
**Date:** 2026-10-26  
**Milestone:** M9

## Context

M9 implements GAM-5: the 1.8 window protocol and the inventory/container GUIs.
The player must be able to open their inventory, interact with server-opened
containers (chests, furnaces, crafting tables, …), move items between slots,
and acknowledge transaction ids so the server does not desynchronise.

Dependencies: M4 (UI foundation) and M5 (model system / texture atlas).

## Decision

### Layered architecture

The implementation is split into five layers with no upward dependencies:

1. **`umicraft-protocol`** — ten new packet structs (six clientbound, four
   serverbound) following the existing one-file-per-packet pattern.

2. **`umicraft-net`** — six new `PlayEvent` variants forwarded by
   `PlaySession::react()`. A rejected `ConfirmTransaction` also produces an
   automatic reply (the echo the server requires).

3. **`umicraft-game/inventory`** — a pure, I/O-free inventory model:
   `PlayerInventory` (45 slots), `ContainerWindow`, `WindowManager` (owns
   the player inventory, the optional open container, the cursor item and
   the transaction counter). The slot-mapping logic between window-relative
   indices and the player inventory's absolute indices is a tested helper
   (`resolve_slot`).

4. **`umicraft-ui`** — a `slot` widget (background, item-id text, stack
   count, damage bar, hover highlight) and layout functions
   (`player_inventory_layout`, `container_layout`) that return slot
   rectangles. No new GPU pipeline; the existing `draw list → ui_pass`
   path renders the overlay.

5. **`umicraft-client`** — the `Game` struct gains a `WindowManager`. The
   six new `PlayEvent` arms apply the server updates; the HUD hotbar is
   derived from `WindowManager::hotbar()` instead of the former `held_item`
   field. Slot interactions flow from the UI thread to the game thread as
   `SlotAction` values in `InputUpdate`; the game thread builds the
   `ClickWindow` packet and sends it.

### Wire ids (1.8 protocol 47)

Clientbound: `OpenWindow` 0x2D, `CloseWindow` 0x2E, `SetSlot` 0x2F,
`WindowItems` 0x30, `WindowProperty` 0x31, `ConfirmTransaction` 0x32.

Serverbound: `CloseWindow` 0x0D, `ClickWindow` 0x0E,
`ConfirmTransaction` 0x0F, `CreativeInventoryAction` 0x10.

### Slot mapping (player inventory window, id = 0)

| Range | Meaning |
|---|---|
| 0 | Crafting output |
| 1–4 | Crafting grid (2×2) |
| 5–8 | Armour (head, chest, legs, feet) |
| 9–35 | Main inventory |
| 36–44 | Hotbar |

Container windows prepend their own slots (0..*N*); the player's main
inventory follows at *N*..(*N*+27) and the hotbar at (*N*+27)..(*N*+36).

### Click modes

Mode 0 = normal, 1 = shift-click, 2 = number key, 3 = middle (creative),
4 = drop, 5 = drag, 6 = double-click. The `ClickMode` enum in the
inventory model encodes each into the `(button, mode)` pair the wire
packet expects.

### Out of scope

Item tooltips, enchantment glint, creative inventory browser UI, item
texture sprites in slots (the HUD's fallback text rendering is reused).

## Consequences

- `held_item` and `equipment` loose fields in `Game` are replaced by the
  `WindowManager`, which is the single source of truth for item state.
- The HUD hotbar is always correct because it reads from the real inventory
  slots instead of the single `EntityEquipment` packet.
- Container GUIs use the existing immediate-mode UI layer; no new render
  pipeline is introduced.
- The transaction counter and the automatic echo of rejected confirmations
  prevent the server desync that the acceptance criteria test for.

## Amendment (2026-10-26)

The consequence stating that the `held_item` and `equipment` fields of `Game`
(`crates/umicraft-client/src/game.rs`) are replaced by the `WindowManager`
does not match the code: both fields still exist. `Entity Equipment` packets
for the local player still update them, and the armour value is still derived
from `equipment` (`total_armor`) instead of from the inventory armour slots.
The `WindowManager` is therefore not yet the single source of truth for the
local player's items. Removing both fields and reading the armour bar from
slots 5–8 is planned in milestone P4 of `ROADMAP.md`.
