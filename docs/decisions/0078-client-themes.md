# 0078. Client themes

Status: accepted

## Context

The owner wants themes for the client: buttons, font and background. The default
is the Minecraft look, which the resource packs retexture. Two original themes
come from the owner's mock-ups. Until now the look was hardcoded in
`widgets::button`, `widgets::slider` and `menus::background`, with no seam for
a second one.

## Decision

`umicraft_ui::theme` holds a `Theme { widgets, background, font }` of plain
data, chosen by `ThemeId` (`Vanilla`, `Ember`, `Amethyst`) and stored in `Ui`
(`set_theme`). The set is closed, so skins are enums, not trait objects: no
allocation and static dispatch on the draw path.

- `WidgetSkin::Vanilla` is the former code path, untouched: `widgets.png` from
  the resource packs, or the grey fallback without a client jar. Only this
  theme reads resource packs.
- `WidgetSkin::Flat(FlatSkin)` draws rectangles with an optional border and
  bevel, sized in texels (`height / 20`, like vanilla). Ember and Amethyst are
  two `FlatSkin` values (`theme/palette.rs`, colours sampled from the mock-ups).
- `BackgroundSkin::Dirt | Gradient`: a gradient themes the whole menu; list
  bands repaint the matching slice of the same gradient.
- `FontSkin::Game | Hd`: the HD font of the client pack
  (`font/hd/{16,24,32}.png`), cell size picked from the GUI scale
  (`hd_cell_for_scale`). `FontAtlas::from_hd_png` keeps 1.8 metrics (advances in
  1.8 pixels, `Glyph::scale = 8 / cell`), so no layout code changes. The
  advances derived from the pixels equal the font's `ascii.properties`.

The widgets keep their behaviour (hit test, focus, click sound) and only
delegate painting. The choice is stored as `[menu] theme` in `options.toml`
(unknown value reads as vanilla) and picked on a Theme page of the options.
The window swaps the renderer's font atlas when the theme or the GUI scale
bucket changes (`window/font_sync.rs`).

## Consequences

- A theme changes the font of all UI text, HUD and chat included, because the
  renderer has one game font texture per frame.
- The HD grid covers the 1.8 `ascii.png` characters only. Everything else is
  drawn from the jar's Unicode pages, kept in `LoadedFont::unicode` and applied
  to the HD grid with `FontAtlas::with_unicode`, exactly as for the game font
  (those glyphs keep the vanilla look). Without a client jar there are no
  pages, and such characters show as `?`.
- `title_style` (layout of the title screen) and `theme` (look) are separate.
- Themes are code, not files. Loading them from a file would need serde on the
  skin types; nothing blocks it.
- The HD font files are the owner's (confirmed 2026-11-17), listed in
  `client-pack/umicraft/SOURCES.md` and the `assets-guard` allowlist.
