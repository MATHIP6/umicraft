# 0082. F3 debug chords

Status: accepted

## Context

The vanilla 1.8.9 client lets the player press a letter while F3 is held
(`Minecraft.runTick`). Umicraft had two of them (`F3 + T`, `F3 + P`). The owner
asked for the rest, `F3 + B` and `F3 + G` among them.

## Decision

| Chord | Effect | Origin |
|---|---|---|
| F3 + A | re-mesh every loaded chunk (`GameEvent::ReMeshAll`) | 1.8.9 `renderGlobal.loadRenderers` |
| F3 + B | white box around every entity | 1.8.9 `renderManager.setDebugBoundingBox` |
| F3 + D | clear the chat history (`GameEvent::ClearChat`) | 1.8.9 `GuiNewChat.clearChatMessages` |
| F3 + F | render distance +1, Shift: -1, kept in 2..=16, sent to the server | 1.8.9 `setOptionValue(RENDER_DISTANCE, +-1)` |
| F3 + G | chunk borders around the player | later vanilla versions, not in 1.8.9 |
| F3 + H | advanced item tooltips (durability line), saved in `[controls]` | 1.8.9 `advancedItemTooltips` |
| F3 + S, F3 + T | reload the resource packs | 1.8.9 `refreshResources` |
| F3 + P | runtime profiler (unchanged) | Umicraft |

- **P conflicts with vanilla.** Vanilla's `F3 + P` is "pause on lost focus",
  which Umicraft does not implement (the game keeps running without focus). The
  profiler keeps the key; the vanilla meaning can take another letter when the
  option exists.
- **Not ported:** `F3 + C` held six seconds (the manual crash), and the later
  chords that need features of a newer protocol (`F3 + N`, `F3 + I`, `F3 + Q`).
- **Rendering.** The selection outline renderer became a `LineRenderer` with a
  per-edge colour and a capacity; a second instance (16384 edges) draws the
  `DebugLine`s of `F3 + B` and `F3 + G`. Lines are hidden by terrain, as the
  selection outline is. `debug_lines.rs` builds them as pure functions.
- **Hitbox** is the body size (`combat::hitbox::size_of`, falling back to
  `EntityKind::width/height`) without the 0.1 targeting border; the red eye
  line and the blue look vector of vanilla are left out.
- **Advanced tooltips** use a process-wide flag in `tooltip_draw`, because the
  screens that draw item tooltips only get scene snapshots.

## Consequences

`F3` toggles the text screen when it is released, and only if no chord was used
while it was held (later vanilla versions; 1.8.9 toggled on press, so a chord
flashed the screen).
