// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-frame CPU cost of the HUD (perf 2, step A3): building the draw list,
//! turning it into triangles, and cloning the state, with the chat, the sidebar
//! and the tab list of a busy server. No GPU needed.
//!
//! Run with
//! `cargo test --release -p umicraft-render --test hud_cpu_costs -- --ignored --nocapture`
//! and add the rows to `docs/perf-log.md`.

use std::time::Instant;

use umicraft_render::{
    draw_hud_gui, fallback_atlas, ChatLineView, ChatRunView, HudSheets, HudState, SidebarLineView,
    SidebarView, TabEntryView, TabListView, UiPass,
};
use umicraft_ui::DrawList;

fn run(text: &str) -> ChatRunView {
    ChatRunView {
        text: text.to_owned(),
        color: [255, 255, 255],
        bold: false,
        italic: false,
        underlined: false,
        strikethrough: false,
        obfuscated: false,
        click: None,
        hover: None,
        insertion: None,
    }
}

fn busy() -> HudState {
    let mut hud = HudState {
        now: Some(Instant::now()),
        ..HudState::default()
    };
    for n in 0..100 {
        hud.chat.push(ChatLineView {
            runs: vec![
                run("[Rang] "),
                run(&format!("Player{n}")),
                run(": a message of about forty characters"),
            ],
            received: Instant::now(),
        });
    }
    hud.sidebar = Some(SidebarView {
        title: vec![run("Prisme [Beta]")],
        lines: (0..14)
            .map(|n| SidebarLineView {
                name: vec![run(&format!("line number {n} of the board"))],
                score: n,
            })
            .collect(),
    });
    hud
}

fn tab_list() -> TabListView {
    TabListView {
        header: Some(vec![run("header line one\nline two")]),
        footer: Some(vec![run("footer")]),
        entries: (0..80)
            .map(|n| TabEntryView {
                name: vec![run("[Rang] "), run(&format!("Player{n}"))],
                ping: 40,
                spectator: false,
                score: None,
                uuid: n as u128,
                has_skin: true,
                skin_url: None,
                face: None,
            })
            .collect(),
        value_style: None,
        show_heads: true,
    }
}

fn time(label: &str, hud: &HudState) {
    let font = fallback_atlas();
    let sheets = HudSheets::default();
    let mut pass = UiPass::new(1920, 1080);
    let rounds = 3_000;
    let (mut build, mut tessellate, mut commands) = (0.0, 0.0, 0);
    for _ in 0..rounds {
        let mut list = DrawList::new();
        let a = Instant::now();
        draw_hud_gui(&mut list, hud, (1920, 1080), &font, &sheets);
        let b = Instant::now();
        pass.rebuild(&list, &font);
        let c = Instant::now();
        build += b.duration_since(a).as_secs_f64();
        tessellate += c.duration_since(b).as_secs_f64();
        commands = list.commands().len();
    }
    println!(
        "{label}: {commands} commands, build {:.1} us, tessellate {:.1} us per frame",
        build * 1e6 / f64::from(rounds),
        tessellate * 1e6 / f64::from(rounds)
    );
}

#[test]
#[ignore = "a measurement, run with --release --nocapture"]
fn hud_build_and_tessellation() {
    let full = busy();
    let mut empty = full.clone();
    empty.chat.clear();
    empty.sidebar = None;
    time("empty HUD", &empty);
    let mut sidebar = empty.clone();
    sidebar.sidebar = full.sidebar.clone();
    time("sidebar of 14 lines", &sidebar);
    let mut chat10 = empty.clone();
    chat10.chat = full.chat[..10].to_vec();
    time("chat of 10 lines", &chat10);
    let mut chat100 = empty.clone();
    chat100.chat = full.chat.clone();
    time("chat of 100 lines", &chat100);
    time("chat of 100 lines + sidebar", &full);
    let mut tab = empty.clone();
    tab.tab_list = Some(tab_list());
    tab.show_tab_list = true;
    time("tab list of 80", &tab);
}

#[test]
#[ignore = "a measurement, run with --release --nocapture"]
fn hud_state_clone() {
    let mut hud = busy();
    hud.tab_list = Some(tab_list());
    let rounds = 20_000;
    let start = Instant::now();
    for _ in 0..rounds {
        std::hint::black_box(hud.clone());
    }
    println!(
        "HudState clone (chat 100, sidebar 14, tab list 80): {:.1} us",
        start.elapsed().as_secs_f64() * 1e6 / f64::from(rounds)
    );
}
