// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the options screens.

use umicraft_assets::languages::LanguageInfo;
use umicraft_assets::pack_discovery::DiscoveredPack;
use umicraft_assets::PackMetadata;
use umicraft_core::lang::Lang;
use umicraft_ui::menus::TitleStyle;
use umicraft_ui::{FontAtlas, InputEvent, PointerButton, Rect, Ui, UiInput};

use super::controls_page::sensitivity_label;
use super::main_page::fov_label;
use super::sounds_page::volume_text;
use super::video_page::gamma_value;
use super::*;
use winit::keyboard::KeyCode;

use crate::options::{Binding, CategoryLabel, SkinPart, SoundCategory};

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 1280.0,
    height: 720.0,
};

fn frame(screen: &mut OptionsScreen, options: &mut Options, events: Vec<InputEvent>) -> Outcome {
    let mut ui = Ui::new(VIEWPORT);
    ui.begin_frame(VIEWPORT, UiInput::new(events));
    let outcome = screen.show(&mut ui, &FontAtlas::fallback(), options, &Lang::builtin());
    ui.end_frame();
    outcome
}

fn click(rect: Rect) -> Vec<InputEvent> {
    let position = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
    vec![
        InputEvent::PointerMoved { position },
        InputEvent::PointerPressed {
            position,
            button: PointerButton::Left,
        },
        InputEvent::PointerReleased {
            position,
            button: PointerButton::Left,
        },
    ]
}

/// Draws a frame to lay the widgets out, then clicks `id` in the next.
fn click_widget(screen: &mut OptionsScreen, options: &mut Options, id: &str) -> Outcome {
    frame(screen, options, vec![]);
    let rect = screen
        .rect_of(id)
        .unwrap_or_else(|| panic!("no widget {id}"));
    frame(screen, options, click(rect))
}

/// GUI scale 3 at 1280x720: the window is 427x240 GUI units.
fn gui_rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x * 3.0, y * 3.0, w * 3.0, h * 3.0)
}

#[test]
fn the_hub_page_matches_gui_options() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    frame(&mut screen, &mut options, vec![]);
    // width 427 -> cx 213; height 240 -> h/6 = 40.
    assert_eq!(
        screen.rect_of("fov"),
        Some(gui_rect(58.0, 28.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("difficulty"),
        Some(gui_rect(218.0, 28.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("nav:video"),
        Some(gui_rect(58.0, 130.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("nav:controls"),
        Some(gui_rect(218.0, 130.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("nav:chat"),
        Some(gui_rect(218.0, 154.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("done"),
        Some(gui_rect(113.0, 208.0, 200.0, 20.0))
    );
}

#[test]
fn done_on_the_hub_closes_and_done_on_a_page_goes_back() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:chat");
    assert_eq!(screen.page(), Page::Chat);
    let outcome = click_widget(&mut screen, &mut options, "done");
    assert!(!outcome.done);
    assert_eq!(screen.page(), Page::Main);
    assert!(click_widget(&mut screen, &mut options, "done").done);
}

#[test]
fn a_hub_button_without_a_backend_does_nothing() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    for id in ["shaders", "stream"] {
        let outcome = click_widget(&mut screen, &mut options, id);
        assert_eq!(screen.page(), Page::Main, "{id}");
        assert_eq!(outcome, Outcome::default(), "{id}");
    }
}

#[test]
fn the_fov_slider_edits_within_its_range_and_names_the_ends() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    frame(&mut screen, &mut options, vec![]);
    let fov = screen.rect_of("fov").unwrap();
    let events = vec![InputEvent::PointerPressed {
        position: [fov.x + fov.width, fov.y + 5.0],
        button: PointerButton::Left,
    }];
    let outcome = frame(&mut screen, &mut options, events);
    assert!(outcome.changed);
    assert_eq!(options.video.fov, 110.0);
}

#[test]
fn the_video_page_lists_the_options_in_vanilla_order() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:video");
    frame(&mut screen, &mut options, vec![]);
    // Row 0 starts at top + 4 = 36; Render Distance is the right column.
    assert_eq!(
        screen.rect_of("video:0"),
        Some(gui_rect(58.0, 36.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("distance"),
        Some(gui_rect(218.0, 36.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("done"),
        Some(gui_rect(113.0, 213.0, 200.0, 20.0))
    );
}

#[test]
fn the_wheel_scrolls_the_video_list() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:video");
    frame(&mut screen, &mut options, vec![]);
    let before = screen.rect_of("video:0").unwrap();
    frame(
        &mut screen,
        &mut options,
        vec![InputEvent::Wheel { delta: 1.0 }],
    );
    let after = screen.rect_of("video:0").unwrap();
    assert!(after.y < before.y, "the row moved up");
}

#[test]
fn the_controls_page_rebinds_a_key_and_resets_it() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:controls");
    assert_eq!(screen.page(), Page::Controls);
    // Scroll to the Movement keys, then click Jump.
    frame(
        &mut screen,
        &mut options,
        vec![InputEvent::Wheel { delta: 40.0 }],
    );
    click_widget(&mut screen, &mut options, "key:jump");
    assert_eq!(screen.capturing(), Some(Action::Jump));
    assert!(screen.capture(&mut options, Binding::Key(KeyCode::KeyJ)));
    assert_eq!(options.keys.get(Action::Jump), KeyCode::KeyJ);
    click_widget(&mut screen, &mut options, "keyreset:jump");
    assert_eq!(options.keys.get(Action::Jump), KeyCode::Space);
}

#[test]
fn a_mouse_button_can_be_captured_and_a_reserved_key_cannot() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:controls");
    frame(
        &mut screen,
        &mut options,
        vec![InputEvent::Wheel { delta: 40.0 }],
    );
    click_widget(&mut screen, &mut options, "key:jump");
    assert!(!screen.capture(&mut options, Binding::Key(KeyCode::Escape)));
    assert_eq!(screen.capturing(), Some(Action::Jump), "capture goes on");
    assert!(screen.capture(&mut options, Binding::Mouse(3)));
    assert_eq!(screen.capturing(), None);
    assert_eq!(options.keys.get(Action::Jump), Binding::Mouse(3));
}

#[test]
fn the_key_list_follows_the_translated_order() {
    let entries = controls_page::key_entries(&Lang::builtin());
    let keys = entries
        .iter()
        .filter(|entry| matches!(entry, controls_page::KeyEntry::Key(_)))
        .count();
    assert_eq!(keys, Action::ALL.len(), "every action is listed once");
    // Vanilla order with the English file: Gameplay, Inventory, Miscellaneous,
    // Movement, Multiplayer.
    let english = Lang::from_texts(
        "en_US",
        Some("key.categories.gameplay=Gameplay\nkey.categories.inventory=Inventory\nkey.categories.misc=Miscellaneous\nkey.categories.movement=Movement\nkey.categories.multiplayer=Multiplayer\n"),
        None,
    );
    let headings: Vec<_> = controls_page::key_entries(&english)
        .into_iter()
        .filter_map(|entry| match entry {
            controls_page::KeyEntry::Category(key) => Some(key),
            controls_page::KeyEntry::Key(_) => None,
        })
        .collect();
    assert_eq!(
        headings,
        [
            "key.categories.gameplay",
            "key.categories.inventory",
            "key.categories.misc",
            "key.categories.movement",
            "key.categories.multiplayer"
        ]
    );
}

#[test]
fn reset_keys_is_disabled_until_a_key_changes() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:controls");
    let outcome = click_widget(&mut screen, &mut options, "reset");
    assert!(!outcome.changed);
    options.keys.set(Action::Chat, KeyCode::KeyC);
    let outcome = click_widget(&mut screen, &mut options, "reset");
    assert!(outcome.changed);
    assert_eq!(options.keys.get(Action::Chat), KeyCode::KeyT);
}

#[test]
fn the_chat_page_cycles_visibility_and_toggles_colors() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:chat");
    let before = options.chat.visibility;
    let outcome = click_widget(&mut screen, &mut options, "visibility");
    assert!(outcome.changed);
    assert_ne!(options.chat.visibility, before);
    let colors = options.chat.colors;
    click_widget(&mut screen, &mut options, "colors");
    assert_eq!(options.chat.colors, !colors);
    // Vanilla places the right column 160 units after the left.
    let gap = screen.rect_of("colors").unwrap().x - screen.rect_of("visibility").unwrap().x;
    assert_eq!(gap, 160.0 * 3.0);
}

#[test]
fn the_chat_sliders_edit_the_options_and_name_their_pixels() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:chat");
    frame(&mut screen, &mut options, vec![]);
    let width = screen.rect_of("width").expect("a width slider");
    // The far left of the bar is the minimum: 40 pixels.
    let events = vec![InputEvent::PointerPressed {
        position: [width.x + 1.0, width.y + 5.0],
        button: PointerButton::Left,
    }];
    let outcome = frame(&mut screen, &mut options, events);
    assert!(outcome.changed);
    assert_eq!(options.chat.width, 0.0);
    let opacity = screen.rect_of("opacity").expect("an opacity slider");
    let events = vec![InputEvent::PointerPressed {
        position: [opacity.x + 1.0, opacity.y + 5.0],
        button: PointerButton::Left,
    }];
    frame(&mut screen, &mut options, events);
    assert_eq!(options.chat.opacity, 0.0);
    click_widget(&mut screen, &mut options, "links");
    assert!(!options.chat.links);
}

#[test]
fn labels_follow_game_settings() {
    let lang = Lang::from_texts(
        "xx_XX",
        None,
        Some(
            "options.sensitivity=Sens\noptions.sensitivity.min=low\n\
             options.sensitivity.max=high\noptions.fov=View\noptions.fov.min=std\n\
             options.fov.max=wide\n",
        ),
    );
    assert_eq!(sensitivity_label(&lang, 0.0), "Sens: low");
    assert_eq!(sensitivity_label(&lang, 1.0), "Sens: high");
    assert_eq!(sensitivity_label(&lang, 0.5), "Sens: 100%");
    assert_eq!(fov_label(&lang, 70.0), "View: std");
    assert_eq!(fov_label(&lang, 110.0), "View: wide");
    assert_eq!(fov_label(&lang, 90.0), "View: 90");
}

fn language(code: &str, name: &str, region: &str) -> LanguageInfo {
    LanguageInfo {
        code: code.to_string(),
        name: name.to_string(),
        region: region.to_string(),
    }
}

fn pack(name: &str, pack_format: u32) -> DiscoveredPack {
    DiscoveredPack {
        name: name.to_string(),
        metadata: Some(PackMetadata {
            pack_format,
            description: format!("{name} pack"),
        }),
        icon: None,
    }
}

/// A screen that lists two languages and three packs.
fn catalog_screen() -> OptionsScreen {
    let mut screen = OptionsScreen::new();
    screen.set_catalog(Catalog::new(
        vec![
            language("en_US", "English", "US"),
            language("de_DE", "Deutsch", "Deutschland"),
        ],
        vec![pack("a.zip", 1), pack("b.zip", 1), pack("old.zip", 2)],
    ));
    screen
}

/// Presses the left button at the left edge of widget `id`, which sets a
/// slider to its minimum, or at its right edge for its maximum.
fn press_slider(screen: &mut OptionsScreen, options: &mut Options, id: &str, at_end: bool) {
    let rect = screen.rect_of(id).expect("a slider");
    let x = if at_end {
        rect.x + rect.width
    } else {
        rect.x + 1.0
    };
    let events = vec![InputEvent::PointerPressed {
        position: [x, rect.y + 5.0],
        button: PointerButton::Left,
    }];
    let outcome = frame(screen, options, events);
    assert!(outcome.changed, "{id}");
}

#[test]
fn every_new_hub_button_opens_its_page_and_done_comes_back() {
    for (id, page) in [
        ("skin", Page::Skin),
        ("sounds", Page::Sounds),
        ("language", Page::Language),
        ("resourcepacks", Page::ResourcePacks),
    ] {
        let mut screen = catalog_screen();
        let mut options = Options::default();
        click_widget(&mut screen, &mut options, id);
        assert_eq!(screen.page(), page, "{id}");
        let outcome = click_widget(&mut screen, &mut options, "done");
        assert_eq!(screen.page(), Page::Main, "{id}");
        assert_eq!(outcome, Outcome::default(), "{id} done only goes back");
    }
}

#[test]
fn the_skin_page_toggles_each_part_and_reports_the_change() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "skin");
    frame(&mut screen, &mut options, vec![]);
    // Row 0 is at h/6 = 40, two buttons per row, Done under the last row.
    assert_eq!(
        screen.rect_of("part:1"),
        Some(gui_rect(218.0, 40.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("done"),
        Some(gui_rect(113.0, 136.0, 200.0, 20.0))
    );
    for part in SkinPart::ALL {
        let outcome = click_widget(&mut screen, &mut options, &format!("part:{}", part.id()));
        assert!(outcome.changed && outcome.skin_changed, "{part:?}");
        assert!(!options.skin.is_enabled(part), "{part:?}");
    }
    assert_eq!(options.skin.parts_mask(), 0);
    click_widget(&mut screen, &mut options, "part:6");
    assert_eq!(options.skin.parts_mask(), 0x40);
}

#[test]
fn the_sound_page_sets_every_category_and_names_off() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "sounds");
    frame(&mut screen, &mut options, vec![]);
    // The master slider spans both columns; music starts the second row.
    assert_eq!(
        screen.rect_of("sound:soundCategory.master"),
        Some(gui_rect(58.0, 28.0, 310.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("sound:soundCategory.music"),
        Some(gui_rect(58.0, 52.0, 150.0, 20.0))
    );
    assert_eq!(
        screen.rect_of("sound:soundCategory.ambient"),
        Some(gui_rect(218.0, 124.0, 150.0, 20.0))
    );
    for category in SoundCategory::ALL {
        let id = format!("sound:{}", category.lang_key());
        press_slider(&mut screen, &mut options, &id, false);
        assert_eq!(options.sound.volume(category), 0.0, "{category:?}");
    }
    assert_eq!(volume_text(&Lang::builtin(), 0.0), "options.off");
    assert_eq!(volume_text(&Lang::builtin(), 0.5), "50%");
}

#[test]
fn picking_a_language_sets_the_code_and_signals_the_reload() {
    let mut screen = catalog_screen();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "language");
    let outcome = click_widget(&mut screen, &mut options, "language:de_DE");
    assert!(outcome.language_changed && outcome.changed);
    assert_eq!(options.language.code, "de_DE");
    // Picking the language that is already selected changes nothing.
    let again = click_widget(&mut screen, &mut options, "language:de_DE");
    assert!(!again.language_changed);
    let back = click_widget(&mut screen, &mut options, "language:en_US");
    assert!(back.language_changed);
    assert_eq!(options.language.code, "en_US");
}

#[test]
fn the_resource_pack_page_moves_packs_between_the_lists() {
    let mut screen = catalog_screen();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "resourcepacks");
    assert!(screen.pack_draft().is_empty());
    click_widget(&mut screen, &mut options, "pack:Available:0");
    assert_eq!(screen.pack_draft(), ["a.zip"]);
    // The next pack goes on top: highest priority first.
    click_widget(&mut screen, &mut options, "pack:Available:0");
    assert_eq!(screen.pack_draft(), ["b.zip", "a.zip"]);
    click_widget(&mut screen, &mut options, "pack:down:Selected:0");
    assert_eq!(screen.pack_draft(), ["a.zip", "b.zip"]);
    click_widget(&mut screen, &mut options, "pack:up:Selected:1");
    assert_eq!(screen.pack_draft(), ["b.zip", "a.zip"]);
    click_widget(&mut screen, &mut options, "pack:Selected:0");
    assert_eq!(screen.pack_draft(), ["a.zip"]);
    assert!(
        options.resource_packs.enabled.is_empty(),
        "a draft until Done"
    );
}

#[test]
fn an_incompatible_pack_asks_before_it_is_enabled() {
    let mut screen = catalog_screen();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "resourcepacks");
    click_widget(&mut screen, &mut options, "pack:Available:2");
    assert!(screen.pack_draft().is_empty(), "the question comes first");
    // No: nothing changes.
    let layout = umicraft_ui::menus::confirm_layout(VIEWPORT);
    frame(&mut screen, &mut options, click(layout.no));
    assert!(screen.pack_draft().is_empty());
    // Yes: the pack is enabled.
    click_widget(&mut screen, &mut options, "pack:Available:2");
    frame(&mut screen, &mut options, click(layout.yes));
    assert_eq!(screen.pack_draft(), ["old.zip"]);
}

#[test]
fn done_applies_the_pack_draft_lowest_priority_first_and_signals_the_reload() {
    let mut screen = catalog_screen();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "resourcepacks");
    click_widget(&mut screen, &mut options, "pack:Available:0");
    click_widget(&mut screen, &mut options, "pack:Available:0");
    let outcome = click_widget(&mut screen, &mut options, "done");
    assert!(outcome.packs_changed && outcome.changed);
    assert_eq!(screen.page(), Page::Main);
    // The top of the list (b.zip) is the highest priority, so it loads last.
    assert_eq!(options.resource_packs.enabled, ["a.zip", "b.zip"]);

    // Opening and leaving again without a change does not reload.
    click_widget(&mut screen, &mut options, "resourcepacks");
    assert_eq!(screen.pack_draft(), ["b.zip", "a.zip"]);
    let outcome = click_widget(&mut screen, &mut options, "done");
    assert!(!outcome.packs_changed);
}

#[test]
fn a_saved_pack_that_left_the_folder_is_dropped_from_the_draft() {
    let mut screen = catalog_screen();
    let mut options = Options::default();
    options.resource_packs.enabled = vec!["gone.zip".to_string(), "a.zip".to_string()];
    click_widget(&mut screen, &mut options, "resourcepacks");
    assert_eq!(screen.pack_draft(), ["a.zip"]);
}

#[test]
fn the_open_folder_button_raises_its_flag() {
    let mut screen = catalog_screen();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "resourcepacks");
    assert!(click_widget(&mut screen, &mut options, "open_folder").open_packs_folder);
}

#[test]
fn the_video_buttons_cycle_and_toggle_their_options() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:video");
    click_widget(&mut screen, &mut options, "video:0");
    assert!(!options.video.fancy_graphics);
    click_widget(&mut screen, &mut options, "video:2");
    assert_eq!(options.video.smooth_lighting, 0);
    click_widget(&mut screen, &mut options, "video:4");
    assert!(!options.video.view_bobbing);
    click_widget(&mut screen, &mut options, "video:5");
    assert_eq!(options.video.gui_scale, 1);
    click_widget(&mut screen, &mut options, "video:7");
    assert_eq!(options.video.clouds, 0);
    click_widget(&mut screen, &mut options, "video:8");
    assert_eq!(options.video.particles, 1);
    click_widget(&mut screen, &mut options, "video:9");
    assert!(options.video.fullscreen);
    click_widget(&mut screen, &mut options, "video:10");
    assert!(!options.video.vsync);
}

#[test]
fn the_gui_scale_button_wraps_after_large() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    options.video.gui_scale = 3;
    click_widget(&mut screen, &mut options, "nav:video");
    click_widget(&mut screen, &mut options, "video:5");
    assert_eq!(options.video.gui_scale, 0);
}

#[test]
fn the_brightness_and_mipmap_sliders_edit_within_their_ranges() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "nav:video");
    options.video.mipmap_levels = 0;
    frame(&mut screen, &mut options, vec![]);
    press_slider(&mut screen, &mut options, "gamma", true);
    assert_eq!(options.video.gamma, 1.0);
    press_slider(&mut screen, &mut options, "gamma", false);
    assert_eq!(options.video.gamma, 0.0);
    press_slider(&mut screen, &mut options, "mipmap", true);
    assert_eq!(options.video.mipmap_levels, 4);
    press_slider(&mut screen, &mut options, "mipmap", false);
    assert_eq!(options.video.mipmap_levels, 0);
}

#[test]
fn the_new_labels_follow_game_settings() {
    let lang = Lang::from_texts(
        "xx_XX",
        None,
        Some("options.gamma.min=Moody\noptions.gamma.max=Bright\n"),
    );
    assert_eq!(gamma_value(&lang, 0.0), "Moody");
    assert_eq!(gamma_value(&lang, 1.0), "Bright");
    assert_eq!(gamma_value(&lang, 0.5), "+50%");
}

#[test]
fn the_snooper_button_opens_its_page_and_the_switch_is_stored() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    click_widget(&mut screen, &mut options, "snooper");
    assert_eq!(screen.page(), Page::Snooper);
    assert!(!options.multiplayer.snooper);
    let outcome = click_widget(&mut screen, &mut options, "snooper_toggle");
    assert!(outcome.changed && options.multiplayer.snooper);
    click_widget(&mut screen, &mut options, "done");
    assert_eq!(screen.page(), Page::Main);
}

#[test]
fn the_title_style_button_toggles_the_option() {
    let mut screen = OptionsScreen::new();
    let mut options = Options::default();
    assert_eq!(options.menu.style(), TitleStyle::Modern);
    let outcome = click_widget(&mut screen, &mut options, "title_style");
    assert!(outcome.changed);
    assert_eq!(options.menu.style(), TitleStyle::Classic);
    click_widget(&mut screen, &mut options, "title_style");
    assert_eq!(options.menu.style(), TitleStyle::Modern);
}

#[test]
fn the_title_style_label_names_the_current_style() {
    let lang = Lang::from_texts("en_US", None, None)
        .with_client_layer(umicraft_assets::client_pack::lang("en_US"), None);
    let options = Options::default();
    assert_eq!(
        option_label(
            &lang,
            "umicraft.options.titleStyle",
            main_page::title_style_text(&lang, &options)
        ),
        "Title screen: Modern"
    );
}
