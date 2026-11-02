// SPDX-License-Identifier: MIT OR Apache-2.0

//! The player tab list (`GuiPlayerTabOverlay.renderPlayerlist`).
//!
//! The columns come from [`TabLayout`]; this module measures the names,
//! draws the bands, the header and footer (wrapped to `width - 50`), the
//! heads, the names, the score column and the ping bars.
//!
//! Gap with the reference: the head is a flat square, light brown with a
//! skin and the Steve or Alex colour without. The GUI draw list cannot
//! sample a skin texture yet; the column width and position are exact.

use umicraft_ui::{DrawList, FontAtlas, Rect, SpriteSheet};

use super::runs::{draw_runs, obfuscation_tick, styled, width, wrap};
use super::tab_layout::{
    hearts_plan, ping_row, HeartsPlan, TabLayout, ValueColumn, HEAD_WIDTH, MAX_VISIBLE, ROW_HEIGHT,
};
use crate::hud::{ChatRunView, HudState, TabEntryView, TabListView, TabValueStyle};
use umicraft_ui::styled_text::draw_styled;

/// `Integer.MIN_VALUE`, the `0x80000000` band: black at half opacity.
const BAND: [f32; 4] = [0.0, 0.0, 0.0, 128.0 / 255.0];
/// `553648127`, the `0x20FFFFFF` band behind a player.
const ROW_BAND: [f32; 4] = [1.0, 1.0, 1.0, 32.0 / 255.0];
/// `-1862270977`, the `0x91FFFFFF` colour of a spectator name.
const SPECTATOR_ALPHA: f32 = 145.0 / 255.0;
/// The top of the first header line.
const TOP: i32 = 10;
/// `EnumChatFormatting.YELLOW`, the colour of integer scores.
const YELLOW: [u8; 3] = [0xFF, 0xFF, 0x55];
/// The Steve and the Alex tint of a head without a skin.
const STEVE: [f32; 4] = [0.45, 0.30, 0.20, 1.0];
const ALEX: [f32; 4] = [0.80, 0.60, 0.45, 1.0];
const SKIN: [f32; 4] = [0.70, 0.55, 0.45, 1.0];

/// Where the drawing goes: the GUI scale factor and the font.
struct Pen<'a> {
    sf: f32,
    font: &'a FontAtlas,
    state: &'a HudState,
}

impl Pen<'_> {
    fn rect(&self, list: &mut DrawList, [x0, y0, x1, y1]: [i32; 4], color: [f32; 4]) {
        let (sf, to_px) = (self.sf, |value: i32| value as f32);
        let rect = Rect::new(
            to_px(x0) * sf,
            to_px(y0) * sf,
            to_px(x1 - x0) * sf,
            to_px(y1 - y0) * sf,
        );
        list.rect(rect, color);
    }

    fn text(&self, list: &mut DrawList, at: [f32; 2], runs: &[ChatRunView], alpha: f32) {
        let at = [at[0] * self.sf, at[1] * self.sf];
        draw_runs(list, self.font, self.state, at, runs, self.sf, alpha, true);
    }

    fn plain(&self, list: &mut DrawList, at: [f32; 2], text: &str, rgb: [u8; 3]) {
        self.text(list, at, &[plain_run(text, rgb)], 1.0);
    }
}

fn plain_run(text: &str, color: [u8; 3]) -> ChatRunView {
    ChatRunView {
        text: text.to_owned(),
        color,
        bold: false,
        italic: false,
        underlined: false,
        strikethrough: false,
        obfuscated: false,
        ..ChatRunView::default()
    }
}

/// The widest `" " + score` of the integer objective, `j` in the reference.
fn widest_score(font: &FontAtlas, entries: &[TabEntryView]) -> i32 {
    entries
        .iter()
        .filter_map(|entry| entry.score)
        .map(|score| font.measure(&format!(" {score}"), 1.0).ceil() as i32)
        .max()
        .unwrap_or(0)
}

/// Draws the tab list centred on a screen `gw` units wide.
pub(super) fn draw_tab_list(
    list: &mut DrawList,
    state: &HudState,
    tab: &TabListView,
    gw: f32,
    sf: f32,
    (font, icons): (&FontAtlas, Option<&SpriteSheet>),
) {
    if tab.entries.is_empty() {
        return;
    }
    let pen = Pen { sf, font, state };
    let screen_w = gw as i32;
    let name_width = tab
        .entries
        .iter()
        .map(|entry| width(font, &entry.name).ceil() as i32)
        .max()
        .unwrap_or(0);
    let value = match tab.value_style {
        None => ValueColumn::None,
        Some(TabValueStyle::Hearts) => ValueColumn::Hearts,
        Some(TabValueStyle::Integer) => ValueColumn::Integer(widest_score(font, &tab.entries)),
    };
    let visible = &tab.entries[..tab.entries.len().min(MAX_VISIBLE)];
    let layout = TabLayout::new(
        visible.len() as i32,
        name_width,
        value,
        tab.show_heads,
        screen_w,
    );

    let wrap_to = (screen_w - 50) as f32;
    let header = tab.header.as_deref().map(|runs| wrap(font, runs, wrap_to));
    let footer = tab.footer.as_deref().map(|runs| wrap(font, runs, wrap_to));
    let widest_text = header
        .iter()
        .chain(&footer)
        .flatten()
        .map(|row| width(font, row) as i32)
        .fold(layout.total_width, i32::max);

    let band = |top: i32, bottom: i32| {
        [
            screen_w / 2 - widest_text / 2 - 1,
            top - 1,
            screen_w / 2 + widest_text / 2 + 1,
            bottom,
        ]
    };

    let mut y = TOP;
    if let Some(rows) = &header {
        pen.rect(list, band(y, y + rows.len() as i32 * ROW_HEIGHT), BAND);
        y = draw_centred_rows(list, &pen, rows, y, screen_w) + 1;
    }
    pen.rect(list, band(y, y + layout.rows * ROW_HEIGHT), BAND);
    for (index, entry) in visible.iter().enumerate() {
        let column = index as i32 / layout.rows;
        let row = index as i32 % layout.rows;
        let at = [layout.column_x(column), y + row * ROW_HEIGHT];
        draw_entry(list, &pen, entry, at, (&layout, name_width, tab), icons);
    }
    if let Some(rows) = &footer {
        let top = y + layout.rows * ROW_HEIGHT + 1;
        pen.rect(list, band(top, top + rows.len() as i32 * ROW_HEIGHT), BAND);
        draw_centred_rows(list, &pen, rows, top, screen_w);
    }
}

/// Draws header or footer rows centred, returns the `y` below the last.
fn draw_centred_rows(
    list: &mut DrawList,
    pen: &Pen,
    rows: &[Vec<ChatRunView>],
    mut y: i32,
    screen_w: i32,
) -> i32 {
    for row in rows {
        let x = screen_w / 2 - width(pen.font, row) as i32 / 2;
        pen.text(list, [x as f32, y as f32], row, 1.0);
        y += ROW_HEIGHT;
    }
    y
}

/// One player: band, head, name, score and ping.
fn draw_entry(
    list: &mut DrawList,
    pen: &Pen,
    entry: &TabEntryView,
    [x, y]: [i32; 2],
    (layout, name_width, tab): (&TabLayout, i32, &TabListView),
    icons: Option<&SpriteSheet>,
) {
    pen.rect(list, [x, y, x + layout.column_width, y + 8], ROW_BAND);
    let mut name_x = x;
    if tab.show_heads {
        pen.rect(list, [x, y, x + 8, y + 8], head_color(entry));
        name_x += HEAD_WIDTH;
    }
    if entry.spectator {
        let runs = styled(&entry.name, SPECTATOR_ALPHA, true);
        let at = [name_x as f32 * pen.sf, y as f32 * pen.sf];
        let tick = obfuscation_tick(pen.state);
        draw_styled(list, pen.font, at, &runs, pen.sf, true, tick);
    } else {
        pen.text(list, [name_x as f32, y as f32], &entry.name, 1.0);
    }

    let start = name_x + name_width + 1;
    let end = start + layout.value_width;
    if let (Some(score), false) = (entry.score, entry.spectator) {
        if end - start > 5 {
            draw_value(list, pen, tab.value_style, score, [start, end, y], icons);
        }
    }
    if let Some(icons) = icons {
        let ping_x = (x + layout.column_width - 11) as f32;
        let src = icons.sub(0, 176 + ping_row(entry.ping) * 8, 10, 8);
        let dst = Rect::new(
            ping_x * pen.sf,
            y as f32 * pen.sf,
            10.0 * pen.sf,
            8.0 * pen.sf,
        );
        list.image(src, dst, [1.0; 4]);
    }
}

/// The placeholder colour of a head (see the module docs).
fn head_color(entry: &TabEntryView) -> [f32; 4] {
    if entry.has_skin {
        SKIN
    } else if (entry.uuid as u32 ^ (entry.uuid >> 32) as u32) & 1 == 1 {
        ALEX
    } else {
        STEVE
    }
}

/// `drawScoreboardValues`: hearts or a yellow number in `start..end`.
fn draw_value(
    list: &mut DrawList,
    pen: &Pen,
    style: Option<TabValueStyle>,
    score: i32,
    [start, end, y]: [i32; 3],
    icons: Option<&SpriteSheet>,
) {
    if style == Some(TabValueStyle::Hearts) {
        match hearts_plan(score, start, end) {
            Some(HeartsPlan::Icons(sprites)) => {
                let Some(icons) = icons else { return };
                for (offset, u) in sprites {
                    let dst = Rect::new(
                        (start as f32 + offset) * pen.sf,
                        y as f32 * pen.sf,
                        9.0 * pen.sf,
                        9.0 * pen.sf,
                    );
                    list.image(icons.sub(u, 0, 9, 9), dst, [1.0; 4]);
                }
            }
            Some(HeartsPlan::Text { half_hearts, rgb }) => {
                let fits =
                    (end - start) as f32 - pen.font.measure(&format!("{half_hearts}hp"), 1.0);
                let text = if fits >= 0.0 {
                    format!("{half_hearts}hp")
                } else {
                    half_hearts
                };
                let x = (start + end) / 2 - pen.font.measure(&text, 1.0) as i32 / 2;
                pen.plain(list, [x as f32, y as f32], &text, rgb);
            }
            None => {}
        }
    } else {
        let text = score.to_string();
        let x = end - pen.font.measure(&text, 1.0).ceil() as i32;
        pen.plain(list, [x as f32, y as f32], &text, YELLOW);
    }
}

#[cfg(test)]
mod tests {
    use super::super::runs::run;
    use super::*;
    use umicraft_ui::DrawCommand;

    fn entry(name: &str, score: Option<i32>, spectator: bool) -> TabEntryView {
        TabEntryView {
            name: vec![run(name)],
            ping: 20,
            spectator,
            score,
            uuid: 1,
            has_skin: false,
        }
    }

    fn view(entries: Vec<TabEntryView>) -> TabListView {
        TabListView {
            header: None,
            footer: None,
            entries,
            value_style: None,
            show_heads: false,
        }
    }

    fn draw(tab: &TabListView) -> DrawList {
        let mut list = DrawList::new();
        let font = FontAtlas::fallback();
        let sheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
        let state = HudState::default();
        draw_tab_list(&mut list, &state, tab, 320.0, 1.0, (&font, Some(&sheet)));
        list
    }

    fn texts(list: &DrawList) -> Vec<(String, f32)> {
        list.commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, style, .. } => Some((text.clone(), style.color[3])),
                _ => None,
            })
            .collect()
    }

    fn images(list: &DrawList) -> usize {
        list.commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::Image { .. }))
            .count()
    }

    #[test]
    fn an_empty_list_draws_nothing() {
        assert!(draw(&view(vec![])).is_empty());
    }

    #[test]
    fn each_player_gets_a_name_and_a_ping_bar() {
        let list = draw(&view(vec![
            entry("Ann", None, false),
            entry("Bob", None, false),
        ]));
        let names: Vec<String> = texts(&list).into_iter().map(|(t, _)| t).collect();
        assert!(names.contains(&"Ann".to_owned()) && names.contains(&"Bob".to_owned()));
        assert_eq!(images(&list), 2);
    }

    #[test]
    fn a_spectator_name_is_faint() {
        let list = draw(&view(vec![entry("Spec", None, true)]));
        let alphas: Vec<f32> = texts(&list)
            .into_iter()
            .filter(|(t, _)| t == "Spec")
            .map(|(_, a)| a)
            .collect();
        assert!(alphas.iter().all(|a| (*a - SPECTATOR_ALPHA).abs() < 1e-6));
    }

    #[test]
    fn only_eighty_players_are_drawn() {
        let entries = (0..90)
            .map(|i| entry(&format!("P{i:02}"), None, false))
            .collect();
        assert_eq!(images(&draw(&view(entries))), MAX_VISIBLE);
    }

    #[test]
    fn an_integer_objective_draws_a_yellow_score_but_not_for_spectators() {
        let mut tab = view(vec![
            entry("Ann", Some(7), false),
            entry("Spec", Some(9), true),
        ]);
        tab.value_style = Some(TabValueStyle::Integer);
        let list = draw(&tab);
        let numbers: Vec<String> = texts(&list)
            .into_iter()
            .map(|(t, _)| t)
            .filter(|t| t.chars().all(|c| c.is_ascii_digit()))
            .collect();
        assert_eq!(numbers, ["7", "7"], "shadow and text of one score");
    }

    #[test]
    fn a_hearts_objective_draws_heart_sprites() {
        let mut tab = view(vec![entry("Ann", Some(20), false)]);
        tab.value_style = Some(TabValueStyle::Hearts);
        let list = draw(&tab);
        // Ping bar, then 10 containers and 10 hearts.
        assert_eq!(images(&list), 1 + 20);
    }

    #[test]
    fn the_head_column_pushes_the_name_nine_units_right() {
        let mut tab = view(vec![entry("Ann", None, false)]);
        let plain_x = first_text_x(&draw(&tab));
        tab.show_heads = true;
        let head_x = first_text_x(&draw(&tab));
        // The wider column re-centres the list, so compare against it.
        assert!((head_x - plain_x - 4.0).abs() <= 1.0, "{plain_x} {head_x}");
    }

    fn first_text_x(list: &DrawList) -> f32 {
        list.commands()
            .iter()
            .find_map(|command| match command {
                DrawCommand::Text { position, .. } => Some(position[0]),
                _ => None,
            })
            .expect("text")
    }

    #[test]
    fn the_header_and_footer_are_drawn_wrapped() {
        let mut tab = view(vec![entry("Ann", None, false)]);
        tab.header = Some(vec![run("one\ntwo")]);
        tab.footer = Some(vec![run("foot")]);
        let names: Vec<String> = texts(&draw(&tab)).into_iter().map(|(t, _)| t).collect();
        for expected in ["one", "two", "foot"] {
            assert!(
                names.contains(&expected.to_owned()),
                "{expected} in {names:?}"
            );
        }
    }
}
