// SPDX-License-Identifier: MIT OR Apache-2.0

//! The server-less M10 acceptance run (`--hud-demo`).
//!
//! The demo feeds scripted server packets (styled chat, a scoreboard with a
//! team, a tab list, a title and an action bar) through the same game code a
//! live session uses, then draws the resulting HUD, or one of the screens
//! (the game menu, the options, the controls), and prints what it built. With
//! `--screenshot PATH` the same scene is rendered offscreen, so the picture
//! and the numbers describe one state.
//!
//! Nothing here is read from the player's machine except the UI font, and the
//! texts are original Umicraft or generic player names.

use umicraft_game::chat_input::ChatInput;
use umicraft_net::play::PlayEvent;
use umicraft_protocol::packets::play::clientbound::{
    AddPlayerEntry, DisplayScoreboard, ObjectiveRenderType, PlayerListHeaderFooter, PlayerListItem,
    ScoreboardObjective, SpawnMob, TeamInfo, Teams, Title, UpdateScore,
};
use umicraft_protocol::packets::play::metadata::{Metadata, WatchedObject};
use umicraft_render::{
    draw_debug_only, draw_hud, draw_hud_gui, Camera, ChatInputView, HudSheets, HudState, Overlay,
    Renderer, SpriteSheet,
};
use umicraft_ui::game_menu::GameMenu;
use umicraft_ui::{DrawList, FontAtlas, InputEvent, PointerButton, Rect, Ui, UiInput};
use winit::keyboard::KeyCode;

use crate::args::{Args, HUD_VIEWS};
use crate::error::ClientError;
use crate::game::hud_after;
use crate::options::{Action, Options};
use crate::options_screen::OptionsScreen;
use crate::screenshot::write_png;
use crate::ui_font::{self, FontSource};

/// The name of the scripted player: a member of the red team.
const PLAYER: &str = "Steve";
/// Game ticks the title and the action bar are advanced by: past the fade-in.
const SCENE_TICKS: u32 = 20;

/// What a scene is made of.
enum Scene {
    /// The HUD, drawn by the overlay (fallback mode).
    Hud(Box<HudState>),
    /// The textured HUD, drawn by the UI pass with debug block on the overlay.
    HudGui(Box<HudState>, DrawList),
    /// A screen, drawn by the UI pass.
    Screen(DrawList),
}

/// The scene and the numbers that describe it.
struct Demo {
    scene: Scene,
    lines: Vec<(&'static str, String)>,
}

impl Demo {
    fn render(&self, view: &str, viewport: (u32, u32), source: FontSource) -> String {
        let mut out = String::new();
        out.push_str("umicraft M10 hud demo\n");
        out.push_str(&format!("view              : {view}\n"));
        out.push_str(&format!(
            "viewport          : {}x{}\n",
            viewport.0, viewport.1
        ));
        out.push_str(&format!("font              : {source}\n"));
        for (name, value) in &self.lines {
            out.push_str(&format!("{name:<18}: {value}\n"));
        }
        out
    }
}

fn text(json: &str) -> PlayEvent {
    PlayEvent::Chat(json.to_string())
}

fn uuid(seed: u8) -> u128 {
    u128::from_be_bytes([seed; 16])
}

fn player(seed: u8, name: &str, game_mode: i32, ping: i32) -> AddPlayerEntry {
    AddPlayerEntry {
        uuid: uuid(seed),
        name: name.to_string(),
        properties: Vec::new(),
        game_mode,
        ping,
        display_name: None,
    }
}

/// The packets a server would send to build the HUD of the demo.
fn scripted_packets() -> Vec<PlayEvent> {
    let mut events = vec![
        text(r#"{"text":"Welcome to the Umicraft demo server!","color":"yellow"}"#),
        text(r#"{"translate":"chat.type.text","with":[{"text":"Alex","color":"aqua"},"hello everyone"]}"#),
        text(
            r#"{"text":"[","color":"dark_gray","extra":[{"text":"Admin","color":"red","bold":true},{"text":"] ","color":"dark_gray"},{"text":"Bob joined the game","color":"yellow"}]}"#,
        ),
        text("{\"text\":\"\\u00a7aGreen \\u00a7lbold \\u00a7nunderlined \\u00a7mstruck \\u00a7kxxxx\\u00a7r plain\"}"),
        text(
            r#"{"text":"This is a long message from the server that wraps onto several rows so the word wrap of the chat can be seen at work.","color":"gray"}"#,
        ),
        text(r#"{"text":""}"#),
        text(r#"{"text":"Use ","color":"white","extra":[{"text":"/help","color":"green","underlined":true,"clickEvent":{"action":"suggest_command","value":"/help"}},{"text":" for the commands"}]}"#),
    ];

    events.push(PlayEvent::ScoreboardObjective(
        ScoreboardObjective::Create {
            name: "kills".to_string(),
            value: "Kills".to_string(),
            render_type: ObjectiveRenderType::Integer,
        },
    ));
    events.push(PlayEvent::DisplayScoreboard(DisplayScoreboard {
        position: 1,
        score_name: "kills".to_string(),
    }));
    for (name, value) in [
        ("Alex", 4),
        (PLAYER, 9),
        ("Bob", 5),
        ("Cleo", 7),
        ("#hidden", 99),
    ] {
        events.push(PlayEvent::UpdateScore(UpdateScore::Change {
            name: name.to_string(),
            objective: "kills".to_string(),
            value,
        }));
    }
    events.push(PlayEvent::Teams(Box::new(Teams::Create {
        name: "red".to_string(),
        info: TeamInfo {
            display_name: "Red".to_string(),
            prefix: "[R] ".to_string(),
            suffix: String::new(),
            friendly_flags: 0,
            name_tag_visibility: "always".to_string(),
            color: 12,
        },
        players: vec![PLAYER.to_string(), "Alex".to_string()],
    })));

    let mut players = vec![
        player(1, PLAYER, 0, 22),
        player(2, "Alex", 1, 48),
        player(3, "Bob", 0, 130),
        player(4, "Cleo", 0, 310),
        player(5, "Dana", 3, 75),
    ];
    for (seed, ping) in (6u8..28).zip([20, 60, 90, 160, 240, 420, 700, 1200].into_iter().cycle()) {
        players.push(player(seed, &format!("Player{seed:02}"), 0, ping));
    }
    players[2].display_name = Some(r#"{"text":"Bob the Builder","color":"gold"}"#.to_string());
    events.push(PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(
        players,
    )));
    events.push(PlayEvent::PlayerListHeaderFooter(PlayerListHeaderFooter {
        header: r#"{"text":"UMICRAFT DEMO SERVER","color":"gold"}"#.to_string(),
        footer: r#"{"text":"play.example.invalid","color":"gray"}"#.to_string(),
    }));

    events.push(PlayEvent::Title(Title::Title {
        json: r#"{"text":"ROUND 1","color":"red","bold":true}"#.to_string(),
    }));
    events.push(PlayEvent::Title(Title::Subtitle {
        json: r#"{"text":"FIGHT!","color":"yellow"}"#.to_string(),
    }));
    events.push(PlayEvent::ActionBar(
        r#"{"text":"Now playing: Umicraft theme","color":"green"}"#.to_string(),
    ));

    // Boss mob: an Ender Dragon with 150 health (out of 200)
    let mut dragon_metadata = Metadata::default();
    dragon_metadata.objects.push(WatchedObject {
        type_id: 3,
        index: 6,
        value: 150.0_f32.to_be_bytes().to_vec(),
    });
    events.push(PlayEvent::SpawnMob(SpawnMob {
        entity_id: 600,
        mob_type: 63,
        x: 0,
        y: 64 * 32,
        z: 0,
        yaw: 0,
        pitch: 0,
        head_yaw: 0,
        velocity_x: 0,
        velocity_y: 0,
        velocity_z: 0,
        metadata: dragon_metadata,
    }));

    events
}

/// The HUD of the demo: the vitals and the hotbar around the server data.
fn scene_hud() -> HudState {
    let mut hud = hud_after(scripted_packets(), PLAYER, SCENE_TICKS);
    hud.health = 14.0;
    hud.food = 17.0;
    hud.armor = 8.0;
    hud.experience = 0.4;
    hud.level = 7;
    hud.selected_slot = 2;
    hud
}

fn click(rect: Rect) -> UiInput {
    let position = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
    UiInput::new(vec![
        InputEvent::PointerMoved { position },
        InputEvent::PointerPressed {
            position,
            button: PointerButton::Left,
        },
        InputEvent::PointerReleased {
            position,
            button: PointerButton::Left,
        },
    ])
}

/// Runs the options screen through `inputs` (one UI frame each), then returns
/// the draw list of a last idle frame.
fn run_options(
    width: u32,
    height: u32,
    atlas: &FontAtlas,
    options: &mut Options,
    clicks: &[&str],
    widgets_sheet: Option<SpriteSheet>,
) -> (OptionsScreen, DrawList) {
    let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
    let mut ui = Ui::new(viewport);
    ui.set_widgets_sheet(widgets_sheet);
    let mut screen = OptionsScreen::new();
    let mut frame = |screen: &mut OptionsScreen, ui: &mut Ui, input: UiInput| {
        ui.begin_frame(viewport, input);
        screen.show(ui, atlas, options);
        ui.end_frame()
    };
    frame(&mut screen, &mut ui, UiInput::default());
    for id in clicks {
        let rect = screen.rect_of(id).unwrap_or(Rect::ZERO);
        let output = frame(&mut screen, &mut ui, click(rect));
        ui.recycle_output(output);
        // A click can change the page: lay the new one out before the next.
        let output = frame(&mut screen, &mut ui, UiInput::default());
        ui.recycle_output(output);
    }
    let output = frame(&mut screen, &mut ui, UiInput::default());
    (screen, output.draw_list)
}

fn build(view: &str, width: u32, height: u32, atlas: &FontAtlas, sheets: &HudSheets) -> Demo {
    match view {
        "menu" => {
            let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
            let mut ui = Ui::new(viewport);
            ui.set_widgets_sheet(sheets.widgets);
            let mut menu = GameMenu::new();
            ui.begin_frame(viewport, UiInput::default());
            menu.show(&mut ui, atlas);
            let output = ui.end_frame();
            Demo {
                lines: vec![
                    (
                        "buttons",
                        "Back to Game, Achievements, Statistics, Options..., Open to LAN, Disconnect"
                            .to_string(),
                    ),
                    ("draw commands", output.draw_list.len().to_string()),
                ],
                scene: Scene::Screen(output.draw_list),
            }
        }
        "options" => {
            let mut options = Options::default();
            options.video.fov = 95.0;
            options.video.render_distance = 12;
            let (screen, draw_list) =
                run_options(width, height, atlas, &mut options, &[], sheets.widgets);
            Demo {
                lines: vec![
                    ("page", format!("{:?}", screen.page())),
                    ("fov", options.video.fov.to_string()),
                    ("render distance", options.video.render_distance.to_string()),
                    ("max fps", options.video.max_fps.to_string()),
                    ("draw commands", draw_list.len().to_string()),
                ],
                scene: Scene::Screen(draw_list),
            }
        }
        "video" | "chat-options" => {
            let mut options = Options::default();
            let click = if view == "video" {
                "nav:video"
            } else {
                "nav:chat"
            };
            let (screen, draw_list) =
                run_options(width, height, atlas, &mut options, &[click], sheets.widgets);
            Demo {
                lines: vec![
                    ("page", format!("{:?}", screen.page())),
                    ("draw commands", draw_list.len().to_string()),
                ],
                scene: Scene::Screen(draw_list),
            }
        }
        "controls" => {
            let mut options = Options::default();
            // Two actions on one key show the conflict bar; a click on Hotbar Slot 3
            // leaves it waiting for a key.
            options.keys.set(Action::Hotbar2, KeyCode::Digit1);
            let (screen, draw_list) = run_options(
                width,
                height,
                atlas,
                &mut options,
                &["nav:controls", "key:hotbar3"],
                sheets.widgets,
            );
            let conflicts = Action::ALL
                .iter()
                .filter(|action| !options.keys.conflicts(**action).is_empty())
                .count();
            Demo {
                lines: vec![
                    ("page", format!("{:?}", screen.page())),
                    (
                        "waiting for a key",
                        screen
                            .capturing()
                            .map_or_else(|| "none".to_string(), |a| a.label().to_string()),
                    ),
                    ("actions in conflict", conflicts.to_string()),
                    ("draw commands", draw_list.len().to_string()),
                ],
                scene: Scene::Screen(draw_list),
            }
        }
        view => {
            let mut hud = scene_hud();
            let mut lines = vec![
                ("chat messages", hud.chat.len().to_string()),
                (
                    "boss",
                    hud.boss.as_ref().map_or_else(
                        || "none".to_string(),
                        |b| format!("{} (health {:.2})", b.name, b.health),
                    ),
                ),
                (
                    "sidebar",
                    hud.sidebar.as_ref().map_or_else(
                        || "none".to_string(),
                        |s| format!("{} ({} lines)", s.title, s.lines.len()),
                    ),
                ),
                (
                    "tab list",
                    hud.tab_list.as_ref().map_or_else(
                        || "none".to_string(),
                        |t| format!("{} players", t.entries.len()),
                    ),
                ),
                (
                    "title",
                    hud.title.as_ref().map_or_else(
                        || "none".to_string(),
                        |t| format!("{} / {} (alpha {:.2})", t.title, t.subtitle, t.alpha),
                    ),
                ),
                (
                    "action bar",
                    hud.action_bar.as_ref().map_or_else(
                        || "none".to_string(),
                        |(text, alpha)| format!("{text} (alpha {alpha:.2})"),
                    ),
                ),
            ];
            if view == "tab" {
                hud.show_tab_list = true;
            }
            if view == "chat" {
                let input = ChatInput::new("/gamemode cr", 0);
                hud.chat_open = true;
                hud.chat_input = Some(ChatInputView {
                    text: input.text(),
                    caret: input.caret(),
                    caret_visible: true,
                });
                lines.push(("chat input", input.text()));
            }

            if sheets.is_complete() {
                let mut draw_list = DrawList::default();
                draw_hud_gui(&mut draw_list, &hud, (width, height), atlas, sheets);
                let mut overlay = Overlay::new(width, height);
                draw_debug_only(&mut overlay, &hud, width, height);
                lines.push(("draw commands", draw_list.len().to_string()));
                lines.push(("overlay vertices", overlay.vertex_count().to_string()));
                Demo {
                    lines,
                    scene: Scene::HudGui(Box::new(hud), draw_list),
                }
            } else {
                let mut overlay = Overlay::new(width, height);
                draw_hud(&mut overlay, &hud, width, height);
                lines.push(("overlay vertices", overlay.vertex_count().to_string()));
                Demo {
                    lines,
                    scene: Scene::Hud(Box::new(hud)),
                }
            }
        }
    }
}

/// Runs the demo: prints the report and, with `--screenshot`, renders it.
///
/// # Errors
/// The renderer or the PNG file failed.
pub fn run(args: &Args) -> Result<(), ClientError> {
    let view = if HUD_VIEWS.contains(&args.hud_view.as_str()) {
        args.hud_view.as_str()
    } else {
        HUD_VIEWS[0]
    };
    let font = ui_font::load();
    let gui_atlas = crate::official::load_gui_atlas();
    let sheets = crate::official::hud_sheets(gui_atlas.as_ref());
    let viewport = (args.width, args.height);
    let Some(path) = args.screenshot.as_deref() else {
        let demo = build(view, viewport.0, viewport.1, &font.atlas, &sheets);
        print!("{}", demo.render(view, viewport, font.source));
        return Ok(());
    };

    let mut renderer = Renderer::new_offscreen(viewport.0, viewport.1)?;
    let installed = ui_font::install(&mut renderer, &font.atlas)?;
    if let Some(atlas) = gui_atlas.as_ref() {
        if !atlas.is_empty() {
            renderer.set_gui_atlas(atlas.width(), atlas.height(), atlas.pixels());
        }
    }
    let source = if installed.used_fallback {
        FontSource::Fallback
    } else {
        font.source
    };
    let demo = build(view, viewport.0, viewport.1, &installed.atlas, &sheets);
    print!("{}", demo.render(view, viewport, source));
    render_scene(&mut renderer, &demo, viewport)?;
    let image = renderer.read_pixels()?;
    write_png(path, &image.rgba, image.width, image.height)?;
    println!(
        "screenshot written: {} ({}x{})",
        path.display(),
        image.width,
        image.height
    );
    Ok(())
}

/// Renders the scene into the renderer's offscreen target.
fn render_scene(
    renderer: &mut Renderer,
    demo: &Demo,
    viewport: (u32, u32),
) -> Result<(), ClientError> {
    let camera = Camera::default();
    let mut overlay = Overlay::new(viewport.0, viewport.1);
    match &demo.scene {
        Scene::Hud(hud) => {
            draw_hud(&mut overlay, hud, viewport.0, viewport.1);
            renderer.render(&camera, &overlay)?;
        }
        Scene::HudGui(hud, draw_list) => {
            draw_debug_only(&mut overlay, hud, viewport.0, viewport.1);
            renderer.render_with_ui(&camera, &overlay, draw_list, &mut umicraft_render::NoIcons)?;
        }
        Scene::Screen(draw_list) => {
            renderer.render_with_ui(&camera, &overlay, draw_list, &mut umicraft_render::NoIcons)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(view: &str) -> String {
        let atlas = FontAtlas::fallback();
        let sheets = HudSheets::default();
        let demo = build(view, 1280, 720, &atlas, &sheets);
        demo.render(view, (1280, 720), FontSource::Fallback)
    }

    #[test]
    fn every_view_builds_and_reports() {
        for view in HUD_VIEWS {
            let text = report(view);
            assert!(
                text.contains(&format!("view              : {view}")),
                "{text}"
            );
            assert!(text.lines().count() > 4, "{view}: {text}");
        }
    }

    #[test]
    fn the_overview_shows_the_server_data_through_the_real_game_code() {
        let text = report("overview");
        assert!(text.contains("chat messages     : 7"), "{text}");
        assert!(
            text.contains("boss              : Ender Dragon (health 0.75)"),
            "{text}"
        );
        assert!(
            text.contains("sidebar           : Kills (4 lines)"),
            "{text}"
        );
        assert!(text.contains("tab list          : 27 players"), "{text}");
        assert!(
            text.contains("title             : ROUND 1 / FIGHT!"),
            "{text}"
        );
        assert!(
            text.contains("action bar        : Now playing: Umicraft theme"),
            "{text}"
        );
    }

    #[test]
    fn the_hidden_score_holder_is_not_on_the_sidebar() {
        // The `#hidden` holder scored 99 but must not show: four lines remain.
        let hud = scene_hud();
        let sidebar = hud.sidebar.expect("a sidebar");
        assert_eq!(sidebar.lines.len(), 4);
        assert!(sidebar
            .lines
            .iter()
            .all(|(name, _)| !name.contains("hidden")));
        // The team prefix decorates the members.
        assert!(sidebar.lines.iter().any(|(name, _)| name == "[R] Steve"));
        // The highest score is on top.
        assert_eq!(sidebar.lines[0].1, 9);
    }

    #[test]
    fn the_tab_list_is_sorted_with_spectators_last_and_uses_display_names() {
        let hud = scene_hud();
        let tab = hud.tab_list.expect("a tab list");
        assert_eq!(tab.entries.last().map(|e| e.game_mode), Some(3));
        assert!(tab.entries.iter().any(|e| e.name == "Bob the Builder"));
        assert_eq!(tab.header, "UMICRAFT DEMO SERVER");
    }

    #[test]
    fn the_chat_view_opens_the_chat_with_its_input() {
        let text = report("chat");
        assert!(text.contains("chat input        : /gamemode cr"), "{text}");
    }

    #[test]
    fn the_controls_view_waits_for_a_key_and_flags_the_conflict() {
        let text = report("controls");
        assert!(text.contains("waiting for a key : HOTBAR 3"), "{text}");
        assert!(text.contains("actions in conflict: 2"), "{text}");
    }

    #[test]
    fn the_options_view_shows_the_edited_values() {
        let text = report("options");
        assert!(text.contains("fov               : 95"), "{text}");
        assert!(text.contains("render distance   : 12"), "{text}");
    }

    #[test]
    fn the_report_is_stable() {
        for view in HUD_VIEWS {
            assert_eq!(report(view), report(view), "{view}");
        }
    }

    /// Renders a view offscreen and counts the pixels that are not the
    /// background, to catch a scene that draws nothing.
    fn drawn_pixels(view: &str) -> usize {
        let mut renderer = Renderer::new_offscreen(640, 360).expect("offscreen renderer");
        let font = ui_font::load();
        let gui_atlas = crate::official::load_gui_atlas();
        let sheets = crate::official::hud_sheets(gui_atlas.as_ref());
        let installed = ui_font::install(&mut renderer, &font.atlas).expect("font");
        if let Some(atlas) = gui_atlas.as_ref() {
            if !atlas.is_empty() {
                renderer.set_gui_atlas(atlas.width(), atlas.height(), atlas.pixels());
            }
        }
        let demo = build(view, 640, 360, &installed.atlas, &sheets);
        render_scene(&mut renderer, &demo, (640, 360)).expect("render");
        let image = renderer.read_pixels().expect("readback");
        let background = [image.rgba[0], image.rgba[1], image.rgba[2]];
        image
            .rgba
            .chunks_exact(4)
            .filter(|pixel| pixel[..3] != background)
            .count()
    }

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn every_scene_draws_something_visible() {
        for view in HUD_VIEWS {
            assert!(drawn_pixels(view) > 500, "{view} drew almost nothing");
        }
    }
}
