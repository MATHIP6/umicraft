// SPDX-License-Identifier: MIT OR Apache-2.0

//! The dirt backgrounds of the menu screens and the `GuiSlot` list chrome.
//!
//! Vanilla draws `options_background.png` tiled over the whole screen when no
//! world is behind a menu (`GuiScreen.drawBackground`, tint `0x404040`) and
//! inside a list as the darker `0x202020` container with a `0x404040` band
//! above and below and a 4 unit black shadow on each list edge
//! (`GuiSlot.drawScreen`, `overlayBackground`, `drawContainerBackground`).
//!
//! The texture is a 16 pixel square drawn 32 GUI units wide, so each texel is
//! two GUI units. The GUI atlas has no wrap mode, so the tiles are emitted as
//! separate [`crate::DrawCommand::Image`] commands, the ones at the border cut
//! with a proportional UV. Without the texture (no client jar) the same areas
//! are filled with the tint colour alone.
//!
//! Source: MCP `GuiScreen.drawBackground`, `GuiSlot.drawContainerBackground`
//! and `GuiSlot.overlayBackground` (tile size 32.0, UV `x / 32`).

use crate::context::Ui;
use crate::draw::{Color, SpriteSheet};
use crate::geom::Rect;
use crate::widgets::gui_scale;

/// Edge of one dirt tile in GUI units (`float f = 32.0F` in `drawBackground`).
const TILE_GUI: f32 = 32.0;
/// The tint of a screen background and of the list bands (`0x404040`).
const BAND_TINT: Color = [0.25, 0.25, 0.25, 1.0];
/// The tint of the list container (`0x202020`).
const LIST_TINT: Color = [0.125, 0.125, 0.125, 1.0];
/// Height of the black shadow on each list edge, in GUI units.
const SHADOW_GUI: u32 = 4;

/// The textures of the user's client jar the menus draw as sprites.
///
/// They are Mojang assets read at runtime from the user's own download and
/// never stored in the repository (spec §3, `docs/sources.md`). `None`
/// selects the original fallback: flat tint for the dirt, the text logo.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MenuArt {
    /// `textures/gui/options_background.png` (16x16).
    pub dirt: Option<SpriteSheet>,
    /// `textures/gui/title/minecraft.png` (256x256), the title logo.
    pub logo: Option<SpriteSheet>,
    /// `textures/gui/container/stats_icons.png` (128x128), the sprites of the
    /// statistics tables.
    pub stats_icons: Option<SpriteSheet>,
    /// The achievement screen's background texture and map tiles.
    pub achievements: Option<super::achievements::AchievementArt>,
    /// `textures/gui/icons.png` (256x256): the latency cells of the server list.
    pub icons: Option<SpriteSheet>,
    /// `textures/misc/unknown_server.png` (64x64): the icon of a server
    /// without a favicon.
    pub unknown_server: Option<SpriteSheet>,
    /// `textures/gui/server_selection.png` (256x256): the arrows of a hovered
    /// server row.
    pub server_selection: Option<SpriteSheet>,
}

/// Fills `rect` with the dirt texture tinted by `tint`.
///
/// The tile grid starts at `origin` (physical pixels) and each tile is
/// `tile` pixels wide; only the part of a tile inside `rect` is drawn, with
/// its UV cut to match. Without the dirt texture `rect` is one flat fill of
/// `tint`.
pub fn draw_dirt(ui: &mut Ui, rect: Rect, origin: [f32; 2], tile: f32, tint: Color) {
    let Some(sheet) = ui.menu_art().dirt else {
        ui.draw().rect(rect, tint);
        return;
    };
    if rect.is_empty() || tile <= 0.0 {
        return;
    }
    let whole = sheet.sub(0, 0, sheet.width, sheet.height);
    let first_col = ((rect.x - origin[0]) / tile).floor();
    let first_row = ((rect.y - origin[1]) / tile).floor();
    let mut y = origin[1] + first_row * tile;
    while y < rect.bottom() {
        let mut x = origin[0] + first_col * tile;
        while x < rect.right() {
            let cell = Rect::new(x, y, tile, tile);
            if let Some(visible) = cell.intersect(rect) {
                let src = cut_uv(whole, cell, visible);
                ui.draw().image(src, visible, tint);
            }
            x += tile;
        }
        y += tile;
    }
}

/// The UV of the part `visible` of a tile `cell` whose full UV is `whole`.
fn cut_uv(whole: [f32; 4], cell: Rect, visible: Rect) -> [f32; 4] {
    let lerp = |from: f32, to: f32, t: f32| from + (to - from) * t;
    let fraction = |value: f32, start: f32| (value - start) / cell.width;
    [
        lerp(whole[0], whole[2], fraction(visible.x, cell.x)),
        lerp(whole[1], whole[3], fraction(visible.y, cell.y)),
        lerp(whole[0], whole[2], fraction(visible.right(), cell.x)),
        lerp(whole[1], whole[3], fraction(visible.bottom(), cell.y)),
    ]
}

/// The tile edge in pixels at the GUI scale of `ui`'s viewport.
fn tile_pixels(ui: &Ui) -> (f32, f32) {
    let viewport = ui.viewport();
    let scale = gui_scale(viewport.width as u32, viewport.height as u32) as f32;
    (scale, TILE_GUI * scale)
}

/// Draws the screen background of a menu with no world behind it
/// (`GuiScreen.drawBackground`): the dirt over the whole viewport.
pub fn draw_menu_background(ui: &mut Ui) {
    let viewport = ui.viewport();
    let (_, tile) = tile_pixels(ui);
    draw_dirt(ui, viewport, [viewport.x, viewport.y], tile, BAND_TINT);
}

/// Draws the container of a `GuiSlot` list (`drawContainerBackground`).
///
/// `list` is the list rectangle in pixels and `scroll` the scroll offset in
/// pixels: the dirt moves with the content, as it does in vanilla.
pub fn draw_list_background(ui: &mut Ui, list: Rect, scroll: f32) {
    let (_, tile) = tile_pixels(ui);
    draw_dirt(ui, list, [0.0, -scroll], tile, LIST_TINT);
}

/// Draws what `GuiSlot` paints over the rows: the bands above and below the
/// list (`overlayBackground`) and the black shadows on its two edges.
///
/// Call it after the rows so the bands cover the rows that overflow.
pub fn draw_list_overlay(ui: &mut Ui, list: Rect) {
    let viewport = ui.viewport();
    let (scale, tile) = tile_pixels(ui);
    let origin = [viewport.x, viewport.y];
    let top = Rect::new(viewport.x, viewport.y, viewport.width, list.y - viewport.y);
    let bottom = Rect::new(
        viewport.x,
        list.bottom(),
        viewport.width,
        viewport.bottom() - list.bottom(),
    );
    draw_dirt(ui, top, origin, tile, BAND_TINT);
    draw_dirt(ui, bottom, origin, tile, BAND_TINT);
    draw_shadows(ui, list, scale);
}

/// The 4 unit gradients from opaque black to transparent below the top edge
/// and above the bottom edge of `list`, one band per GUI unit.
fn draw_shadows(ui: &mut Ui, list: Rect, scale: f32) {
    for step in 0..SHADOW_GUI {
        let alpha = 1.0 - (step as f32 + 0.5) / SHADOW_GUI as f32;
        let color = [0.0, 0.0, 0.0, alpha];
        let offset = step as f32 * scale;
        ui.draw()
            .rect(Rect::new(list.x, list.y + offset, list.width, scale), color);
        ui.draw().rect(
            Rect::new(list.x, list.bottom() - offset - scale, list.width, scale),
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::DrawCommand;
    use crate::input::UiInput;
    use crate::menus::common::testing::frame;

    const DIRT: SpriteSheet = SpriteSheet::new(0.5, 0.0, 0.75, 0.25, 16, 16);

    fn images(art: MenuArt, f: impl FnOnce(&mut Ui)) -> Vec<([f32; 4], Rect, Color)> {
        frame(UiInput::default(), |ui| {
            ui.set_menu_art(art);
            f(ui);
            ui.draw()
                .commands()
                .iter()
                .filter_map(|command| match command {
                    DrawCommand::Image { src, dst, color } => Some((*src, *dst, *color)),
                    _ => None,
                })
                .collect()
        })
    }

    fn with_dirt() -> MenuArt {
        MenuArt {
            dirt: Some(DIRT),
            logo: None,
            ..MenuArt::default()
        }
    }

    #[test]
    fn the_dirt_repeats_every_thirty_two_gui_units() {
        // Scale 3: a tile is 96 px, 1280x720 needs 14 columns and 8 rows.
        let tiles = images(with_dirt(), draw_menu_background);
        assert_eq!(tiles.len(), 14 * 8);
        let (src, dst, color) = tiles[0];
        assert_eq!(dst, Rect::new(0.0, 0.0, 96.0, 96.0));
        assert_eq!(src, [0.5, 0.0, 0.75, 0.25]);
        assert_eq!(color, BAND_TINT);
    }

    #[test]
    fn a_tile_cut_by_the_screen_edge_cuts_its_uv() {
        let tiles = images(with_dirt(), draw_menu_background);
        // 1280 = 13 * 96 + 32: the last column keeps a third of the texture.
        let (src, dst, _) = tiles[13];
        assert_eq!(dst.width, 32.0);
        assert!((src[2] - (0.5 + 0.25 / 3.0)).abs() < 1e-6);
        // 720 = 7 * 96 + 48: the last row keeps half of it.
        let (src, dst, _) = tiles[7 * 14];
        assert_eq!(dst.height, 48.0);
        assert!((src[3] - 0.125).abs() < 1e-6);
    }

    #[test]
    fn the_list_dirt_scrolls_with_the_content() {
        let list = Rect::new(0.0, 96.0, 192.0, 96.0);
        let tiles = images(with_dirt(), |ui| draw_list_background(ui, list, 48.0));
        // Shifted up by half a tile: the first row shows the lower half.
        let (src, dst, color) = tiles[0];
        assert_eq!(dst, Rect::new(0.0, 96.0, 96.0, 48.0));
        assert!((src[1] - 0.125).abs() < 1e-6);
        assert_eq!(color, LIST_TINT);
    }

    #[test]
    fn without_the_texture_the_areas_are_flat() {
        let art = MenuArt::default();
        let list = Rect::new(0.0, 96.0, 192.0, 96.0);
        let flat = frame(UiInput::default(), |ui| {
            ui.set_menu_art(art);
            draw_list_background(ui, list, 0.0);
            ui.draw().commands().to_vec()
        });
        assert_eq!(
            flat,
            [DrawCommand::Rect {
                rect: list,
                color: LIST_TINT
            }]
        );
    }

    #[test]
    fn the_overlay_covers_both_bands_and_shades_the_edges() {
        let list = Rect::new(0.0, 96.0, 1280.0, 480.0);
        let tiles = images(with_dirt(), |ui| draw_list_overlay(ui, list));
        let top_tiles = tiles.iter().filter(|(_, dst, _)| dst.y < 96.0).count();
        let bottom_tiles = tiles.iter().filter(|(_, dst, _)| dst.y >= 576.0).count();
        assert_eq!(top_tiles, 14);
        assert_eq!(bottom_tiles, 14 * 2);
        let shadow = frame(UiInput::default(), |ui| {
            ui.set_menu_art(MenuArt::default());
            draw_shadows(ui, list, 3.0);
            ui.draw().len()
        });
        assert_eq!(shadow, 8);
    }
}
