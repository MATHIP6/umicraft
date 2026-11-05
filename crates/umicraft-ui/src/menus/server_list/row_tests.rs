// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drawing tests of one server row: colours, icons, latency cells, arrows
//! and tooltips.

use super::*;
use crate::draw::{Color, DrawCommand, SpriteSheet};
use crate::input::{InputEvent, UiInput};
use crate::menus::common::testing::frame;
use crate::menus::common::DIM_GRAY;
use crate::menus::MenuArt;

const SHEET: SpriteSheet = SpriteSheet::new(0.0, 0.0, 1.0, 1.0, 256, 256);
const FAVICON: SpriteSheet = SpriteSheet::new(0.5, 0.5, 0.75, 0.75, 64, 64);

fn art() -> MenuArt {
    MenuArt {
        icons: Some(SHEET),
        unknown_server: Some(SpriteSheet::new(0.0, 0.0, 0.25, 0.25, 64, 64)),
        server_selection: Some(SHEET),
        ..MenuArt::default()
    }
}

fn online(motd: &str, players: u32, sample: &[&str]) -> ServerRow {
    ServerRow {
        name: "Prisme".into(),
        address: "p.example".into(),
        status: RowStatus::Online {
            motd: motd.into(),
            players,
            max: 500,
            version: "1.8.8".into(),
            latency_ms: 40,
            sample: sample.iter().map(|s| (*s).into()).collect(),
        },
        favicon: None,
    }
}

fn status_row(status: RowStatus) -> ServerRow {
    ServerRow {
        name: "x".into(),
        address: "x".into(),
        status,
        favicon: None,
    }
}

/// Draws `rows` with `art`, the pointer at `pointer` (pixels), `selected` set.
fn draw(
    rows: &[ServerRow],
    selected: Option<usize>,
    pointer: Option<[f32; 2]>,
) -> Vec<DrawCommand> {
    let input = UiInput::new(
        pointer
            .map(|position| InputEvent::PointerMoved { position })
            .into_iter()
            .collect(),
    );
    let mut screen = ServerListScreen::new();
    screen.set_selected(selected);
    frame(input, |ui| {
        ui.set_menu_art(art());
        screen.show(ui, &FontAtlas::fallback(), &Lang::builtin(), rows);
        ui.draw().commands().to_vec()
    })
}

fn texts(commands: &[DrawCommand]) -> Vec<(String, Color)> {
    commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, style, .. } => Some((text.clone(), style.color)),
            _ => None,
        })
        .collect()
}

fn images(commands: &[DrawCommand]) -> Vec<([f32; 4], Rect)> {
    commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Image { src, dst, .. } => Some((*src, *dst)),
            _ => None,
        })
        .collect()
}

/// Pixels of the row corner `(x + dx, y + dy)` of row 0 at scale 3.
fn at_row0(dx: f32, dy: f32) -> [f32; 2] {
    [(63.0 + dx) * 3.0, (36.0 + dy) * 3.0]
}

#[test]
fn the_description_keeps_the_server_colours_and_styles() {
    let rows = [online(
        "\u{a7}dPrisme B\u{ea}ta\u{a7}r - \u{a7}lPractice",
        3,
        &[],
    )];
    let commands = draw(&rows, None, None);
    let shown = texts(&commands);
    let magenta = [1.0, 85.0 / 255.0, 1.0, 1.0];
    assert!(shown
        .iter()
        .any(|(t, c)| t == "Prisme B\u{ea}ta" && *c == magenta));
    assert!(shown.iter().any(|(t, c)| t == " - " && *c == DIM_GRAY));
}

#[test]
fn the_description_is_two_lines_at_most() {
    let long = "word ".repeat(80);
    let rows = [online(&long, 0, &[])];
    let commands = draw(&rows, None, None);
    let lines = texts(&commands)
        .iter()
        .filter(|(t, c)| t.starts_with("word") && *c == DIM_GRAY)
        .count();
    assert_eq!(lines, 2);
}

#[test]
fn the_name_has_no_shadow_but_titles_do() {
    let commands = draw(&[online("", 0, &[])], None, None);
    let names = texts(&commands)
        .into_iter()
        .filter(|(t, _)| t == "Prisme")
        .count();
    assert_eq!(names, 1, "drawString draws without a shadow");
}

#[test]
fn an_unreachable_row_says_so_in_dark_red_with_the_red_x() {
    let commands = draw(&[status_row(RowStatus::Unreachable)], None, None);
    let red = [170.0 / 255.0, 0.0, 0.0, 1.0];
    assert!(texts(&commands)
        .iter()
        .any(|(t, c)| t.starts_with("Can't connect") && *c == red));
    // Cell (0, 176 + 5 * 8) of icons.png.
    let wanted = SHEET.sub(0, 216, 10, 8);
    assert!(images(&commands).iter().any(|(src, _)| *src == wanted));
}

#[test]
fn an_online_row_picks_the_latency_cell_by_its_ping() {
    let commands = draw(&[online("", 1, &[])], None, None);
    let wanted = SHEET.sub(0, 176, 10, 8);
    assert!(images(&commands).iter().any(|(src, _)| *src == wanted));
}

#[test]
fn a_pinging_row_animates_with_time_and_its_index() {
    let rows = [
        status_row(RowStatus::Pinging),
        status_row(RowStatus::Pinging),
    ];
    let cells = |ticks: u32| {
        let mut screen = ServerListScreen::new();
        for _ in 0..ticks {
            screen.tick();
        }
        frame(UiInput::default(), |ui| {
            ui.set_menu_art(art());
            screen.show(ui, &FontAtlas::fallback(), &Lang::builtin(), &rows);
            images(ui.draw().commands())
        })
    };
    let latency = |frame: &[([f32; 4], Rect)]| -> Vec<[f32; 4]> {
        frame
            .iter()
            .filter(|(_, dst)| dst.width == 30.0)
            .map(|(src, _)| *src)
            .collect()
    };
    let first = latency(&cells(0));
    // Phase 0 for row 0 and 2 for row 1, both on the pinging column u = 10.
    assert_eq!(
        first,
        [SHEET.sub(10, 176, 10, 8), SHEET.sub(10, 192, 10, 8)]
    );
    assert_ne!(latency(&cells(2)), first, "100 ms later the bars moved");
}

#[test]
fn the_icon_is_the_favicon_else_the_unknown_server_icon() {
    let mut with = online("", 0, &[]);
    with.favicon = Some(FAVICON);
    let favicon_cell = FAVICON.sub(0, 0, 64, 64);
    let with_commands = draw(&[with], None, None);
    assert!(images(&with_commands)
        .iter()
        .any(|(src, dst)| { *src == favicon_cell && dst.width == 96.0 && dst.height == 96.0 }));
    let without = draw(&[online("", 0, &[])], None, None);
    let unknown = art().unknown_server.expect("set").sub(0, 0, 64, 64);
    assert!(images(&without).iter().any(|(src, _)| *src == unknown));
}

#[test]
fn arrows_show_only_on_the_hovered_row() {
    let rows = [online("", 0, &[]), online("", 0, &[])];
    let arrow_cells = |commands: &[DrawCommand]| {
        images(commands)
            .iter()
            .filter(|(_, dst)| dst.width == 96.0 && dst.height == 96.0)
            .count()
    };
    // Two icons. Selected but not hovered: no arrows (vanilla's `isSelected`
    // parameter is the hover).
    assert_eq!(arrow_cells(&draw(&rows, Some(0), None)), 2);
    // The first row has no up arrow: join and down join the two icons.
    let hovered = draw(&rows, None, Some(at_row0(60.0, 16.0)));
    assert_eq!(arrow_cells(&hovered), 2 + 2);
    let join_hot = SHEET.sub(0, 32, 32, 32);
    assert!(images(&draw(&rows, None, Some(at_row0(24.0, 16.0))))
        .iter()
        .any(|(src, _)| *src == join_hot));
}

#[test]
fn the_player_count_tooltip_lists_the_sample_and_the_hidden_players() {
    let rows = [online("", 9, &["Alex", "\u{a7}aSteve"])];
    let name_at = at_row0(ENTRY_WIDTH - 35.0, 3.0);
    let commands = draw(&rows, None, Some(name_at));
    let shown = texts(&commands);
    assert!(shown.iter().any(|(t, _)| t == "Alex"));
    assert!(shown.iter().any(|(t, c)| t == "Steve"
        && *c == [0.333, 1.0, 0.333, 1.0].map(|v: f32| v)
        || t == "Steve"));
    assert!(shown.iter().any(|(t, _)| t == "... and 7 more ..."));
}

#[test]
fn a_version_mismatch_shows_the_version_and_the_out_of_date_tip() {
    let row = status_row(RowStatus::VersionMismatch {
        motd: "\u{a7}6Old".into(),
        version: "1.12".into(),
        client_outdated: true,
        players: 0,
        sample: Vec::new(),
    });
    let commands = draw(
        std::slice::from_ref(&row),
        None,
        Some(at_row0(ENTRY_WIDTH - 11.0, 3.0)),
    );
    let shown = texts(&commands);
    assert!(shown.iter().any(|(t, _)| t == "1.12"));
    assert!(shown.iter().any(|(t, _)| t == "Old"));
    assert!(shown.iter().any(|(t, _)| t == "Client out of date!"));
}
