// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scrolling map of `GuiAchievements`: the random block tiles, the links
//! between achievements, the framed icons and the one under the pointer.
//!
//! Everything is laid out in "map units", 24 per achievement cell, and drawn
//! through one transform: GUI position = window origin + map position / zoom.
//! The tiles come from a `java.util.Random` seeded per tile row and column, so
//! the ground looks the same at every scroll position.

use umicraft_core::java_random::JavaRandom;

use super::model::{AchievementArt, AchievementsData, MapTile, Progress};
use crate::draw::Color;
use crate::geom::Rect;
use crate::menus::common::Gui;
use crate::text::FontAtlas;
use crate::widgets::draw_item;
use crate::Ui;

/// Map units per achievement cell.
pub(super) const CELL: i32 = 24;
/// The size of the map window in GUI units.
pub(super) const VIEW: [f32; 2] = [224.0, 155.0];
/// The edge of a tile in map units.
const TILE: i32 = 16;
/// Where the map draws from, relative to the window.
pub(super) const MAP_OFFSET: [f32; 2] = [16.0, 17.0];
/// The size of an achievement frame in the texture.
const FRAME: u32 = 26;
const UNLOCKED_LINK: Color = [0.627, 0.627, 0.627, 1.0];
const AVAILABLE_LINK: Color = [0.0, 1.0, 0.0, 1.0];
const LOCKED_LINK: Color = [0.0, 0.0, 0.0, 1.0];

/// The tile of a map cell. `row` is the tile row counted from the top of the
/// world, so the ground gets harder with depth and ends in bedrock.
pub(super) fn tile_at(player_hash: i32, column: i32, row: i32) -> MapTile {
    let seed = player_hash
        .wrapping_add(column)
        .wrapping_add(row.wrapping_mul(16));
    let mut random = JavaRandom::new(i64::from(seed) as u64);
    let depth = random.next_int(1 + row as u32) as i32 + row / 2;
    if depth > 37 || row == 35 {
        return MapTile::Bedrock;
    }
    match depth {
        22 if random.next_int(2) == 0 => MapTile::DiamondOre,
        22 => MapTile::RedstoneOre,
        10 => MapTile::IronOre,
        8 => MapTile::CoalOre,
        5.. => MapTile::Stone,
        1.. => MapTile::Dirt,
        _ => MapTile::Sand,
    }
}

/// The part of the screen the map is drawn on, and how it is seen.
pub(super) struct MapView<'a> {
    pub gui: &'a Gui,
    /// The window's top-left corner in GUI units.
    pub window: [f32; 2],
    pub zoom: f32,
    /// The map position at the top-left of the view, in map units.
    pub scroll: [i32; 2],
    pub data: &'a AchievementsData,
    pub art: Option<&'a AchievementArt>,
    pub player_hash: i32,
}

impl MapView<'_> {
    fn origin(&self) -> [f32; 2] {
        [
            self.window[0] + MAP_OFFSET[0],
            self.window[1] + MAP_OFFSET[1],
        ]
    }

    /// A rectangle of map units, as physical pixels.
    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        let [ox, oy] = self.origin();
        self.gui.rect(
            ox + x / self.zoom,
            oy + y / self.zoom,
            w / self.zoom,
            h / self.zoom,
        )
    }

    /// The map position under GUI point `point` (`f3` and `f4`).
    pub(super) fn to_map(&self, point: [f32; 2]) -> [f32; 2] {
        let [ox, oy] = self.origin();
        [(point[0] - ox) * self.zoom, (point[1] - oy) * self.zoom]
    }

    /// Draws the map clipped to its window and returns the achievement under
    /// `pointer` (GUI units), if any.
    pub(super) fn draw(
        &self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        pointer: Option<[f32; 2]>,
    ) -> Option<usize> {
        let [ox, oy] = self.origin();
        ui.push_clip(self.gui.rect(ox, oy, VIEW[0], VIEW[1]));
        self.draw_tiles(ui);
        self.draw_links(ui);
        let hovered = self.draw_nodes(ui, atlas, pointer.map(|p| self.to_map(p)));
        ui.pop_clip();
        hovered
    }

    fn draw_tiles(&self, ui: &mut Ui) {
        let [scroll_x, scroll_y] = self.scroll;
        let (first_column, offset_x) = ((scroll_x + 288) >> 4, (scroll_x + 288) % TILE);
        let (first_row, offset_y) = ((scroll_y + 288) >> 4, (scroll_y + 288) % TILE);
        let step = TILE as f32 / self.zoom;
        let mut y = 0;
        while (y as f32) * step - (offset_y as f32) < VIEW[1] {
            let row = first_row + y;
            let shade = 0.6 - row as f32 / 25.0 * 0.3;
            let mut x = 0;
            while (x as f32) * step - (offset_x as f32) < VIEW[0] {
                let tile = tile_at(self.player_hash, first_column + x, row);
                let at = [(x * TILE - offset_x) as f32, (y * TILE - offset_y) as f32];
                self.draw_tile(ui, tile, at, shade);
                x += 1;
            }
            y += 1;
        }
    }

    fn draw_tile(&self, ui: &mut Ui, tile: MapTile, at: [f32; 2], shade: f32) {
        let dst = self.rect(at[0], at[1], TILE as f32, TILE as f32);
        let tint = [shade, shade, shade, 1.0];
        match self.art {
            Some(art) => {
                let sheet = art.tiles[tile as usize];
                ui.draw()
                    .image(sheet.sub(0, 0, sheet.width, sheet.height), dst, tint);
            }
            None => ui.draw().rect(dst, tint),
        }
    }

    /// The links from each achievement to its parent, with their arrow.
    fn draw_links(&self, ui: &mut Ui) {
        for (index, node) in self.data.nodes.iter().enumerate() {
            let Some(parent) = node.parent.map(|p| &self.data.nodes[p]) else {
                continue;
            };
            let progress = self.data.progress(index);
            if matches!(progress, Progress::Locked(depth) if depth > 4) {
                continue;
            }
            let color = match progress {
                Progress::Unlocked => UNLOCKED_LINK,
                Progress::Available => AVAILABLE_LINK,
                Progress::Locked(_) => LOCKED_LINK,
            };
            let from = self.cell_centre(node.column, node.row);
            let to = self.cell_centre(parent.column, parent.row);
            self.line(
                ui,
                [from[0].min(to[0]), from[1]],
                (from[0] - to[0]).abs(),
                true,
                color,
            );
            self.line(
                ui,
                [to[0], from[1].min(to[1])],
                (from[1] - to[1]).abs(),
                false,
                color,
            );
            self.draw_arrow(ui, from, to);
        }
    }

    /// The centre of a cell in map units (`column * 24 - i + 11`).
    fn cell_centre(&self, column: i32, row: i32) -> [f32; 2] {
        [
            (column * CELL - self.scroll[0] + 11) as f32,
            (row * CELL - self.scroll[1] + 11) as f32,
        ]
    }

    /// A one unit line of `length` map units from `at`.
    fn line(&self, ui: &mut Ui, at: [f32; 2], length: f32, horizontal: bool, color: Color) {
        let (w, h) = if horizontal {
            (length + 1.0, 1.0)
        } else {
            (1.0, length + 1.0)
        };
        ui.draw().rect(self.rect(at[0], at[1], w, h), color);
    }

    /// The arrow where a link meets its child.
    fn draw_arrow(&self, ui: &mut Ui, from: [f32; 2], to: [f32; 2]) {
        let Some(sheet) = self.art.map(|art| art.background) else {
            return;
        };
        // (u, v, width, height, x offset, y offset) from the child's centre.
        let (u, v, w, h, dx, dy) = if from[0] > to[0] {
            (114, 234, 7, 11, -11.0 - 7.0, -5.0)
        } else if from[0] < to[0] {
            (107, 234, 7, 11, 11.0, -5.0)
        } else if from[1] > to[1] {
            (96, 234, 11, 7, -5.0, -11.0 - 7.0)
        } else if from[1] < to[1] {
            (96, 241, 11, 7, -5.0, 11.0)
        } else {
            return;
        };
        let dst = self.rect(from[0] + dx, from[1] + dy, w as f32, h as f32);
        ui.draw().image(sheet.sub(u, v, w, h), dst, [1.0; 4]);
    }

    /// The frames and icons; returns the node the pointer is over.
    fn draw_nodes(
        &self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        pointer: Option<[f32; 2]>,
    ) -> Option<usize> {
        let mut hovered = None;
        for (index, node) in self.data.nodes.iter().enumerate() {
            let x = (node.column * CELL - self.scroll[0]) as f32;
            let y = (node.row * CELL - self.scroll[1]) as f32;
            if x < -24.0 || y < -24.0 || x > VIEW[0] * self.zoom || y > VIEW[1] * self.zoom {
                continue;
            }
            let progress = self.data.progress(index);
            let Some(shade) = frame_shade(progress) else {
                continue;
            };
            self.draw_frame(ui, node.special, [x, y], shade);
            // Vanilla draws the icon of a locked achievement as a black
            // silhouette; here it is left out (docs/parity.md).
            if matches!(progress, Progress::Unlocked | Progress::Available) {
                let cell = self.rect(x + 3.0, y + 3.0, 16.0, 16.0);
                draw_item(ui.draw(), atlas, cell, &node.icon);
            }
            if pointer
                .is_some_and(|p| p[0] >= x && p[0] <= x + 22.0 && p[1] >= y && p[1] <= y + 22.0)
            {
                hovered = Some(index);
            }
        }
        hovered
    }

    fn draw_frame(&self, ui: &mut Ui, special: bool, at: [f32; 2], shade: f32) {
        let dst = self.rect(at[0] - 2.0, at[1] - 2.0, FRAME as f32, FRAME as f32);
        let tint = [shade, shade, shade, 1.0];
        match self.art.map(|art| art.background) {
            Some(sheet) => {
                let u = if special { 26 } else { 0 };
                ui.draw().image(sheet.sub(u, 202, FRAME, FRAME), dst, tint);
            }
            None => ui.draw().rect(dst, tint),
        }
    }
}

/// How bright a node is drawn; `None` hides nodes more than four steps away.
fn frame_shade(progress: Progress) -> Option<f32> {
    match progress {
        Progress::Unlocked => Some(0.75),
        Progress::Available => Some(1.0),
        Progress::Locked(1..=2) => Some(0.3),
        Progress::Locked(3) => Some(0.2),
        Progress::Locked(4) => Some(0.1),
        Progress::Locked(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_are_deterministic_and_harder_with_depth() {
        assert_eq!(tile_at(7, 3, 40), tile_at(7, 3, 40));
        assert_eq!(tile_at(7, 3, 35), MapTile::Bedrock);
        let deep = (0..64)
            .filter(|&c| tile_at(1, c, 120) == MapTile::Bedrock)
            .count();
        assert_eq!(deep, 64, "far below the surface everything is bedrock");
    }

    #[test]
    fn the_surface_rows_are_mostly_sand_and_dirt() {
        let soft = (0..200)
            .map(|c| tile_at(5, c, 1))
            .filter(|t| matches!(t, MapTile::Sand | MapTile::Dirt))
            .count();
        assert_eq!(soft, 200);
    }

    #[test]
    fn nodes_fade_with_their_distance_from_the_unlocked_ones() {
        assert_eq!(frame_shade(Progress::Available), Some(1.0));
        assert_eq!(frame_shade(Progress::Locked(3)), Some(0.2));
        assert_eq!(frame_shade(Progress::Locked(5)), None);
    }
}
