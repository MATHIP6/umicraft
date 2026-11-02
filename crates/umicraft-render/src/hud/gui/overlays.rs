// SPDX-License-Identifier: MIT OR Apache-2.0

//! The overlays above the world: scoreboard sidebar, title, action bar and
//! the player tab list.

use umicraft_ui::{Color, DrawList, FontAtlas, Rect, SpriteSheet, TextStyle};

use crate::hud::social_hud::tab_geometry;
use crate::hud::{HudState, SidebarView, TabListView, TitleView};

/// Draws the sidebar, title, action bar and (while held) the tab list.
pub(super) fn draw_overlays(
    list: &mut DrawList,
    state: &HudState,
    gw: f32,
    gh: f32,
    sf: f32,
    font: &FontAtlas,
    icons: Option<&SpriteSheet>,
) {
    if let Some(sidebar) = &state.sidebar {
        draw_sidebar(list, sidebar, gw, gh, sf, font);
    }

    if let Some(title) = &state.title {
        draw_title(list, title, gw, gh, sf, font);
    }

    if let Some((text, alpha)) = &state.action_bar {
        draw_action_bar(list, text, *alpha, gh, gw, sf, font);
    }

    if state.show_tab_list {
        if let Some(tab_list) = &state.tab_list {
            draw_tab_list(list, tab_list, gw, sf, font, icons);
        }
    }
}

fn draw_sidebar(
    list: &mut DrawList,
    sidebar: &SidebarView,
    gw: f32,
    gh: f32,
    sf: f32,
    font: &FontAtlas,
) {
    if sidebar.lines.is_empty() {
        return;
    }
    let row_h = 9.0;
    // `renderScoreboard`: the bands end at `width - 3 + 2`.
    let right = gw - 1.0;

    let mut inner = font.measure(&sidebar.title, 1.0);
    for (name, score) in &sidebar.lines {
        let line_w = font.measure(name, 1.0)
            + font.measure(": ", 1.0)
            + font.measure(&score.to_string(), 1.0);
        inner = inner.max(line_w);
    }

    let left = gw - inner - 3.0;
    let count = sidebar.lines.len();
    let base = gh * 0.5 + count as f32 * row_h / 3.0;

    for (index, (name, value)) in sidebar.lines.iter().enumerate() {
        let y = base - (count - index) as f32 * row_h;
        list.rect(
            Rect::new(
                (left - 2.0) * sf,
                y * sf,
                (right - left + 2.0) * sf,
                row_h * sf,
            ),
            [0.0, 0.0, 0.0, 0.31],
        );
        list.text(
            [left * sf, y * sf],
            name,
            TextStyle::new(sf, [1.0, 1.0, 1.0, 1.0]),
        );
        let score_str = value.to_string();
        let score_w = font.measure(&score_str, 1.0);
        list.text(
            [(right - score_w) * sf, y * sf],
            &score_str,
            TextStyle::new(sf, [1.0, 0.33, 0.33, 1.0]),
        );
    }

    let top = base - count as f32 * row_h;
    let title_w = font.measure(&sidebar.title, 1.0);
    list.rect(
        Rect::new(
            (left - 2.0) * sf,
            (top - row_h - 1.0) * sf,
            (right - left + 2.0) * sf,
            row_h * sf,
        ),
        [0.0, 0.0, 0.0, 0.376],
    );
    // The 1-unit strip between the title band and the first row (`0x50000000`).
    list.rect(
        Rect::new(
            (left - 2.0) * sf,
            (top - 1.0) * sf,
            (right - left + 2.0) * sf,
            sf,
        ),
        [0.0, 0.0, 0.0, 0.3125],
    );
    list.text(
        [(left + (inner - title_w) * 0.5) * sf, (top - row_h) * sf],
        &sidebar.title,
        TextStyle::new(sf, [1.0, 1.0, 1.0, 1.0]),
    );
}

fn draw_title(list: &mut DrawList, title: &TitleView, gw: f32, gh: f32, sf: f32, font: &FontAtlas) {
    let alpha = title.alpha.clamp(0.0, 1.0);
    let color: Color = [1.0, 1.0, 1.0, alpha];
    // The shadow is one scaled pixel away: 4 units for the title, 2 for the subtitle.
    let shadow: Color = [0.247, 0.247, 0.247, alpha];

    if !title.title.is_empty() {
        let scale = 4.0;
        let w = font.measure(&title.title, scale);
        let x = (gw * 0.5 - w * 0.5) * sf;
        let y = (gh * 0.5 - 40.0) * sf;
        list.text(
            [x + 4.0 * sf, y + 4.0 * sf],
            &title.title,
            TextStyle::new(scale * sf, shadow),
        );
        list.text([x, y], &title.title, TextStyle::new(scale * sf, color));
    }

    if !title.subtitle.is_empty() {
        let scale = 2.0;
        let w = font.measure(&title.subtitle, scale);
        let x = (gw * 0.5 - w * 0.5) * sf;
        let y = (gh * 0.5 + 10.0) * sf;
        list.text(
            [x + 2.0 * sf, y + 2.0 * sf],
            &title.subtitle,
            TextStyle::new(scale * sf, shadow),
        );
        list.text([x, y], &title.subtitle, TextStyle::new(scale * sf, color));
    }
}

fn draw_action_bar(
    list: &mut DrawList,
    text: &str,
    alpha: f32,
    gh: f32,
    gw: f32,
    sf: f32,
    font: &FontAtlas,
) {
    let alpha = alpha.clamp(0.0, 1.0);
    let w = font.measure(text, 1.0);
    let x = (gw * 0.5 - w * 0.5) * sf;
    let y = (gh - 72.0) * sf;
    list.text([x, y], text, TextStyle::new(sf, [1.0, 1.0, 1.0, alpha]));
}

/// `drawStringWithShadow`: the shadow is one scaled pixel away, at a quarter
/// of the colour (`(c & 0xFCFCFC) >> 2`), with the alpha of the text.
fn shadowed_text(list: &mut DrawList, at: [f32; 2], text: &str, scale: f32, color: Color) {
    let shadow = [color[0] * 0.25, color[1] * 0.25, color[2] * 0.25, color[3]];
    list.text(
        [at[0] + scale, at[1] + scale],
        text,
        TextStyle::new(scale, shadow),
    );
    list.text(at, text, TextStyle::new(scale, color));
}

fn draw_tab_list(
    list: &mut DrawList,
    tab: &TabListView,
    gw: f32,
    sf: f32,
    font: &FontAtlas,
    icons: Option<&SpriteSheet>,
) {
    let count = tab.entries.len().min(80);
    if count == 0 {
        return;
    }
    let entries = &tab.entries[..count];
    let name_w = entries
        .iter()
        .map(|e| font.measure(&e.name, 1.0))
        .fold(0.0_f32, f32::max);

    let row_h = 9.0;
    let geometry = tab_geometry(count, name_w, gw, 1.0);

    let header_lines: Vec<&str> = tab.header.lines().collect();
    let footer_lines: Vec<&str> = tab.footer.lines().collect();

    let mut block_w = geometry.width;
    for &line in header_lines.iter().chain(&footer_lines) {
        block_w = block_w.max(font.measure(line, 1.0));
    }
    let block_left = gw * 0.5 - block_w * 0.5;

    let mut y = 10.0;
    if !header_lines.is_empty() {
        list.rect(
            Rect::new(
                (block_left - 1.0) * sf,
                (y - 1.0) * sf,
                (block_w + 2.0) * sf,
                (header_lines.len() as f32 * row_h + 1.0) * sf,
            ),
            [0.0, 0.0, 0.0, 0.5],
        );
        for (i, line) in header_lines.iter().enumerate() {
            let lw = font.measure(line, 1.0);
            shadowed_text(
                list,
                [(gw * 0.5 - lw * 0.5) * sf, (y + i as f32 * row_h) * sf],
                line,
                sf,
                [1.0; 4],
            );
        }
        y += header_lines.len() as f32 * row_h + 1.0;
    }

    list.rect(
        Rect::new(
            (block_left - 1.0) * sf,
            (y - 1.0) * sf,
            (block_w + 2.0) * sf,
            (geometry.rows as f32 * row_h + 1.0) * sf,
        ),
        [0.0, 0.0, 0.0, 0.5],
    );

    for (index, entry) in entries.iter().enumerate() {
        let col = index / geometry.rows;
        let line = index % geometry.rows;
        let cx = geometry.left + col as f32 * (geometry.column_width + 5.0);
        let cy = y + line as f32 * row_h;

        list.rect(
            Rect::new(
                cx * sf,
                cy * sf,
                geometry.column_width * sf,
                (row_h - 1.0) * sf,
            ),
            [1.0, 1.0, 1.0, 0.125],
        );

        let color = if entry.game_mode == 3 {
            [1.0, 1.0, 1.0, 0.57]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        shadowed_text(list, [cx * sf, cy * sf], &entry.name, sf, color);

        if let Some(icons) = icons {
            let j = if entry.ping < 0 {
                5
            } else if entry.ping < 150 {
                0
            } else if entry.ping < 300 {
                1
            } else if entry.ping < 600 {
                2
            } else if entry.ping < 1000 {
                3
            } else {
                4
            };
            let ping_src = icons.sub(0, 176 + j * 8, 10, 8);
            let ping_dst = Rect::new(
                (cx + geometry.column_width - 11.0) * sf,
                cy * sf,
                10.0 * sf,
                8.0 * sf,
            );
            list.image(ping_src, ping_dst, [1.0; 4]);
        }
    }
    y += geometry.rows as f32 * row_h + 1.0;

    if !footer_lines.is_empty() {
        list.rect(
            Rect::new(
                (block_left - 1.0) * sf,
                (y - 1.0) * sf,
                (block_w + 2.0) * sf,
                (footer_lines.len() as f32 * row_h + 1.0) * sf,
            ),
            [0.0, 0.0, 0.0, 0.5],
        );
        for (i, line) in footer_lines.iter().enumerate() {
            let lw = font.measure(line, 1.0);
            shadowed_text(
                list,
                [(gw * 0.5 - lw * 0.5) * sf, (y + i as f32 * row_h) * sf],
                line,
                sf,
                [1.0; 4],
            );
        }
    }
}
