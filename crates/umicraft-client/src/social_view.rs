// SPDX-License-Identifier: MIT OR Apache-2.0

//! The social overlay views of the HUD: sidebar, tab list, title, action bar
//! and boss bar.
//!
//! The game model (`umicraft_game::hud`) decides what is shown; this module
//! only turns its frames into the plain views of `umicraft_render`, so
//! `game.rs` carries no overlay logic (ADR-0036).

use std::sync::OnceLock;

use crate::chat::run_views;
use umicraft_game::chat::LangTable;
use umicraft_game::entity::{BossTracker, EntityStore};
use umicraft_game::hud::boss_bar::BossBar;
use umicraft_game::hud::sidebar::sidebar_frame;
use umicraft_game::hud::tab_list::{tab_list_frame, ValueStyle};
use umicraft_game::hud::titles::{action_bar_frame, title_frame};
use umicraft_game::player_list::PlayerList;
use umicraft_game::scoreboard::Scoreboard;
use umicraft_game::title::{ActionBar, TitleState};
use umicraft_render::{
    ActionBarView, BossView, SidebarLineView, SidebarView, TabEntryView, TabListView,
    TabValueStyle, TitleView,
};

/// The translations of the overlays: the built-in table until the language
/// files are installed.
fn translations() -> &'static LangTable {
    static TABLE: OnceLock<LangTable> = OnceLock::new();
    TABLE.get_or_init(LangTable::builtin)
}

/// The sidebar `own_name` sees.
pub(crate) fn sidebar_view(scoreboard: &Scoreboard, own_name: &str) -> Option<SidebarView> {
    let frame = sidebar_frame(scoreboard, own_name)?;
    Some(SidebarView {
        title: run_views(&frame.title),
        lines: frame
            .lines
            .iter()
            .map(|line| SidebarLineView {
                name: run_views(&line.name),
                score: line.score,
            })
            .collect(),
    })
}

/// The tab list in display order, once the server listed a player.
///
/// `show_heads` is whether the head column is drawn; the reference draws it
/// only on an integrated server or an encrypted connection.
pub(crate) fn tab_list_view(
    list: &PlayerList,
    scoreboard: &Scoreboard,
    show_heads: bool,
) -> Option<TabListView> {
    let frame = tab_list_frame(list, scoreboard, translations())?;
    Some(TabListView {
        header: frame.header.as_deref().map(run_views),
        footer: frame.footer.as_deref().map(run_views),
        entries: frame
            .rows
            .iter()
            .map(|row| TabEntryView {
                name: run_views(&row.name),
                ping: row.ping,
                spectator: row.spectator,
                score: row.score,
                uuid: row.uuid,
                has_skin: row.textures.is_some(),
            })
            .collect(),
        value_style: frame.value_style.map(|style| match style {
            ValueStyle::Integer => TabValueStyle::Integer,
            ValueStyle::Hearts => TabValueStyle::Hearts,
        }),
        show_heads,
    })
}

/// The title and subtitle with their opacity, while they are displayed.
pub(crate) fn title_view(state: &TitleState) -> Option<TitleView> {
    let frame = title_frame(state, translations(), 0.0)?;
    Some(TitleView {
        title: run_views(&frame.title),
        subtitle: run_views(&frame.subtitle),
        alpha: frame.alpha,
    })
}

/// The action bar message with its opacity, while one is displayed.
pub(crate) fn action_bar_view(bar: &ActionBar) -> Option<ActionBarView> {
    let frame = action_bar_frame(bar, translations(), 0.0)?;
    Some(ActionBarView {
        text: run_views(&frame.text),
        alpha: frame.alpha,
    })
}

/// The boss bar, while its countdown runs.
pub(crate) fn boss_view(bar: &BossBar) -> Option<BossView> {
    let frame = bar.frame()?;
    Some(BossView {
        name: run_views(&frame.name),
        health: frame.health_scale,
    })
}

/// Refreshes the boss bar from the entities and counts it down one game
/// tick, like the frame in which a boss renders and `renderBossHealth` runs.
///
/// Returns whether the bar was or is shown, i.e. whether the HUD changed.
pub(crate) fn tick_boss_bar(bar: &mut BossBar, entities: &EntityStore) -> bool {
    let was_visible = bar.is_visible();
    bar.observe(BossTracker::find_boss(entities).as_ref(), translations());
    bar.tick();
    was_visible || bar.is_visible()
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::chat::parse;

    #[test]
    fn unstyled_runs_are_white_and_styled_runs_keep_their_flags() {
        let runs = parse(
            r#"[{"text":"a"},{"text":"b","color":"red","bold":true}]"#,
            &LangTable::default(),
        );
        let views = run_views(&runs);
        assert_eq!(views[0].color, [255, 255, 255]);
        assert_eq!(views[1].color, [0xFF, 0x55, 0x55]);
        assert!(views[1].bold && !views[0].bold);
    }

    #[test]
    fn an_empty_player_list_has_no_view() {
        assert_eq!(
            tab_list_view(&PlayerList::new(), &Scoreboard::new(), true),
            None
        );
    }
}
