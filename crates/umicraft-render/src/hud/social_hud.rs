// SPDX-License-Identifier: MIT OR Apache-2.0

//! The overlays of M10 (GAM-7): the scoreboard sidebar, the title and
//! subtitle, the action bar and the tab list.
//!
//! The geometry follows MCP `GuiIngame.renderScoreboard`, the title and
//! overlay-message parts of `GuiIngame.renderGameOverlay` and
//! `GuiPlayerTabOverlay.renderPlayerlist`. Like the rest of the HUD it is
//! pure geometry drawn with the built-in font: the text is plain (the styled
//! text of the chat work replaces it later) and the player heads, which 1.8
//! only draws on encrypted connections, are left out.

use crate::font;
use crate::hud::{HudLayout, HudState, SidebarView, TabListView, TitleView};
use crate::overlay::Overlay;

/// A tab-list column holds at most this many rows before a column is added.
const TAB_MAX_ROWS: usize = 20;
/// The tab list shows at most this many players.
const TAB_MAX_ENTRIES: usize = 80;
/// Width reserved at the right of a tab-list cell for the ping bars, plus the
/// gap (`+ 13` in the reference).
const TAB_CELL_EXTRA: f32 = 13.0;
/// Width of the ping bars.
const PING_WIDTH: f32 = 10.0;
/// Gap between two tab-list columns.
const TAB_COLUMN_GAP: f32 = 5.0;
/// Space kept free at the sides of the tab list.
const TAB_SIDE_MARGIN: f32 = 50.0;
/// Distance between the top of the screen and the tab list.
const TAB_TOP: f32 = 10.0;
/// Distance between the sidebar and the right edge of the screen.
const SIDEBAR_MARGIN: f32 = 3.0;

/// Dark panel behind the tab-list header, footer and rows (`0x80000000`).
const PANEL: [f32; 4] = [0.0, 0.0, 0.0, 0.5];
/// Light cell behind one tab-list entry (`0x20FFFFFF`).
const CELL: [f32; 4] = [1.0, 1.0, 1.0, 0.125];
/// Row behind a sidebar line (`0x50000000`).
const SIDEBAR_ROW: [f32; 4] = [0.0, 0.0, 0.0, 0.31];
/// Title row of the sidebar (`0x60000000`).
const SIDEBAR_TITLE: [f32; 4] = [0.0, 0.0, 0.0, 0.376];
/// Plain text.
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
/// A spectator's name in the tab list (`0x91FFFFFF`).
const SPECTATOR_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 0.57];
/// The score of a sidebar line (`§c`).
const SCORE_RED: [f32; 4] = [1.0, 0.33, 0.33, 1.0];
/// An unlit ping bar.
const PING_OFF: [f32; 4] = [0.25, 0.25, 0.25, 1.0];
/// The game mode id of spectators.
const SPECTATOR: i32 = 3;
/// Heights of the five ping bars, in unscaled pixels.
const PING_BAR_HEIGHTS: [f32; 5] = [2.0, 3.0, 5.0, 6.0, 8.0];

/// The columns and rows of a tab list with `count` players: one column until
/// it would exceed [`TAB_MAX_ROWS`] rows, then as many as needed
/// (`GuiPlayerTabOverlay`: `for (j4 = 1; i4 > 20; i4 = (l3 + j4 - 1) / j4)`).
#[must_use]
pub fn tab_grid(count: usize) -> (usize, usize) {
    let mut columns = 1;
    let mut rows = count;
    while rows > TAB_MAX_ROWS {
        columns += 1;
        rows = count.div_ceil(columns);
    }
    (columns, rows)
}

/// The placement of the tab-list columns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabGeometry {
    /// The number of columns.
    pub columns: usize,
    /// The number of rows of every column.
    pub rows: usize,
    /// The width of one column.
    pub column_width: f32,
    /// The left edge of the first column.
    pub left: f32,
    /// The width of all the columns and the gaps between them.
    pub width: f32,
}

/// Places the columns of a tab list of `count` players whose widest name is
/// `name_width` pixels, centred in a viewport `viewport_width` pixels wide.
#[must_use]
pub fn tab_geometry(count: usize, name_width: f32, viewport_width: f32, scale: f32) -> TabGeometry {
    let (columns, rows) = tab_grid(count);
    let wanted = columns as f32 * (name_width + TAB_CELL_EXTRA * scale);
    let available = (viewport_width - TAB_SIDE_MARGIN * scale).max(columns as f32);
    let column_width = wanted.min(available) / columns as f32;
    let gap = TAB_COLUMN_GAP * scale;
    let width = column_width * columns as f32 + gap * (columns as f32 - 1.0);
    TabGeometry {
        columns,
        rows,
        column_width,
        left: viewport_width * 0.5 - width * 0.5,
        width,
    }
}

/// How many of the five ping bars are lit, from the latency in milliseconds.
/// A negative latency (unknown) lights none (`drawPing`: 150 / 300 / 600 /
/// 1000 ms thresholds).
#[must_use]
pub fn lit_ping_bars(ping: i32) -> usize {
    match ping {
        i32::MIN..=-1 => 0,
        0..=149 => 5,
        150..=299 => 4,
        300..=599 => 3,
        600..=999 => 2,
        _ => 1,
    }
}

/// The colour of the lit ping bars: green while the connection is good, then
/// yellow, orange and red.
fn ping_color(lit: usize) -> [f32; 4] {
    match lit {
        4.. => [0.33, 0.85, 0.33, 1.0],
        3 => [0.9, 0.85, 0.25, 1.0],
        2 => [0.95, 0.55, 0.2, 1.0],
        _ => [0.9, 0.25, 0.25, 1.0],
    }
}

/// `text` without line breaks, cut to the characters that fit in
/// `max_width` pixels.
fn clip(text: &str, max_width: f32, scale: f32) -> String {
    let advance = font::CELL_WIDTH as f32 * scale;
    let max_chars = (max_width / advance).floor().max(0.0) as usize;
    text.chars()
        .filter(|character| *character != '\n')
        .take(max_chars)
        .collect()
}

/// Draws every overlay the state asks for.
pub(crate) fn draw(overlay: &mut Overlay, state: &HudState, layout: &HudLayout) {
    if let Some(sidebar) = &state.sidebar {
        draw_sidebar(overlay, sidebar, layout);
    }
    if let Some(title) = &state.title {
        draw_title(overlay, title, layout);
    }
    if let Some((text, alpha)) = &state.action_bar {
        draw_action_bar(overlay, text, *alpha, state.armor > 0.0, layout);
    }
    if state.show_tab_list {
        if let Some(tab_list) = &state.tab_list {
            draw_tab_list(overlay, tab_list, layout);
        }
    }
}

/// One line of text vertically centred in a row.
fn row_text(overlay: &mut Overlay, x: f32, row_y: f32, scale: f32, color: [f32; 4], text: &str) {
    let glyph_height = font::CELL_HEIGHT as f32 * scale;
    let row = 8.0 * scale;
    overlay.text(x, row_y + (row - glyph_height) * 0.5, scale, color, text);
}

fn draw_sidebar(overlay: &mut Overlay, sidebar: &SidebarView, layout: &HudLayout) {
    if sidebar.lines.is_empty() {
        return;
    }
    let scale = layout.scale();
    let row = layout.line_height();
    let margin = SIDEBAR_MARGIN * scale;
    let right = layout.width() - margin + 2.0 * scale;

    let mut inner = Overlay::text_width(&sidebar.title, scale);
    for (name, value) in &sidebar.lines {
        let label = Overlay::text_width(name, scale) + Overlay::text_width(": ", scale);
        inner = inner.max(label + Overlay::text_width(&value.to_string(), scale));
    }
    let left = layout.width() - inner - margin;
    let count = sidebar.lines.len();
    let base = layout.height() * 0.5 + count as f32 * row / 3.0;

    for (index, (name, value)) in sidebar.lines.iter().enumerate() {
        let y = base - (count - index) as f32 * row;
        overlay.rect(
            left - 2.0 * scale,
            y,
            right - left + 2.0 * scale,
            row,
            SIDEBAR_ROW,
        );
        row_text(overlay, left, y, scale, WHITE, &clip(name, inner, scale));
        let score = value.to_string();
        let score_width = Overlay::text_width(&score, scale);
        row_text(overlay, right - score_width, y, scale, SCORE_RED, &score);
    }

    let top = base - count as f32 * row;
    let title_width = Overlay::text_width(&sidebar.title, scale);
    overlay.rect(
        left - 2.0 * scale,
        top - row - scale,
        right - left + 2.0 * scale,
        row,
        SIDEBAR_TITLE,
    );
    overlay.rect(
        left - 2.0 * scale,
        top - scale,
        right - left + 2.0 * scale,
        scale,
        SIDEBAR_ROW,
    );
    row_text(
        overlay,
        left + inner * 0.5 - title_width * 0.5,
        top - row,
        scale,
        WHITE,
        &sidebar.title,
    );
}

fn draw_title(overlay: &mut Overlay, title: &TitleView, layout: &HudLayout) {
    let scale = layout.scale();
    let centre_x = layout.width() * 0.5;
    let centre_y = layout.height() * 0.5;
    let color = [1.0, 1.0, 1.0, title.alpha.clamp(0.0, 1.0)];
    let mut draw_centered = |text: &str, text_scale: f32, y: f32| {
        let text = clip(text, layout.width(), text_scale);
        let width = Overlay::text_width(&text, text_scale);
        overlay.text(centre_x - width * 0.5, y, text_scale, color, &text);
    };
    // The reference draws the title at 4x from -10 and the subtitle at 2x
    // from +5, in units of its own scaled space.
    draw_centered(&title.title, 4.0 * scale, centre_y - 40.0 * scale);
    draw_centered(&title.subtitle, 2.0 * scale, centre_y + 10.0 * scale);
}

fn draw_action_bar(
    overlay: &mut Overlay,
    text: &str,
    alpha: f32,
    armor_visible: bool,
    layout: &HudLayout,
) {
    let scale = layout.scale();
    let text = clip(text, layout.width() - 8.0 * scale, scale);
    let width = Overlay::text_width(&text, scale);
    let y = layout.content_top(armor_visible) - 2.0 * layout.line_height();
    overlay.text(
        layout.width() * 0.5 - width * 0.5,
        y,
        scale,
        [1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0)],
        &text,
    );
}

fn draw_tab_list(overlay: &mut Overlay, tab: &TabListView, layout: &HudLayout) {
    let scale = layout.scale();
    let row = layout.line_height();
    let count = tab.entries.len().min(TAB_MAX_ENTRIES);
    if count == 0 {
        return;
    }
    let entries = &tab.entries[..count];
    let name_width = entries
        .iter()
        .map(|entry| Overlay::text_width(&entry.name, scale))
        .fold(0.0_f32, f32::max);
    let geometry = tab_geometry(count, name_width, layout.width(), scale);
    let text_limit = layout.width() - TAB_SIDE_MARGIN * scale;

    let header: Vec<String> = tab
        .header
        .lines()
        .map(|line| clip(line, text_limit, scale))
        .collect();
    let footer: Vec<String> = tab
        .footer
        .lines()
        .map(|line| clip(line, text_limit, scale))
        .collect();
    let mut block_width = geometry.width;
    for line in header.iter().chain(&footer) {
        block_width = block_width.max(Overlay::text_width(line, scale));
    }
    let block_left = layout.width() * 0.5 - block_width * 0.5;

    let centered_block = |overlay: &mut Overlay, lines: &[String], top: f32| {
        overlay.rect(
            block_left - scale,
            top - scale,
            block_width + 2.0 * scale,
            lines.len() as f32 * row + scale,
            PANEL,
        );
        for (index, line) in lines.iter().enumerate() {
            let width = Overlay::text_width(line, scale);
            row_text(
                overlay,
                layout.width() * 0.5 - width * 0.5,
                top + index as f32 * row,
                scale,
                WHITE,
                line,
            );
        }
    };

    let mut y = TAB_TOP * scale;
    if !header.is_empty() {
        centered_block(overlay, &header, y);
        y += header.len() as f32 * row + scale;
    }
    overlay.rect(
        block_left - scale,
        y - scale,
        block_width + 2.0 * scale,
        geometry.rows as f32 * row + scale,
        PANEL,
    );
    for (index, entry) in entries.iter().enumerate() {
        let column = index / geometry.rows;
        let line = index % geometry.rows;
        let x = geometry.left + column as f32 * (geometry.column_width + TAB_COLUMN_GAP * scale);
        let cell_y = y + line as f32 * row;
        overlay.rect(x, cell_y, geometry.column_width, row - scale, CELL);
        let color = if entry.game_mode == SPECTATOR {
            SPECTATOR_TEXT
        } else {
            WHITE
        };
        let name_room = geometry.column_width - PING_WIDTH * scale;
        row_text(
            overlay,
            x,
            cell_y,
            scale,
            color,
            &clip(&entry.name, name_room, scale),
        );
        draw_ping(
            overlay,
            x + geometry.column_width,
            cell_y,
            entry.ping,
            scale,
        );
    }
    if !footer.is_empty() {
        let footer_top = y + geometry.rows as f32 * row + scale;
        centered_block(overlay, &footer, footer_top);
    }
}

/// Draws the five ping bars with their right edge at `right`.
fn draw_ping(overlay: &mut Overlay, right: f32, row_y: f32, ping: i32, scale: f32) {
    let lit = lit_ping_bars(ping);
    let bottom = row_y + 8.0 * scale - scale;
    let left = right - (PING_WIDTH + 1.0) * scale;
    let color = if ping < 0 {
        [0.9, 0.25, 0.25, 1.0]
    } else {
        ping_color(lit)
    };
    for (index, height) in PING_BAR_HEIGHTS.iter().enumerate() {
        let bar = if index < lit { color } else { PING_OFF };
        overlay.rect(
            left + index as f32 * 2.0 * scale,
            bottom - height * scale,
            1.5 * scale,
            height * scale,
            bar,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::TabEntryView;

    fn layout() -> HudLayout {
        HudLayout::new(1280, 720)
    }

    fn entry(name: &str, ping: i32) -> TabEntryView {
        TabEntryView {
            name: name.to_string(),
            ping,
            game_mode: 0,
        }
    }

    fn drawn(state: &HudState) -> u32 {
        let mut overlay = Overlay::new(1280, 720);
        draw(&mut overlay, state, &layout());
        overlay.vertex_count()
    }

    #[test]
    fn the_tab_list_adds_a_column_every_twenty_rows() {
        assert_eq!(tab_grid(0), (1, 0));
        assert_eq!(tab_grid(20), (1, 20));
        assert_eq!(tab_grid(21), (2, 11));
        assert_eq!(tab_grid(40), (2, 20));
        assert_eq!(tab_grid(41), (3, 14));
        assert_eq!(tab_grid(80), (4, 20));
    }

    #[test]
    fn the_columns_are_centred_and_stay_inside_the_viewport() {
        let geometry = tab_geometry(80, 200.0, 1280.0, 2.0);
        assert_eq!(geometry.columns, 4);
        assert!((geometry.left + geometry.width * 0.5 - 640.0).abs() < 0.01);
        assert!(geometry.left >= 0.0);
        assert!(geometry.left + geometry.width <= 1280.0);

        // A narrow viewport squeezes the columns instead of overflowing.
        let narrow = tab_geometry(80, 200.0, 300.0, 1.0);
        assert!(narrow.left >= 0.0);
        assert!(narrow.left + narrow.width <= 300.0);
    }

    #[test]
    fn the_ping_bars_follow_the_reference_thresholds() {
        assert_eq!(lit_ping_bars(-1), 0);
        assert_eq!(lit_ping_bars(0), 5);
        assert_eq!(lit_ping_bars(149), 5);
        assert_eq!(lit_ping_bars(150), 4);
        assert_eq!(lit_ping_bars(299), 4);
        assert_eq!(lit_ping_bars(300), 3);
        assert_eq!(lit_ping_bars(600), 2);
        assert_eq!(lit_ping_bars(999), 2);
        assert_eq!(lit_ping_bars(1000), 1);
        assert_eq!(lit_ping_bars(i32::MAX), 1);
    }

    #[test]
    fn nothing_is_drawn_without_an_overlay_to_show() {
        assert_eq!(drawn(&HudState::default()), 0);
    }

    #[test]
    fn the_tab_list_is_hidden_until_the_key_is_held() {
        let mut state = HudState {
            tab_list: Some(TabListView {
                header: "WELCOME".to_string(),
                footer: String::new(),
                entries: vec![entry("STEVE", 20), entry("ALEX", 400)],
            }),
            ..HudState::default()
        };
        assert_eq!(drawn(&state), 0);
        state.show_tab_list = true;
        assert!(drawn(&state) > 0);
    }

    #[test]
    fn the_sidebar_title_and_action_bar_each_draw_something() {
        let sidebar = HudState {
            sidebar: Some(SidebarView {
                title: "KILLS".to_string(),
                lines: vec![("ALEX".to_string(), 4), ("BOB".to_string(), 1)],
            }),
            ..HudState::default()
        };
        let title = HudState {
            title: Some(TitleView {
                title: "ROUND 1".to_string(),
                subtitle: "FIGHT".to_string(),
                alpha: 1.0,
            }),
            ..HudState::default()
        };
        let action_bar = HudState {
            action_bar: Some(("GO".to_string(), 0.5)),
            ..HudState::default()
        };
        assert!(drawn(&sidebar) > 0);
        assert!(drawn(&title) > 0);
        assert!(drawn(&action_bar) > 0);
    }

    #[test]
    fn an_empty_sidebar_draws_nothing() {
        let state = HudState {
            sidebar: Some(SidebarView {
                title: "T".to_string(),
                lines: vec![],
            }),
            ..HudState::default()
        };
        assert_eq!(drawn(&state), 0);
    }

    #[test]
    fn text_is_clipped_to_the_room_it_has() {
        assert_eq!(clip("HELLO WORLD", 4.0 * 5.0, 1.0), "HELLO");
        assert_eq!(clip("A\nB", 100.0, 1.0), "AB");
        assert_eq!(clip("ABC", 0.0, 1.0), "");
    }

    #[test]
    fn a_huge_tab_list_is_capped_and_stays_on_screen() {
        let entries = (0..200).map(|i| entry(&format!("PLAYER{i}"), 30)).collect();
        let state = HudState {
            show_tab_list: true,
            tab_list: Some(TabListView {
                header: String::new(),
                footer: String::new(),
                entries,
            }),
            ..HudState::default()
        };
        let mut overlay = Overlay::new(1280, 720);
        draw(&mut overlay, &state, &layout());
        assert!(overlay.vertex_count() > 0);
        for vertex in overlay.vertices() {
            // Vertices are in clip space, so everything on screen is in -1..=1.
            assert!(vertex.position[0].abs() <= 1.001, "x {:?}", vertex.position);
            assert!(vertex.position[1].abs() <= 1.001, "y {:?}", vertex.position);
        }
    }
}
