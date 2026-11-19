# 0083. Sign text colours and the sign editor

Status: accepted

## Context

Signs drew their text flattened to plain black, the editor showed a flat
coloured board, and the sign table was never pruned. Vanilla draws the lines as
formatted text (`TileEntitySignRenderer`, `FontRenderer.renderStringAtPos`)
and the editor draws the sign model facing the screen (`GuiEditSign`).

## Decision

- **Colour in the light word.** Sign glyphs stay in the entity pass with the
  other tile entities. The sign font texture is white with the glyph coverage
  as alpha; each vertex carries a chat colour index in bits 14..=17 of the
  light word, flagged by bits 13 (dye) and 18 (orb) together. No other user
  sets both, so the dye and orb meanings are unchanged and the vertex format
  does not grow. The palette is `FontRenderer.colorCode` (`CHAT_COLORS` in
  `entity.wgsl`).
- **Styles in world space.** `block_entity_render::sign_text` lays a line out
  as quads: bold twice one pixel apart, italic sheared one pixel each way,
  underline and strikethrough as bars on the atlas's opaque texel,
  obfuscated glyphs re-picked every tick.
- **Editor drawn flat.** The orthographic front view of the sign model shows
  only the front faces of the board and post, so the editor draws those two
  rectangles of `entity/sign.png` (now in the GUI atlas, in a 64 x 32 space)
  at the positions derived from the reference transforms (`SignLayout`).
- **Live text.** Each change in the editor sends `GameEvent::SignTyped`, which
  sets the local sign lines without a packet, as the reference edits the tile
  entity in place. The server's `Update Sign` still overrides them.
- **Lifetime.** Sign and block entity data are removed when the block becomes
  another block or the column is unloaded or replaced.

## Consequences

Italic text in the editor is upright: the UI draw list has no sheared glyphs.
A future third user of bits 13 + 18 must pick another flag.
