// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the mods menu: the helpers and the layout of the panel pages. The
//! landing page is in `landing`, the list in `list`, the settings page in
//! `settings`, the page flow in `flow` and the HUD layer inside the menu in `hud`.

mod flow;
mod hud;
mod landing;
mod landing_look;
mod list;
mod settings;

use super::card::controls;
use super::layout::{card_rect, columns};
use super::settings_panel::{choice_button, row_rect, slider_track, swatch_rect, toggle_track};
use super::*;
use crate::draw::DrawCommand;
use crate::geom::Rect;
use crate::input::{InputEvent, Key, PointerButton, UiInput};
use crate::menus::common::testing::{click_at, frame, VIEWPORT};
use crate::mod_widget::{ModLine, ModWidgetView};
use crate::modern;

fn card(id: &str, name: &str, category: usize, settings: Vec<SettingView>) -> ModCardView {
    ModCardView {
        id: id.into(),
        name: name.into(),
        description: format!("About {name}"),
        category,
        enabled: false,
        settings,
    }
}

fn setting(key: &str, control: SettingControl) -> SettingView {
    SettingView {
        key: key.into(),
        label: format!("Label {key}"),
        control,
    }
}

fn view() -> ModsView {
    let settings = vec![
        setting("shadow", SettingControl::Toggle(false)),
        setting(
            "size",
            SettingControl::Slider {
                min: 1.0,
                max: 3.0,
                step: 0.5,
                value: 1.0,
            },
        ),
        setting(
            "mode",
            SettingControl::Choice {
                options: vec![
                    ("split".into(), "Split".into()),
                    ("sum".into(), "Sum".into()),
                ],
                selected: 0,
            },
        ),
        setting("color", SettingControl::Color("#ffffff".into())),
    ];
    ModsView {
        categories: vec!["HUD".into(), "Gameplay".into()],
        cards: vec![
            card("fps", "FPS", 0, settings),
            card("coordinates", "Coordinates", 0, Vec::new()),
            card("toggle_sprint", "Toggle Sprint", 1, Vec::new()),
        ],
        hud: HudEditorView::default(),
    }
}

/// No snapping: a dragged widget lands where the pointer puts it.
fn no_snap(rect: Rect, _screen: [f32; 2]) -> Rect {
    rect
}

/// A menu already on the list page, the one most tests start from.
fn on_list() -> ModsScreen {
    ModsScreen {
        page: Page::List,
        ..ModsScreen::default()
    }
}

fn show_view(screen: &mut ModsScreen, view: &ModsView, input: UiInput) -> Option<ModsOutcome> {
    let atlas = FontAtlas::fallback();
    frame(input, |ui| {
        screen.show(
            ui,
            (&atlas, &atlas),
            &Lang::builtin(),
            view,
            (&no_snap, 0.0),
        )
    })
}

fn show(screen: &mut ModsScreen, input: UiInput) -> Option<ModsOutcome> {
    show_view(screen, &view(), input)
}

fn texts(screen: &mut ModsScreen, view: &ModsView) -> Vec<String> {
    let atlas = FontAtlas::fallback();
    frame(UiInput::default(), |ui| {
        screen.show(
            ui,
            (&atlas, &atlas),
            &Lang::builtin(),
            view,
            (&no_snap, 0.0),
        );
        ui.draw()
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    })
}

fn has_text(texts: &[String], wanted: &str) -> bool {
    texts.iter().any(|t| t == wanted)
}

fn first_card() -> Rect {
    let layout = mods_layout(VIEWPORT);
    let cols = columns(layout.list.width / layout.scale);
    card_rect(layout.list, layout.scale, 0, (cols, 0.0))
}

/// Opens the settings page of the first mod.
fn on_settings_page() -> ModsScreen {
    let mut screen = on_list();
    let (_, gear) = controls(first_card(), mods_layout(VIEWPORT).scale);
    assert_eq!(show(&mut screen, click_at(gear)), None);
    screen
}

fn row(index: usize) -> Rect {
    let layout = mods_layout(VIEWPORT);
    row_rect(layout.detail, layout.scale, index)
}

#[test]
fn the_panel_is_centred_and_holds_every_part() {
    let layout = mods_layout(VIEWPORT);
    let centre = VIEWPORT.width * 0.5;
    let half_unit = layout.scale * 0.5;
    assert!((layout.panel.x + layout.panel.width * 0.5 - centre).abs() <= half_unit);
    for part in [
        layout.header,
        layout.close,
        layout.search,
        layout.back,
        layout.tabs,
        layout.list,
        layout.detail,
    ] {
        assert!(part.x >= layout.panel.x && part.right() <= layout.panel.right() + 1e-3);
        assert!(part.y >= layout.panel.y && part.bottom() <= layout.panel.bottom() + 1e-3);
    }
    assert!(layout.search.bottom() <= layout.tabs.y);
    assert!(layout.tabs.bottom() <= layout.list.y);
}

#[test]
fn a_small_screen_shrinks_the_panel_to_fit() {
    let small = Rect::new(0.0, 0.0, 400.0, 300.0);
    let layout = mods_layout(small);
    assert!(layout.panel.x >= 0.0 && layout.panel.right() <= small.right());
    assert!(layout.panel.y >= 0.0 && layout.panel.bottom() <= small.bottom());
}

#[test]
fn the_sheet_and_the_blur_fade_in_while_the_panel_is_there_at_once() {
    let mut screen = ModsScreen::new();
    assert_eq!(screen.blur_strength(0.0), 0.0);
    let mut previous = 0.0;
    for _ in 0..animation::OPEN_TICKS {
        screen.tick();
        let now = screen.blur_strength(0.0);
        assert!(now >= previous);
        previous = now;
    }
    assert_eq!(previous, 1.0);
    assert!(has_text(&texts(&mut on_list(), &view()), "FPS"));
}
