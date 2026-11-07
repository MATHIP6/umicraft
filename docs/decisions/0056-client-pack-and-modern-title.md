# 0056. Client pack and modern title screen

Status: accepted

## Context

Umicraft starts to ship features of its own, beyond 1.8 parity. The first one is
a modern title screen in the style of PvP clients, with the Umicraft logo, an
original panorama and a sans-serif font. The classic vanilla title screen must
stay available behind an option.

Until now the client had no place for its own assets. The few `umicraft.*`
texts live in the `BUILTIN` fallback table of `umicraft-core/src/lang.rs`, next
to vanilla fallbacks. The UI renderer has one bitmap font slot and no rounded
shapes.

## Decision

### Client pack

Umicraft's own assets live in the `umicraft` namespace, committed under
`crates/umicraft-assets/client-pack/umicraft/` and embedded at build time:

```
client-pack/umicraft/
  SOURCES.md                       provenance and license of every file
  lang/<code>.lang                 umicraft.* keys only
  textures/ui/logo.png             the Umicraft logo (owner's art)
  textures/ui/panorama/panorama_0..5.png
  font/Inter-SemiBold.ttf, OFL.txt
```

`umicraft_assets::client_pack` is the only module that names these files (one
`include_bytes!`/`include_str!` table). It exposes:

```rust
pub fn lang(code: &str) -> Option<&'static str>;
pub fn logo_png() -> &'static [u8];
pub fn panorama_png(face: usize) -> Option<&'static [u8]>;
pub fn brand_font_ttf() -> &'static [u8];
```

Language priority, lowest first: `LangTable::builtin` → vanilla `en_US` →
vanilla selected → Umicraft `en_US` → Umicraft selected. The `umicraft.*`
entries move out of `BUILTIN` into `lang/en_US.lang` (the built-in table keeps
the vanilla offline fallbacks only). `Lang::from_texts` gains the two client
texts as a separate layer.

Every binary file of the pack is listed in the `assets-guard` allowlist with
its license.

### Panorama waiver (owner decision, 2026-11-06)

The panorama faces come from the Bedrock resource pack "More Panoramas —
Moonlight Landscape" by TheLake. They are a third-party capture, not a Mojang
file, but the scene shows Minecraft-style blocks. Spec §3 rule 3 asks for
original assets only. The project owner explicitly waives that rule **for these
six images only** and takes responsibility for the permission to redistribute
them. The faces are downscaled from 1080 to 1024 pixels to keep the binary
smaller. Before a public release (spec §3 rule 8) the owner records the
author's permission and license in `client-pack/umicraft/SOURCES.md`, or
replaces the images with an original panorama. No other asset is covered by
this waiver.

### Fonts

The UI keeps the game bitmap font for every vanilla screen. A second face,
Inter SemiBold (SIL OFL 1.1), is rasterized at runtime with `fontdue`
(MIT OR Apache-2.0 OR Zlib) into a second `FontAtlas`:

```rust
// umicraft-ui::draw
pub enum FontFace { Game, Brand }      // Default: Game
pub struct TextStyle { scale, color, rotation, pub face: FontFace }
impl TextStyle { pub const fn with_face(self, face: FontFace) -> Self; }
```

The UI pass batches text per face. The brand atlas is sampled with linear
filtering. Measuring brand text goes through the brand `FontAtlas`, which the
caller passes like the game atlas.

### Rounded shapes

`DrawCommand::RoundedRect { rect, radius, color }` is tessellated on the CPU
into triangles that use the font atlas white texel, with a one-pixel alpha
fringe for anti-aliasing. No shader changes.

### Modern widgets

`umicraft-ui/src/modern/` holds reusable pieces for Umicraft's own screens:
`theme` (colour, radius, spacing and opacity tokens), `button` (translucent
rounded button with an animated hover), `panel` (translucent panel, vignette)
and `icons` (original icons built from primitives: gear, power).

### Title screen

`menus/title.rs` becomes `menus/title/`:

- `mod.rs`: `TitleStyle { Modern, Classic }` (default `Modern`),
  `TitleOutcome`, and the `TitleScreen` façade that dispatches on the style.
- `classic.rs`: the vanilla screen, unchanged.
- `modern/`: `layout.rs` (pure layout), `wordmark.rs` (logo and "UMICRAFT"),
  `mod.rs` (`show`). The modern screen has two buttons, **Play** (opens the
  server list) and **Options**, and a small power icon to quit.

The style is stored in `options.toml` as `[menu] title_style = "modern"` and
switched from the main options page. While no brand atlas is bound (the font
failed to rasterize or upload) the window shows the classic screen.

The brand images enter the GUI atlas as dynamic images, so their regions are
`dynamic/umicraft/{logo,power,settings}`. `GuiAtlas::with_dynamic` replaces
earlier dynamic images, so every atlas growth goes through
`official::grow_atlas`.

## Consequences

- Umicraft-specific texts and art have one home and one access point. Adding a
  language is adding one `.lang` file.
- The binary grows by about 5.5 MB (panorama, font, logo).
- The renderer gains a second font texture and a second text batch kind.
- The panorama waiver is an open item for the public release checklist.
