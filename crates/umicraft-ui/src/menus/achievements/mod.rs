// SPDX-License-Identifier: MIT OR Apache-2.0

//! The achievements screen (MCP `GuiAchievements`): a 256 by 202 window with
//! a draggable, zoomable map of the achievement tree.
//!
//! The left button drags the map, the wheel zooms between 1 and 2 (a quarter
//! per notch around the window centre), and hovering an achievement shows its
//! name and description. Until the server answers the statistics request the
//! screen shows `multiplayer.downloadingStats`. Done sits at
//! `(cx + 24, cy + 74)`, 80 wide. Vanilla 1.8 has no parallax: the tiles
//! scroll with the map.
//!
//! Beyond vanilla the title bar also counts the unlocked achievements.

mod map;
mod model;
#[cfg(test)]
mod tests;
mod toast;

use umicraft_core::lang::Lang;

use self::map::{MapView, CELL, VIEW};
pub use self::model::{AchievementArt, AchievementNode, AchievementsData, MapTile, Progress};
pub use self::toast::{draw_toast, toast_offset, Toast, TOAST_DURATION};
use super::background::draw_menu_background;
use super::common::{escape_pressed, wrap_lines, Gui, TextPlacement, WHITE};
use crate::context::Ui;
use crate::draw::{Color, TextStyle};
use crate::input::{InputEvent, PointerButton};
use crate::text::FontAtlas;

/// The window texture size and the part of it that is the window.
const WINDOW: [f32; 2] = [256.0, 202.0];
/// Ticks per marker of the loading text, as on the statistics screen.
const TICKS_PER_MARKER: u32 = 3;
/// Where the map drag may start, relative to the window.
const DRAG_OFFSET: [f32; 2] = [8.0, 17.0];
const TITLE_COLOR: Color = [0.251, 0.251, 0.251, 1.0];
const TOOLTIP_BACKGROUND: Color = [0.0, 0.0, 0.0, 0.75];
const DESCRIPTION: Color = [0.627, 0.627, 0.627, 1.0];
const REQUIRES: Color = [0.439, 0.314, 0.314, 1.0];
const TAKEN: Color = [0.565, 0.565, 1.0, 1.0];

/// What the player did on the achievements screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AchievementsOutcome {
    /// Leave the screen (Done or Escape).
    Done,
}

/// The Done button for `viewport`.
#[must_use]
pub fn achievements_done_rect(viewport: crate::geom::Rect) -> crate::geom::Rect {
    let gui = Gui::new(viewport);
    gui.rect(
        gui.cx() + 24.0,
        (gui.height / 2.0).floor() + 74.0,
        80.0,
        20.0,
    )
}

/// The scroll, zoom and drag state of the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct AchievementsScreen {
    scroll: Option<[f32; 2]>,
    zoom: f32,
    drag: Option<[f32; 2]>,
    ticks: u32,
}

impl Default for AchievementsScreen {
    fn default() -> Self {
        Self {
            scroll: None,
            zoom: 1.0,
            drag: None,
            ticks: 0,
        }
    }
}

impl AchievementsScreen {
    /// Creates the screen centred on the first achievement.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The zoom factor, from 1 to 2.
    #[must_use]
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Advances the loading marker by one game tick.
    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
    }

    /// Draws one frame. `loading` is true until the server answered the
    /// request; `player_hash` seeds the map tiles (vanilla hashes the player
    /// id).
    pub fn show(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        data: &AchievementsData,
        (loading, player_hash): (bool, i32),
    ) -> Option<AchievementsOutcome> {
        draw_menu_background(ui);
        let gui = Gui::new(ui.viewport());
        if loading {
            draw_loading(ui, atlas, lang, &gui, self.ticks);
            return escape_pressed(ui).then_some(AchievementsOutcome::Done);
        }
        let window = [
            ((gui.width - WINDOW[0]) / 2.0).trunc(),
            ((gui.height - WINDOW[1]) / 2.0).trunc(),
        ];
        self.handle_input(ui, &gui, window, data);
        let scroll = self.scroll_position(data);
        let art = ui.menu_art().achievements;
        let view = MapView {
            gui: &gui,
            window,
            zoom: self.zoom,
            scroll: [scroll[0].floor() as i32, scroll[1].floor() as i32],
            data,
            art: art.as_ref(),
            player_hash,
        };
        let pointer = ui.pointer().map(|p| gui.to_gui(p));
        let hovered = view.draw(ui, atlas, pointer);
        draw_window(ui, &gui, window, art.as_ref());
        draw_title(ui, atlas, lang, &gui, window, data);
        let done = done_button(ui, atlas, lang);
        if let (Some(index), Some(pointer)) = (hovered, pointer) {
            draw_tooltip(ui, atlas, lang, &gui, pointer, (data, index));
        }
        (done || escape_pressed(ui)).then_some(AchievementsOutcome::Done)
    }

    /// The scroll position, centred on the first achievement the first time,
    /// and kept inside the map (`field_146565_w` clamping).
    fn scroll_position(&mut self, data: &AchievementsData) -> [f32; 2] {
        let (columns, rows) = data.extent();
        let low = [
            (columns[0] * CELL - 112) as f32,
            (rows[0] * CELL - 112) as f32,
        ];
        let high = [
            (columns[1] * CELL - 77) as f32,
            (rows[1] * CELL - 77) as f32,
        ];
        let start = data.nodes.first().map_or([0.0, 0.0], |first| {
            // `column * 24 - 141 / 2 - 12` and `row * 24 - 141 / 2`.
            [
                (first.column * CELL - 70 - 12) as f32,
                (first.row * CELL - 70) as f32,
            ]
        });
        let mut position = self.scroll.unwrap_or(start);
        for axis in 0..2 {
            position[axis] = position[axis].max(low[axis]);
            if position[axis] >= high[axis] {
                position[axis] = high[axis] - 1.0;
            }
        }
        self.scroll = Some(position);
        position
    }

    /// Applies the drag and the wheel of this frame.
    fn handle_input(&mut self, ui: &Ui, gui: &Gui, window: [f32; 2], data: &AchievementsData) {
        let start = self.scroll_position(data);
        let mut scroll = start;
        let region = [window[0] + DRAG_OFFSET[0], window[1] + DRAG_OFFSET[1]];
        let inside = |p: [f32; 2]| {
            p[0] >= region[0]
                && p[0] < region[0] + VIEW[0]
                && p[1] >= region[1]
                && p[1] < region[1] + VIEW[1]
        };
        for event in &ui.input().events {
            match *event {
                InputEvent::PointerPressed {
                    position,
                    button: PointerButton::Left,
                } => {
                    let point = gui.to_gui(position);
                    self.drag = inside(point).then_some(point);
                }
                InputEvent::PointerMoved { position } => {
                    let point = gui.to_gui(position);
                    if let (Some(last), true) = (self.drag, inside(point)) {
                        scroll[0] -= (point[0] - last[0]) * self.zoom;
                        scroll[1] -= (point[1] - last[1]) * self.zoom;
                        self.drag = Some(point);
                    }
                }
                InputEvent::PointerReleased { .. } => self.drag = None,
                InputEvent::Wheel { delta } if delta != 0.0 => {
                    scroll = self.zoomed(scroll, delta > 0.0);
                }
                _ => {}
            }
        }
        self.scroll = Some(scroll);
    }

    /// One wheel notch: zoom out for a downward notch, keeping the window
    /// centre fixed (`GuiAchievements.drawScreen`).
    fn zoomed(&mut self, scroll: [f32; 2], out: bool) -> [f32; 2] {
        let before = self.zoom;
        self.zoom = (before + if out { 0.25 } else { -0.25 }).clamp(1.0, 2.0);
        let change = self.zoom - before;
        [
            scroll[0] - change * WINDOW[0] * 0.5,
            scroll[1] - change * WINDOW[1] * 0.5,
        ]
    }
}

fn draw_loading(ui: &mut Ui, atlas: &FontAtlas, lang: &Lang, gui: &Gui, ticks: u32) {
    const MARKERS: [&str; 7] = [
        "oooooo", "Oooooo", "oOoooo", "ooOooo", "oooOoo", "ooooOo", "oooooO",
    ];
    let middle = (gui.height / 2.0).floor();
    let text = lang.tr("multiplayer.downloadingStats");
    gui.text(
        ui,
        atlas,
        text,
        TextPlacement::centered([gui.cx(), middle], WHITE),
    );
    let marker = MARKERS[(ticks / TICKS_PER_MARKER) as usize % MARKERS.len()];
    let at = [gui.cx(), middle + 18.0];
    gui.text(ui, atlas, marker, TextPlacement::centered(at, WHITE));
}

/// The window texture over the map (`drawTexturedModalRect(k, l, 0, 0, 256, 202)`).
fn draw_window(ui: &mut Ui, gui: &Gui, window: [f32; 2], art: Option<&AchievementArt>) {
    let dst = gui.rect(window[0], window[1], WINDOW[0], WINDOW[1]);
    match art {
        Some(art) => {
            let src = art.background.sub(0, 0, 256, 202);
            ui.draw().image(src, dst, [1.0; 4]);
        }
        None => ui.draw().rect(dst, [0.0, 0.0, 0.0, 0.5]),
    }
}

/// Text without the shadow, as `FontRenderer.drawString(text, x, y, color)`.
fn plain_text(ui: &mut Ui, gui: &Gui, text: &str, at: [f32; 2], color: Color) {
    let origin = gui.rect(at[0], at[1], 0.0, 0.0);
    ui.draw()
        .text([origin.x, origin.y], text, TextStyle::new(gui.scale, color));
}

fn draw_title(
    ui: &mut Ui,
    atlas: &FontAtlas,
    lang: &Lang,
    gui: &Gui,
    window: [f32; 2],
    data: &AchievementsData,
) {
    let y = window[1] + 5.0;
    plain_text(
        ui,
        gui,
        lang.tr("gui.achievements"),
        [window[0] + 15.0, y],
        TITLE_COLOR,
    );
    let count = format!("{}/{}", data.unlocked_count(), data.nodes.len());
    let width = atlas.measure(&count, gui.scale) / gui.scale;
    let x = window[0] + WINDOW[0] - 15.0 - width;
    plain_text(ui, gui, &count, [x, y], TITLE_COLOR);
}

fn done_button(ui: &mut Ui, atlas: &FontAtlas, lang: &Lang) -> bool {
    let viewport = ui.viewport();
    let gui = Gui::new(viewport);
    ui.push_id("achievements");
    let id = ui.id("done");
    let rect = achievements_done_rect(viewport);
    let done = gui.button(ui, id, rect, atlas, lang.tr("gui.done"), true);
    ui.pop_id();
    done
}

/// The box next to the pointer (`GuiAchievements.drawAchievementScreen`).
fn draw_tooltip(
    ui: &mut Ui,
    atlas: &FontAtlas,
    lang: &Lang,
    gui: &Gui,
    pointer: [f32; 2],
    (data, index): (&AchievementsData, usize),
) {
    let node = &data.nodes[index];
    let progress = data.progress(index);
    let available = matches!(progress, Progress::Unlocked | Progress::Available);
    let [x, y] = [pointer[0] + 12.0, pointer[1] - 4.0];
    let name_width = |text: &str| (atlas.measure(text, gui.scale) / gui.scale).max(120.0);
    let lines = |text: &str, width: f32| wrap_lines(atlas, text, width * gui.scale, gui.scale);
    let (title, width, body, body_color, extra) = if available {
        let width = name_width(&node.name);
        let body = lines(&node.description, width);
        (
            node.name.clone(),
            width,
            body,
            DESCRIPTION,
            progress == Progress::Unlocked,
        )
    } else if matches!(progress, Progress::Locked(depth) if depth <= 3) {
        let parent = node.parent.map_or("", |p| data.nodes[p].name.as_str());
        let requires = lang.tr_args("achievement.requires", &[parent]);
        let shown = if progress == Progress::Locked(3) {
            lang.tr("achievement.unknown").to_string()
        } else {
            node.name.clone()
        };
        let width = name_width(&shown);
        (shown, width, lines(&requires, width), REQUIRES, false)
    } else {
        return;
    };
    let body_height = body.len() as f32 * 9.0 + if extra { 12.0 } else { 0.0 };
    let background = gui.rect(x - 3.0, y - 3.0, width + 6.0, body_height + 18.0);
    ui.draw().rect(background, TOOLTIP_BACKGROUND);
    for (row, line) in body.iter().enumerate() {
        plain_text(ui, gui, line, [x, y + 12.0 + row as f32 * 9.0], body_color);
    }
    if extra {
        let taken = lang.tr("achievement.taken");
        gui.text(
            ui,
            atlas,
            taken,
            TextPlacement::left([x, y + body_height + 4.0], TAKEN),
        );
    }
    gui.text(
        ui,
        atlas,
        &title,
        TextPlacement::left([x, y], name_color(available, node.special)),
    );
}

/// The colour of the name: bright when it can be unlocked, yellowish for the
/// special ones.
fn name_color(available: bool, special: bool) -> Color {
    match (available, special) {
        (true, true) => [1.0, 1.0, 0.502, 1.0],
        (true, false) => WHITE,
        (false, true) => [0.502, 0.502, 0.251, 1.0],
        (false, false) => [0.502, 0.502, 0.502, 1.0],
    }
}
