// SPDX-License-Identifier: MIT OR Apache-2.0

//! The plain-data description of the HUD: [`HudState`] and the view structs
//! the client fills every frame (chat, title, sidebar, tab list, boss bar).
//!
//! These types carry no behaviour beyond small conversions, so both the
//! layout maths and the renderers depend on them without depending on each
//! other.

use std::time::Instant;

use super::overlay_views::{ActionBarView, BossView, SidebarView, TabListView, TitleView};
use crate::hud_hearts::HeartsView;
use umicraft_game::hud::{vitals_view, BottomBar, HeldNameView, VitalsInput, VitalsView};
use umicraft_ui::SlotItem;

/// Number of hotbar slots drawn by the HUD.
pub const HOTBAR_SLOTS: usize = 9;

/// Number of icons in each status row (hearts, food, armour).
pub const STATUS_ICONS: usize = 10;

/// Maximum number of chat lines the HUD keeps on screen.
pub const CHAT_LINES: usize = 10;

/// Everything the HUD draws, as plain data.
///
/// The client fills this from the game state every frame and hands it to
/// [`draw`]; the HUD itself keeps no state and owns no resources.
#[derive(Debug, Clone, PartialEq)]
pub struct HudState {
    /// Health points, `0..=20`; twenty points fill ten hearts.
    pub health: f32,
    /// Food points, `0..=20`; twenty points fill ten drumsticks.
    pub food: f32,
    /// Armour points, `0..=20`; zero hides the armour row.
    pub armor: f32,
    /// What the hearts show besides the health: absorption, effects, flash.
    pub hearts: HeartsView,
    /// Progress towards the next level, `0..=1`.
    pub experience: f32,
    /// The current experience level, shown on the experience bar.
    pub level: i32,
    /// The selected hotbar slot; values above eight are clamped.
    pub selected_slot: u8,
    /// Whether the crosshair is drawn.
    pub crosshair: bool,
    /// The items of the nine hotbar slots, with their counts and durability.
    pub hotbar_items: [Option<SlotItem>; HOTBAR_SLOTS],
    /// The hearts, armour, food, mount hearts and air; `None` hides them
    /// (creative and spectator).
    pub vitals: Option<VitalsView>,
    /// The experience bar, or the jump bar of a horse.
    pub bar: BottomBar,
    /// The name of the held item while it is highlighted.
    pub held_name: Option<HeldNameView>,
    /// The chat lines, oldest first, with their styles and arrival times.
    pub chat: Vec<ChatLineView>,
    /// Whether the chat input is open: every line is shown at full opacity.
    pub chat_open: bool,
    /// How many rows the open chat is scrolled up from the newest.
    pub chat_scroll: usize,
    /// The chat input line, while the chat is open.
    pub chat_input: Option<ChatInputView>,
    /// The chat options that shape the log.
    pub chat_style: ChatStyle,
    /// The time of this frame; ages the chat lines. `None` freezes the fade.
    pub now: Option<Instant>,
    /// Debug lines, top to bottom.
    pub debug_lines: Vec<String>,
    /// The vanilla F3 screen (P9-g), `None` while F3 is off.
    pub debug_f3: Option<DebugF3View>,
    /// The action bar text and its opacity, when a message is displayed.
    pub action_bar: Option<ActionBarView>,
    /// The title and subtitle, when displayed.
    pub title: Option<TitleView>,
    /// The scoreboard sidebar, when an objective is assigned to it.
    pub sidebar: Option<SidebarView>,
    /// The tab list, filled whether or not the player holds the key.
    pub tab_list: Option<TabListView>,
    /// Whether the player holds the tab-list key.
    pub show_tab_list: bool,
    /// The tab completions the server offered for the open chat line.
    pub completions: Vec<String>,
    /// Counts the completion answers, so a new answer is told from the last.
    pub completions_serial: u64,
    /// The boss bar of a Wither or an Ender Dragon in the world.
    pub boss: Option<BossView>,
    /// The score shown on the death screen (MCP `EntityPlayer.getScore`).
    pub score: i32,
}

/// The two columns of the vanilla F3 screen, as built by
/// `umicraft_game::debug_f3::DebugF3Lines`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebugF3View {
    /// The left column; empty strings are blank rows.
    pub left: Vec<String>,
    /// The right column; may carry the `\u{a7}a` / `\u{a7}c` colour prefixes.
    pub right: Vec<String>,
}

/// The chat options that shape the log (`GameSettings.chat*`), as the
/// fractions the options screen stores: `0.0..=1.0` each.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChatStyle {
    /// `chatOpacity`: the text and band alpha is `opacity * 0.9 + 0.1`.
    pub opacity: f32,
    /// `chatScale`: scales the whole log, `0.0` hides it.
    pub scale: f32,
    /// `chatWidth`: from 40 to 320 pixels.
    pub width: f32,
    /// `chatHeightFocused`: from 20 to 180 pixels while the chat is open.
    pub height_focused: f32,
    /// `chatHeightUnfocused`: from 20 to 180 pixels while it is closed.
    pub height_unfocused: f32,
}

impl Default for ChatStyle {
    /// The vanilla defaults.
    fn default() -> Self {
        Self {
            opacity: 1.0,
            scale: 1.0,
            width: 1.0,
            height_focused: 1.0,
            height_unfocused: 0.444_444_45,
        }
    }
}

impl ChatStyle {
    /// `GuiNewChat.calculateChatboxWidth`, in pixels.
    #[must_use]
    pub fn width_px(&self) -> i32 {
        (self.width * 280.0 + 40.0) as i32
    }

    /// `GuiNewChat.calculateChatboxHeight` for the open chat, in pixels.
    #[must_use]
    pub fn height_focused_px(&self) -> i32 {
        (self.height_focused * 160.0 + 20.0) as i32
    }

    /// `GuiNewChat.calculateChatboxHeight` for the closed chat, in pixels.
    #[must_use]
    pub fn height_unfocused_px(&self) -> i32 {
        (self.height_unfocused * 160.0 + 20.0) as i32
    }
}

/// One styled piece of a chat line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRunView {
    /// The text.
    pub text: String,
    /// The colour as RGB.
    pub color: [u8; 3],
    /// Drawn twice, one pixel apart.
    pub bold: bool,
    /// Not drawn differently: the built-in font cannot be sheared.
    pub italic: bool,
    /// Underlined with a rectangle.
    pub underlined: bool,
    /// Struck through with a rectangle.
    pub strikethrough: bool,
    /// Replaced by glyphs that change every few ticks.
    pub obfuscated: bool,
}

/// A chat message as it arrived, before it is wrapped to the screen width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLineView {
    /// The styled pieces.
    pub runs: Vec<ChatRunView>,
    /// When the message arrived; the fade counts from here.
    pub received: Instant,
}

impl ChatLineView {
    /// The text of the message without its styles.
    #[must_use]
    pub fn plain(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
}

/// The chat input line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatInputView {
    /// The typed text.
    pub text: String,
    /// The caret as a character index.
    pub caret: usize,
    /// Whether the caret is in the visible half of its blink.
    pub caret_visible: bool,
}

impl Default for HudState {
    /// A player at full health and food,
    fn default() -> Self {
        Self {
            health: 20.0,
            food: 20.0,
            armor: 0.0,
            hearts: HeartsView::default(),
            experience: 0.0,
            level: 0,
            selected_slot: 0,
            crosshair: true,
            hotbar_items: Default::default(),
            vitals: Some(vitals_view(&VitalsInput::default())),
            bar: BottomBar::Experience {
                fill: 0,
                level: None,
            },
            held_name: None,
            chat: Vec::new(),
            chat_open: false,
            chat_scroll: 0,
            chat_input: None,
            chat_style: ChatStyle::default(),
            now: None,
            debug_lines: Vec::new(),
            debug_f3: None,
            action_bar: None,
            title: None,
            sidebar: None,
            tab_list: None,
            show_tab_list: false,
            completions: Vec::new(),
            completions_serial: 0,
            boss: None,
            score: 0,
        }
    }
}
