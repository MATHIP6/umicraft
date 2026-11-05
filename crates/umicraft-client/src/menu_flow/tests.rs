// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the menu flow: command emission per button, book edits and the
//! mapping of ping results to rows.

use umicraft_ui::menus::{
    confirm_layout, connecting_cancel_rect, server_edit_layout, server_list_layout,
    stats_done_rect, title_layout, ConnectStage, EditMode, RowStatus,
};
use umicraft_ui::{InputEvent, Key, PointerButton, Rect, UiInput};

use super::*;
use crate::dials::DialFrames;
use crate::servers::{PingOutcome, ServerEntry, ServerInfo};

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 1280.0,
    height: 720.0,
};

fn book_in(name: &str) -> (ServerBook, std::path::PathBuf) {
    let folder = std::env::temp_dir().join(format!("umicraft-menu-flow-{name}"));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("temp folder");
    let path = folder.join("servers.toml");
    (ServerBook::open(&path), path)
}

fn flow_with(name: &str, entries: &[(&str, &str)]) -> (MenuFlow, std::path::PathBuf) {
    let (mut book, path) = book_in(name);
    for (entry_name, address) in entries {
        book.add(entry_name, address).expect("valid entry");
    }
    (MenuFlow::new(book, None, String::new(), 0), path)
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

fn frame(flow: &mut MenuFlow, screen: Screen, events: Vec<InputEvent>) -> MenuFrame {
    frame_with(flow, screen, events, &[])
}

fn frame_with(
    flow: &mut MenuFlow,
    screen: Screen,
    events: Vec<InputEvent>,
    stats: &[(String, i32)],
) -> MenuFrame {
    let mut ui = Ui::new(VIEWPORT);
    ui.begin_frame(VIEWPORT, UiInput::new(events));
    let context = MenuContext {
        version: "test",
        connect_stage: ConnectStage::Connecting,
        stats_revision: 1,
        stats_loading: false,
        items: ItemView::new(None, DialFrames::default()),
        inventory_key: "E",
        player_hash: 0,
        stats,
        panorama: None,
    };
    let result = flow.show(
        screen,
        &mut ui,
        &FontAtlas::fallback(),
        &umicraft_core::lang::Lang::builtin(),
        &context,
    );
    ui.end_frame();
    result
}

/// Draws once to lay the widgets out, then clicks `rect` in the next frame.
fn click_on(flow: &mut MenuFlow, screen: Screen, rect: Rect) -> Option<MenuCommand> {
    frame(flow, screen, vec![]);
    frame(flow, screen, click(rect)).command
}

fn info() -> Box<ServerInfo> {
    Box::new(ServerInfo {
        motd: "Hi".to_string(),
        players_online: 1,
        players_max: 2,
        version_name: "1.8.8".to_string(),
        protocol: Some(47),
        latency_ms: 10,
        favicon_png: None,
        player_sample: Vec::new(),
    })
}

#[test]
fn the_title_buttons_emit_their_commands() {
    let layout = title_layout(VIEWPORT);
    let (mut flow, _) = flow_with("title", &[]);
    let cases = [
        (layout.multiplayer, MenuCommand::GoTo(Screen::ServerList)),
        (
            layout.options,
            MenuCommand::OpenOptions { page: Page::Main },
        ),
        (
            layout.language,
            MenuCommand::OpenOptions {
                page: Page::Language,
            },
        ),
        (layout.quit, MenuCommand::Quit),
    ];
    for (rect, expected) in cases {
        assert_eq!(click_on(&mut flow, Screen::Title, rect), Some(expected));
    }
}

#[test]
fn singleplayer_is_disabled() {
    let (mut flow, _) = flow_with("single", &[]);
    let rect = title_layout(VIEWPORT).singleplayer;
    assert_eq!(click_on(&mut flow, Screen::Title, rect), None);
}

#[test]
fn a_screen_the_flow_does_not_own_draws_nothing() {
    let (mut flow, _) = flow_with("other", &[]);
    let result = frame(&mut flow, Screen::GameMenu, vec![]);
    assert_eq!(result.command, None);
}

#[test]
fn joining_the_selected_server_connects() {
    let (mut flow, _) = flow_with("join", &[("One", "one.example.net"), ("Two", "two:25570")]);
    flow.list.set_selected(Some(1));
    let layout = server_list_layout(VIEWPORT);
    let command = click_on(&mut flow, Screen::ServerList, layout.join);
    assert_eq!(
        command,
        Some(MenuCommand::Connect(ServerAddress {
            host: "two".to_string(),
            port: 25570
        }))
    );
}

#[test]
fn the_list_buttons_open_the_dialogs_and_cancel_goes_to_the_title() {
    let layout = server_list_layout(VIEWPORT);
    let (mut flow, _) = flow_with("dialogs", &[("One", "one.example.net")]);
    flow.list.set_selected(Some(0));
    for (rect, screen) in [
        (layout.add, Screen::ServerEdit),
        (layout.direct, Screen::ServerEdit),
        (layout.edit, Screen::ServerEdit),
        (layout.delete, Screen::ConfirmDelete),
        (layout.cancel, Screen::Title),
    ] {
        let command = click_on(&mut flow, Screen::ServerList, rect);
        assert_eq!(command, Some(MenuCommand::GoTo(screen)));
    }
}

#[test]
fn edit_and_delete_buttons_need_a_selection() {
    let layout = server_list_layout(VIEWPORT);
    let (mut flow, _) = flow_with("noselect", &[("One", "one.example.net")]);
    assert_eq!(click_on(&mut flow, Screen::ServerList, layout.edit), None);
    assert_eq!(click_on(&mut flow, Screen::ServerList, layout.delete), None);
}

#[test]
fn adding_a_server_saves_it_and_returns_to_the_list() {
    let (mut flow, path) = flow_with("add", &[]);
    let command = flow.finish_edit(EditMode::Add, "Home", "home.example.net:25570");
    assert_eq!(command, Some(MenuCommand::GoTo(Screen::ServerList)));
    assert_eq!(flow.rows.len(), 1);
    assert_eq!(flow.rows[0].name, "Home");
    assert_eq!(flow.list.selected(), Some(0));
    assert_eq!(ServerBook::open(&path).len(), 1);
}

#[test]
fn the_add_form_starts_with_the_default_server_name() {
    let (mut flow, path) = flow_with("add-default", &[]);
    flow.begin_add(&Lang::builtin());
    // One `Ui` for all frames: the focus lives in it.
    let mut ui = Ui::new(VIEWPORT);
    let atlas = FontAtlas::fallback();
    let context = MenuContext {
        version: "test",
        connect_stage: ConnectStage::Connecting,
        stats_revision: 1,
        stats_loading: false,
        items: ItemView::new(None, DialFrames::default()),
        inventory_key: "E",
        player_hash: 0,
        stats: &[],
        panorama: None,
    };
    let inputs = [
        vec![],
        click(server_edit_layout(VIEWPORT, EditMode::Add).address),
        vec![InputEvent::Text("play.example.net".to_string())],
        vec![InputEvent::KeyPressed(Key::Enter)],
    ];
    let mut command = None;
    for events in inputs {
        ui.begin_frame(VIEWPORT, UiInput::new(events));
        command = flow
            .show(
                Screen::ServerEdit,
                &mut ui,
                &atlas,
                &Lang::builtin(),
                &context,
            )
            .command;
        ui.end_frame();
    }
    assert_eq!(command, Some(MenuCommand::GoTo(Screen::ServerList)));
    let saved = ServerBook::open(&path);
    assert_eq!(
        saved.get(0).map(ServerEntry::name),
        Some("Minecraft Server")
    );
}

#[test]
fn editing_changes_the_pending_entry_only() {
    let (mut flow, path) = flow_with("edit", &[("One", "one"), ("Two", "two")]);
    flow.begin_edit(1);
    let command = flow.finish_edit(EditMode::Edit, "Second", "second.example.net");
    assert_eq!(command, Some(MenuCommand::GoTo(Screen::ServerList)));
    let saved = ServerBook::open(&path);
    assert_eq!(saved.get(0).map(ServerEntry::name), Some("One"));
    assert_eq!(saved.get(1).map(ServerEntry::name), Some("Second"));
    assert_eq!(
        saved.get(1).map(ServerEntry::address),
        Some("second.example.net")
    );
}

#[test]
fn deleting_asks_first_and_cancelling_keeps_the_entry() {
    let layout = confirm_layout(VIEWPORT);
    let (mut flow, path) = flow_with("delete", &[("One", "one"), ("Two", "two")]);
    flow.begin_delete(0, &umicraft_core::lang::Lang::builtin());
    let no = click_on(&mut flow, Screen::ConfirmDelete, layout.no);
    assert_eq!(no, Some(MenuCommand::GoTo(Screen::ServerList)));
    assert_eq!(flow.rows.len(), 2);

    flow.begin_delete(0, &umicraft_core::lang::Lang::builtin());
    let yes = click_on(&mut flow, Screen::ConfirmDelete, layout.yes);
    assert_eq!(yes, Some(MenuCommand::GoTo(Screen::ServerList)));
    assert_eq!(flow.rows.len(), 1);
    assert_eq!(flow.rows[0].name, "Two");
    assert_eq!(ServerBook::open(&path).len(), 1);
}

#[test]
fn reordering_swaps_the_rows_and_the_saved_order() {
    let (mut flow, path) = flow_with("order", &[("One", "one"), ("Two", "two")]);
    flow.apply_result(PingResult {
        id: 1,
        generation: 0,
        outcome: PingOutcome::Online(info()),
    });
    let moved = flow.book.move_up(1);
    flow.reorder(moved, 1);
    assert_eq!(flow.rows[0].name, "Two");
    assert!(matches!(flow.rows[0].status, RowStatus::Online { .. }));
    assert_eq!(flow.rows[1].status, RowStatus::Pinging);
    assert_eq!(
        ServerBook::open(&path).get(0).map(ServerEntry::name),
        Some("Two")
    );
    flow.reorder(None, 0);
    assert_eq!(flow.rows[0].name, "Two");
}

#[test]
fn a_direct_connection_remembers_its_address() {
    let (mut flow, _) = flow_with("direct", &[]);
    let command = flow.finish_edit(EditMode::DirectConnect, "", " play.example.net:25570 ");
    assert_eq!(
        command,
        Some(MenuCommand::Connect(ServerAddress {
            host: "play.example.net".to_string(),
            port: 25570
        }))
    );
    assert_eq!(flow.last_server(), "play.example.net:25570");
    assert_eq!(
        flow.finish_edit(EditMode::DirectConnect, "", "bad host!"),
        None
    );
    assert_eq!(flow.last_server(), "play.example.net:25570");
}

#[test]
fn ping_results_fill_the_matching_row() {
    let (mut flow, _) = flow_with("ping", &[("One", "one"), ("Two", "two")]);
    flow.apply_result(PingResult {
        id: 1,
        generation: 1,
        outcome: PingOutcome::Unreachable("no route".to_string()),
    });
    assert_eq!(flow.rows[0].status, RowStatus::Pinging);
    assert_eq!(flow.rows[1].status, RowStatus::Unreachable);
    // A result for a row that no longer exists is ignored.
    flow.apply_result(PingResult {
        id: 9,
        generation: 1,
        outcome: PingOutcome::Online(info()),
    });
    assert_eq!(flow.rows.len(), 2);
}

#[test]
fn entering_the_list_starts_every_ping_again() {
    let (mut flow, _) = flow_with("enter", &[("One", "one")]);
    flow.apply_result(PingResult {
        id: 0,
        generation: 0,
        outcome: PingOutcome::Unreachable(String::new()),
    });
    flow.enter(Screen::ServerList);
    assert_eq!(flow.rows[0].status, RowStatus::Pinging);
}

#[test]
fn connecting_cancel_and_the_disconnect_button_lead_back_to_the_list() {
    let (mut flow, _) = flow_with("back", &[]);
    let cancel = connecting_cancel_rect(VIEWPORT);
    assert_eq!(
        click_on(&mut flow, Screen::Connecting, cancel),
        Some(MenuCommand::GoTo(Screen::ServerList))
    );
    flow.set_disconnected(&ConnectionEnd::kicked("Kicked: flying"), &Lang::builtin());
    let button = DisconnectedScreen::button_rect(VIEWPORT, 1);
    assert_eq!(
        click_on(&mut flow, Screen::Disconnected, button),
        Some(MenuCommand::GoTo(Screen::ServerList))
    );
}

#[test]
fn the_statistics_done_button_returns_to_the_game_menu() {
    let (mut flow, _) = flow_with("stats", &[]);
    let stats = [("stat.jump".to_string(), 3)];
    frame_with(&mut flow, Screen::Stats, vec![], &stats);
    let result = frame_with(
        &mut flow,
        Screen::Stats,
        click(stats_done_rect(VIEWPORT)),
        &stats,
    );
    assert_eq!(result.command, Some(MenuCommand::GoTo(Screen::GameMenu)));
}

#[test]
fn the_animation_clock_runs_in_whole_ticks_and_caps_a_stall() {
    let (mut flow, _) = flow_with("tick", &[]);
    flow.advance(TICK / 2);
    assert_eq!(flow.tick_time, TICK / 2);
    flow.advance(TICK * 100);
    assert!(flow.tick_time <= TICK);
}
