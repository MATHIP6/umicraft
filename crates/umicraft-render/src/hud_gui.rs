// SPDX-License-Identifier: MIT OR Apache-2.0

//! The textured in-game HUD rendered into the immediate-mode [`DrawList`] pass
//! using authentic Minecraft 1.8 GUI textures and proportional font (M10).
//!
//! Hotbar, status rows (health, food, armor), experience bar, crosshair,
//! boss bar, chat, scoreboard sidebar, titles, action bar and tab list are
//! rendered in vanilla GUI scale with official textures from `widgets.png` and
//! `icons.png`.

use umicraft_ui::{Color, DrawList, FontAtlas, Rect, SpriteSheet, TextStyle};

use crate::chat_hud::{self, line_alpha};
use crate::hud::{
    ChatLineView, ChatRunView, HudState, SidebarView, TabListView, TitleView, HOTBAR_SLOTS,
};
use crate::social_hud::tab_geometry;

/// The sprite sheets extracted from the GUI atlas for HUD rendering.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HudSheets {
    /// The `widgets.png` region from the GUI atlas.
    pub widgets: Option<SpriteSheet>,
    /// The `icons.png` region from the GUI atlas.
    pub icons: Option<SpriteSheet>,
}

impl HudSheets {
    /// Returns true when both `widgets` and `icons` sheets are present.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.widgets.is_some() && self.icons.is_some()
    }
}

/// The glyphs an obfuscated character cycles through.
const SCRAMBLE: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ0123456789";

/// Computes the vanilla GUI integer scale for a given physical viewport.
#[must_use]
pub fn gui_scale(width: u32, height: u32) -> u32 {
    let mut s = 1u32;
    while width / (s + 1) >= 320 && height / (s + 1) >= 240 {
        s += 1;
    }
    s
}

/// Draws the complete textured HUD into `list`.
pub fn draw_hud_gui(
    list: &mut DrawList,
    state: &HudState,
    viewport: (u32, u32),
    font: &FontAtlas,
    sheets: &HudSheets,
) {
    let s = gui_scale(viewport.0, viewport.1);
    let sf = s as f32;
    let gw = viewport.0 as f32 / sf;
    let gh = viewport.1 as f32 / sf;

    if let Some(widgets) = &sheets.widgets {
        draw_hotbar(list, state, gw, gh, sf, widgets);
    }

    if let Some(icons) = &sheets.icons {
        draw_icons_layer(list, state, gw, gh, sf, font, icons);
    }

    draw_chat(list, state, gw, gh, sf, font);
    draw_overlays(list, state, gw, gh, sf, font, sheets.icons.as_ref());
}

fn draw_hotbar(
    list: &mut DrawList,
    state: &HudState,
    gw: f32,
    gh: f32,
    sf: f32,
    widgets: &SpriteSheet,
) {
    let x = gw * 0.5 - 91.0;
    let y = gh - 22.0;

    // Hotbar background: (0, 0, 182, 22)
    let src = widgets.sub(0, 0, 182, 22);
    let dst = Rect::new(x * sf, y * sf, 182.0 * sf, 22.0 * sf);
    list.image(src, dst, [1.0; 4]);

    // Selected slot cursor: (0, 22, 24, 24) at (W/2 - 91 - 1 + slot * 20, H - 23)
    let slot = (state.selected_slot as usize).min(HOTBAR_SLOTS - 1) as f32;
    let sel_x = gw * 0.5 - 91.0 - 1.0 + slot * 20.0;
    let sel_y = gh - 23.0;
    let sel_src = widgets.sub(0, 22, 24, 24);
    let sel_dst = Rect::new(sel_x * sf, sel_y * sf, 24.0 * sf, 24.0 * sf);
    list.image(sel_src, sel_dst, [1.0; 4]);
    // The items and their counts are drawn by the UI layer over the hotbar.
}

fn draw_icons_layer(
    list: &mut DrawList,
    state: &HudState,
    gw: f32,
    gh: f32,
    sf: f32,
    font: &FontAtlas,
    icons: &SpriteSheet,
) {
    // Crosshair: icons (0, 0, 16, 16) at (W/2 - 7, H/2 - 7)
    if state.crosshair {
        let cx = (gw * 0.5 - 7.0) * sf;
        let cy = (gh * 0.5 - 7.0) * sf;
        let src = icons.sub(0, 0, 16, 16);
        let dst = Rect::new(cx, cy, 16.0 * sf, 16.0 * sf);
        list.image(src, dst, [1.0; 4]);
    }

    // Health: y = gh - 39
    let health_y = gh - 39.0;
    let ceil_health = state.health.ceil() as i32;
    for i in 0..10 {
        let x = gw * 0.5 - 91.0 + i as f32 * 8.0;
        let dst = Rect::new(x * sf, health_y * sf, 9.0 * sf, 9.0 * sf);
        // Container
        list.image(icons.sub(16, 0, 9, 9), dst, [1.0; 4]);
        let val = i * 2 + 1;
        if val < ceil_health {
            list.image(icons.sub(52, 0, 9, 9), dst, [1.0; 4]);
        } else if val == ceil_health {
            list.image(icons.sub(61, 0, 9, 9), dst, [1.0; 4]);
        }
    }

    // Food: y = gh - 39, x = gw / 2 + 91 - i * 8 - 9
    let food_y = gh - 39.0;
    let ceil_food = state.food.ceil() as i32;
    for i in 0..10 {
        let x = gw * 0.5 + 91.0 - i as f32 * 8.0 - 9.0;
        let dst = Rect::new(x * sf, food_y * sf, 9.0 * sf, 9.0 * sf);
        // Container
        list.image(icons.sub(16, 27, 9, 9), dst, [1.0; 4]);
        let val = i * 2 + 1;
        if val < ceil_food {
            list.image(icons.sub(52, 27, 9, 9), dst, [1.0; 4]);
        } else if val == ceil_food {
            list.image(icons.sub(61, 27, 9, 9), dst, [1.0; 4]);
        }
    }

    // Armor: y = gh - 49 if armor > 0
    if state.armor > 0.0 {
        let armor_y = gh - 49.0;
        let ceil_armor = state.armor.ceil() as i32;
        for i in 0..10 {
            let x = gw * 0.5 - 91.0 + i as f32 * 8.0;
            let dst = Rect::new(x * sf, armor_y * sf, 9.0 * sf, 9.0 * sf);
            let val = i * 2 + 1;
            let sub = if val < ceil_armor {
                icons.sub(34, 9, 9, 9)
            } else if val == ceil_armor {
                icons.sub(25, 9, 9, 9)
            } else {
                icons.sub(16, 9, 9, 9)
            };
            list.image(sub, dst, [1.0; 4]);
        }
    }

    // Experience bar: y = gh - 29
    let xp_x = gw * 0.5 - 91.0;
    let xp_y = gh - 29.0;
    list.image(
        icons.sub(0, 64, 182, 5),
        Rect::new(xp_x * sf, xp_y * sf, 182.0 * sf, 5.0 * sf),
        [1.0; 4],
    );
    let fill_w = ((state.experience * 183.0).floor() as u32).min(182);
    if fill_w > 0 {
        list.image(
            icons.sub(0, 69, fill_w, 5),
            Rect::new(xp_x * sf, xp_y * sf, fill_w as f32 * sf, 5.0 * sf),
            [1.0; 4],
        );
    }

    // Experience level: y = gh - 35
    if state.level > 0 {
        let s = state.level.to_string();
        let text_w = font.measure(&s, 1.0);
        let lx = gw * 0.5 - text_w * 0.5;
        let ly = gh - 35.0;
        let outline = [0.0, 0.0, 0.0, 1.0];
        let green = [0x80 as f32 / 255.0, 1.0, 0x20 as f32 / 255.0, 1.0];
        list.text([(lx + 1.0) * sf, ly * sf], &s, TextStyle::new(sf, outline));
        list.text([(lx - 1.0) * sf, ly * sf], &s, TextStyle::new(sf, outline));
        list.text([lx * sf, (ly + 1.0) * sf], &s, TextStyle::new(sf, outline));
        list.text([lx * sf, (ly - 1.0) * sf], &s, TextStyle::new(sf, outline));
        list.text([lx * sf, ly * sf], &s, TextStyle::new(sf, green));
    }

    // Boss bar: icons (0, 74, 182, 5) at (W/2 - 91, 12), fill (0, 79, l, 5), name at y=2
    if let Some(boss) = &state.boss {
        let bx = gw * 0.5 - 91.0;
        let by = 12.0;
        list.image(
            icons.sub(0, 74, 182, 5),
            Rect::new(bx * sf, by * sf, 182.0 * sf, 5.0 * sf),
            [1.0; 4],
        );
        let l = ((boss.health.clamp(0.0, 1.0) * 183.0).floor() as u32).min(182);
        if l > 0 {
            list.image(
                icons.sub(0, 79, l, 5),
                Rect::new(bx * sf, by * sf, l as f32 * sf, 5.0 * sf),
                [1.0; 4],
            );
        }
        let name_w = font.measure(&boss.name, 1.0);
        let nx = gw * 0.5 - name_w * 0.5;
        let ny = 2.0;
        list.text(
            [(nx + 1.0) * sf, (ny + 1.0) * sf],
            &boss.name,
            TextStyle::new(sf, [0.25, 0.25, 0.25, 1.0]),
        );
        list.text(
            [nx * sf, ny * sf],
            &boss.name,
            TextStyle::new(sf, [1.0, 1.0, 1.0, 1.0]),
        );
    }
}

/// A wrapped row of chat text with its computed opacity.
struct ChatRow {
    pieces: Vec<ChatRunView>,
    alpha: f32,
}

/// Wraps a chat line using proportional font glyph metrics (`font.advance`).
fn wrap_proportional(
    line: &ChatLineView,
    max_width: f32,
    font: &FontAtlas,
) -> Vec<Vec<ChatRunView>> {
    let chars: Vec<(char, usize, f32)> = line
        .runs
        .iter()
        .enumerate()
        .flat_map(|(index, run)| {
            run.text.chars().map(move |character| {
                let adv = font.advance(character, 1.0) + if run.bold { 1.0 } else { 0.0 };
                (character, index, adv)
            })
        })
        .collect();

    let mut rows = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut width = 0.0;
        let mut end = start;
        while end < chars.len() && width + chars[end].2 <= max_width {
            width += chars[end].2;
            end += 1;
        }

        let (chosen_end, next) = if end >= chars.len() {
            (chars.len(), chars.len())
        } else {
            // Find last space
            match (start + 1..=end).rev().find(|&i| chars[i - 1].0 == ' ') {
                Some(space) => (space - 1, space),
                None => (end.max(start + 1), end.max(start + 1)),
            }
        };

        let raw_chars: Vec<(char, usize)> = chars[start..chosen_end]
            .iter()
            .map(|&(c, idx, _)| (c, idx))
            .collect();
        rows.push(chat_hud::regroup(&line.runs, &raw_chars));
        start = next;
    }

    if rows.is_empty() {
        rows.push(Vec::new());
    }
    rows
}

fn draw_chat(list: &mut DrawList, state: &HudState, gw: f32, gh: f32, sf: f32, font: &FontAtlas) {
    let open = state.chat_open;
    // `GuiNewChat`: the text wraps at 320 and the band is `l + 4` wide, from x = 2.
    let text_max_w = 320.0_f32.min(gw - 4.0);
    let chat_width = text_max_w + 4.0;

    let mut rows: Vec<ChatRow> = Vec::new();
    for line in &state.chat {
        let age = chat_hud::age_ticks(line.received, state.now);
        let alpha = line_alpha(age, open);
        for pieces in wrap_proportional(line, text_max_w, font) {
            rows.push(ChatRow { pieces, alpha });
        }
    }

    let limit = if open { 20 } else { 10 };
    let scroll = if open {
        state.chat_scroll.min(rows.len().saturating_sub(limit))
    } else {
        0
    };
    let end = rows.len().saturating_sub(scroll);
    let first = end.saturating_sub(limit);
    let visible = &rows[first..end];

    let seed = state
        .now
        .map_or(0, |now| now.elapsed().as_millis() as usize / 50);

    // Bottom of chat is at y = gh - 28. Rows are 9 units tall, drawn bottom-up.
    let base_y = gh - 28.0;
    for (i, row) in visible.iter().rev().enumerate() {
        if row.alpha <= (3.0 / 255.0) {
            continue;
        }
        let line_y = base_y - (i as f32 + 1.0) * 9.0;
        let band_rect = Rect::new(2.0 * sf, line_y * sf, chat_width * sf, 9.0 * sf);
        list.rect(band_rect, [0.0, 0.0, 0.0, row.alpha * 0.5]);

        let mut cur_x = 2.0;
        let text_y = line_y + 1.0;
        for (run_idx, run) in row.pieces.iter().enumerate() {
            let run_text: String = if run.obfuscated {
                run.text
                    .chars()
                    .enumerate()
                    .map(|(char_idx, c)| {
                        if c == ' ' {
                            ' '
                        } else {
                            let idx = (seed + run_idx * 17 + char_idx * 31) % SCRAMBLE.len();
                            SCRAMBLE[idx] as char
                        }
                    })
                    .collect()
            } else {
                run.text.clone()
            };

            let text_color: Color = [
                run.color[0] as f32 / 255.0,
                run.color[1] as f32 / 255.0,
                run.color[2] as f32 / 255.0,
                row.alpha,
            ];
            let shadow_color: Color = [
                run.color[0] as f32 / 1020.0,
                run.color[1] as f32 / 1020.0,
                run.color[2] as f32 / 1020.0,
                row.alpha,
            ];

            let run_w = font.measure(&run_text, 1.0);

            // Shadow
            list.text(
                [(cur_x + 1.0) * sf, (text_y + 1.0) * sf],
                &run_text,
                TextStyle::new(sf, shadow_color),
            );
            if run.bold {
                list.text(
                    [(cur_x + 2.0) * sf, (text_y + 1.0) * sf],
                    &run_text,
                    TextStyle::new(sf, shadow_color),
                );
            }

            // Main text
            list.text(
                [cur_x * sf, text_y * sf],
                &run_text,
                TextStyle::new(sf, text_color),
            );
            if run.bold {
                list.text(
                    [(cur_x + 1.0) * sf, text_y * sf],
                    &run_text,
                    TextStyle::new(sf, text_color),
                );
            }

            if run.underlined {
                list.rect(
                    Rect::new(cur_x * sf, (text_y + 8.0) * sf, run_w * sf, sf),
                    text_color,
                );
            }
            if run.strikethrough {
                list.rect(
                    Rect::new(cur_x * sf, (text_y + 4.0) * sf, run_w * sf, sf),
                    text_color,
                );
            }

            cur_x += run_w + if run.bold { 1.0 } else { 0.0 };
        }
    }

    if open {
        draw_chat_scrollbar(list, rows.len(), visible.len(), scroll, base_y, sf);
    }

    // Chat input bar
    if let Some(input) = &state.chat_input {
        let input_rect = Rect::new(2.0 * sf, (gh - 14.0) * sf, (gw - 4.0) * sf, 12.0 * sf);
        list.rect(input_rect, [0.0, 0.0, 0.0, 0.5]);

        // `GuiTextField`: 0xE0E0E0 with a shadow, at (4, h - 12).
        let text = &input.text;
        let ink: Color = [0.878, 0.878, 0.878, 1.0];
        list.text(
            [(4.0 + 1.0) * sf, (gh - 12.0 + 1.0) * sf],
            text,
            TextStyle::new(sf, [0.22, 0.22, 0.22, 1.0]),
        );
        list.text([4.0 * sf, (gh - 12.0) * sf], text, TextStyle::new(sf, ink));

        if input.caret_visible {
            let caret_idx = input.caret.min(text.len());
            let caret_x = 4.0 + font.measure(&text[..caret_idx], 1.0);
            if caret_idx == text.len() {
                // At the end vanilla draws an underscore, not a bar.
                list.text(
                    [(caret_x + 1.0) * sf, (gh - 12.0 + 1.0) * sf],
                    "_",
                    TextStyle::new(sf, [0.22, 0.22, 0.22, 1.0]),
                );
                list.text(
                    [caret_x * sf, (gh - 12.0) * sf],
                    "_",
                    TextStyle::new(sf, ink),
                );
            } else {
                // `-3092272` = 0xFFD0D0D0, one unit wide, ten tall.
                list.rect(
                    Rect::new(caret_x * sf, (gh - 13.0) * sf, sf, 10.0 * sf),
                    [0.816, 0.816, 0.816, 1.0],
                );
            }
        }
    }
}

/// The scrollbar `GuiNewChat.drawChat` draws at the left of an open chat.
fn draw_chat_scrollbar(
    list: &mut DrawList,
    total: usize,
    shown: usize,
    scroll: usize,
    base_y: f32,
    sf: f32,
) {
    if total == 0 {
        return;
    }
    let (k, j) = (total as f32, shown as f32);
    let full = k * 10.0;
    let window = j * 10.0;
    if (full - window).abs() < f32::EPSILON {
        return;
    }
    let offset = (scroll as f32 * window / k).floor();
    let thumb = (window * window / full).floor();
    let alpha = if offset > 0.0 { 170.0 } else { 96.0 } / 255.0;
    let (top, height) = (base_y - offset - thumb, thumb);
    // 0xCC3333 while scrolled back, 0x33332A at the bottom.
    let fill: Color = if scroll > 0 {
        [0.8, 0.2, 0.2, alpha]
    } else {
        [0.2, 0.2, 0.165, alpha]
    };
    list.rect(Rect::new(-sf, top * sf, 2.0 * sf, height * sf), fill);
    list.rect(
        Rect::new(sf, top * sf, sf, height * sf),
        [0.8, 0.8, 0.8, alpha],
    );
}

fn draw_overlays(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_scale_at_various_resolutions() {
        assert_eq!(gui_scale(320, 240), 1);
        assert_eq!(gui_scale(640, 480), 2);
        assert_eq!(gui_scale(1280, 720), 3);
        assert_eq!(gui_scale(1920, 1080), 4);
    }

    #[test]
    fn draw_hud_gui_empty_state_emits_commands() {
        let mut list = DrawList::new();
        let state = HudState::default();
        let font = FontAtlas::fallback();
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let sheets = HudSheets {
            widgets: Some(sheet),
            icons: Some(sheet),
        };
        assert!(sheets.is_complete());

        draw_hud_gui(&mut list, &state, (1280, 720), &font, &sheets);
        assert!(!list.is_empty());

        let images = list
            .commands()
            .iter()
            .filter(|cmd| matches!(cmd, umicraft_ui::DrawCommand::Image { .. }))
            .count();
        // Hotbar, selector, crosshair, 10 hearts containers, 10 full hearts,
        // 10 food containers, 10 full food, xp bar background = 44 images
        assert!(images >= 40, "expected at least 40 images, got {images}");
    }

    #[test]
    fn draw_hud_gui_with_boss_bar() {
        use crate::BossView;

        let mut list = DrawList::new();
        let state = HudState {
            boss: Some(BossView {
                name: "Ender Dragon".to_string(),
                health: 0.8,
            }),
            ..Default::default()
        };
        let font = FontAtlas::fallback();
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let sheets = HudSheets {
            widgets: Some(sheet),
            icons: Some(sheet),
        };

        draw_hud_gui(&mut list, &state, (1280, 720), &font, &sheets);
        let boss_text = list.commands().iter().any(|cmd| match cmd {
            umicraft_ui::DrawCommand::Text { text, .. } => text == "Ender Dragon",
            _ => false,
        });
        assert!(boss_text);
    }
}
