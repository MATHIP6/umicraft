// SPDX-License-Identifier: MIT OR Apache-2.0

//! The plain-data views of the social overlays: sidebar, tab list, title,
//! action bar and boss bar.
//!
//! Every text is a list of [`ChatRunView`]s, so the team colours, the server
//! display names and the chat styles reach the renderer intact. The client
//! fills them from the `umicraft-game` HUD frames (`hud::*`).

use crate::hud::ChatRunView;

/// The text of a list of runs without its styles.
#[must_use]
pub fn plain_text(runs: &[ChatRunView]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

/// The boss bar at the top of the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct BossView {
    /// The formatted name drawn above the bar.
    pub name: Vec<ChatRunView>,
    /// The health as a fraction of the maximum; the bar fills
    /// `(int)(health * 183)` pixels of 182.
    pub health: f32,
}

/// The title and subtitle drawn in the middle of the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleView {
    /// The title; empty when there is none.
    pub title: Vec<ChatRunView>,
    /// The subtitle; empty when there is none.
    pub subtitle: Vec<ChatRunView>,
    /// The opacity in `0.0..=1.0`.
    pub alpha: f32,
}

/// The one-line message above the hotbar.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionBarView {
    /// The message.
    pub text: Vec<ChatRunView>,
    /// The opacity in `0.0..=1.0`.
    pub alpha: f32,
}

/// One row of the scoreboard sidebar.
#[derive(Debug, Clone, PartialEq)]
pub struct SidebarLineView {
    /// The holder name with its team prefix and suffix.
    pub name: Vec<ChatRunView>,
    /// The score, drawn in red.
    pub score: i32,
}

/// The scoreboard sidebar: a title and rows, top row first.
#[derive(Debug, Clone, PartialEq)]
pub struct SidebarView {
    /// The objective display name.
    pub title: Vec<ChatRunView>,
    /// The rows.
    pub lines: Vec<SidebarLineView>,
}

/// How the objective of the tab list draws its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabValueStyle {
    /// A yellow number.
    Integer,
    /// Hearts, or `"x.yhp"` when the column is narrow.
    Hearts,
}

/// One player of the tab list.
#[derive(Debug, Clone, PartialEq)]
pub struct TabEntryView {
    /// The name drawn for the player, with its team decoration.
    pub name: Vec<ChatRunView>,
    /// The latency in milliseconds; negative is unknown.
    pub ping: i32,
    /// Whether the player is a spectator (italic, faint, no score).
    pub spectator: bool,
    /// The score of the list objective, when one is displayed.
    pub score: Option<i32>,
    /// The player UUID; it picks the default head when there is no skin.
    pub uuid: u128,
    /// Whether the server sent a skin for the head.
    pub has_skin: bool,
}

/// The tab list in display order, with its header and footer.
#[derive(Debug, Clone, PartialEq)]
pub struct TabListView {
    /// The header; `None` when its text is empty.
    pub header: Option<Vec<ChatRunView>>,
    /// The footer; `None` when its text is empty.
    pub footer: Option<Vec<ChatRunView>>,
    /// Every player. The renderer draws the first 80 but measures all.
    pub entries: Vec<TabEntryView>,
    /// The style of the score column, `None` without a list objective.
    pub value_style: Option<TabValueStyle>,
    /// Whether the head column is drawn (`flag` in `renderPlayerlist`: an
    /// integrated server or an encrypted connection).
    pub show_heads: bool,
}
