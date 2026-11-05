// SPDX-License-Identifier: MIT OR Apache-2.0

//! The state behind the menu screens that surround a session: the title
//! screen, the server list with its add, edit, delete and direct-connect
//! dialogs, the connecting and disconnected screens and the statistics
//! (ROADMAP P10, ADR-0044).
//!
//! [`MenuFlow`] owns the screens, the saved [`ServerBook`], the ping results
//! and the row cache built from both. It draws no more than the current menu
//! screen per frame and answers with a [`MenuCommand`] for what only the
//! window can do (connect, open the options, quit, change the screen). Book
//! edits are applied here. Nothing in this module touches winit, the GPU or a
//! socket of its own, so the flow is unit-tested without a window.

use std::time::Duration;

use umicraft_core::lang::Lang;
use umicraft_ui::menus::{
    ConfirmOutcome, ConfirmScreen, ConnectStage, ConnectingOutcome, ConnectingScreen,
    DisconnectedOutcome, DisconnectedScreen, PanoramaTextures, PanoramaView, RowStatus,
    ServerEditOutcome, ServerEditScreen, ServerListOutcome, ServerListScreen, ServerRow,
    StatsOutcome, StatsScreen, TitleInfo, TitleOutcome, TitleScreen,
};
use umicraft_ui::{FontAtlas, Ui};

use crate::game::TICK;
use crate::menu_rows::{pinging_row, row_status, stat_rows};
use crate::options_screen::Page;
use crate::screen::Screen;
use crate::servers::{parse_address, PingResult, PingTarget, Pinger, ServerAddress, ServerBook};

/// What the window has to do after a menu frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuCommand {
    /// Start a session on this server.
    Connect(ServerAddress),
    /// Open the options on `page`, coming from the current screen.
    OpenOptions {
        /// The page to show first.
        page: Page,
    },
    /// Save the options and leave the program.
    Quit,
    /// Show another screen.
    GoTo(Screen),
}

/// What the window gives the menus each frame.
#[derive(Debug, Clone, Copy)]
pub struct MenuContext<'a> {
    /// The version line of the title screen.
    pub version: &'a str,
    /// The statistics of the last server answer, for [`Screen::Stats`].
    pub stats: &'a [(String, i32)],
    /// The uploaded panorama faces, when the user's assets provide them.
    pub panorama: Option<PanoramaTextures>,
}

/// The result of one menu frame.
#[derive(Debug, Default)]
pub struct MenuFrame {
    /// The command the player's click or key produced.
    pub command: Option<MenuCommand>,
    /// The panorama the title screen wants behind it; the window hands it to
    /// the renderer (ADR-0045).
    pub panorama: Option<PanoramaView>,
}

/// The server entry a dialog is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Edit(usize),
    Delete(usize),
}

/// The menu screens and the data they show.
pub struct MenuFlow {
    title: TitleScreen,
    list: ServerListScreen,
    edit: Option<ServerEditScreen>,
    confirm: Option<ConfirmScreen>,
    disconnected: Option<DisconnectedScreen>,
    stats: StatsScreen,
    book: ServerBook,
    pinger: Option<Pinger>,
    rows: Vec<ServerRow>,
    pending: Option<Pending>,
    last_server: String,
    tick_time: Duration,
}

impl MenuFlow {
    /// Creates the flow around `book`. `pinger` is `None` when its thread
    /// could not start; every row then stays "pinging". `last_server` fills
    /// the direct-connect field and `seed` picks the splash.
    #[must_use]
    pub fn new(book: ServerBook, pinger: Option<Pinger>, last_server: String, seed: u64) -> Self {
        let mut flow = Self {
            title: TitleScreen::new(seed),
            list: ServerListScreen::new(),
            edit: None,
            confirm: None,
            disconnected: None,
            stats: StatsScreen::new(),
            book,
            pinger,
            rows: Vec::new(),
            pending: None,
            last_server,
            tick_time: Duration::ZERO,
        };
        flow.rebuild_rows();
        flow
    }

    /// The address typed on the last direct connection, to save as an option.
    #[must_use]
    pub fn last_server(&self) -> &str {
        &self.last_server
    }

    /// Prepares a screen the window is about to show: the server list pings
    /// every entry anew and the statistics start at the top.
    pub fn enter(&mut self, screen: Screen) {
        match screen {
            Screen::ServerList => self.refresh_pings(),
            Screen::Stats => self.stats = StatsScreen::new(),
            _ => {}
        }
    }

    /// Shows `status` on row `index`, as a ping result would (the demo has
    /// no socket).
    pub(crate) fn set_row_status(&mut self, index: usize, status: RowStatus) {
        if let Some(row) = self.rows.get_mut(index) {
            row.status = status;
        }
    }

    /// Remembers why a session ended, for the disconnected screen.
    pub fn set_disconnected(&mut self, reason: &str) {
        self.disconnected = Some(DisconnectedScreen::new("disconnect.lost", reason));
    }

    /// Advances the animations and the double-click timer by `elapsed`.
    pub fn advance(&mut self, elapsed: Duration) {
        /// A stalled frame must not replay seconds of animation.
        const MAX_TICKS: u32 = 5;
        self.tick_time += elapsed;
        let mut ticks = 0;
        while self.tick_time >= TICK && ticks < MAX_TICKS {
            self.tick_time -= TICK;
            self.title.tick();
            self.list.tick();
            if let Some(edit) = self.edit.as_mut() {
                edit.tick();
            }
            ticks += 1;
        }
        self.tick_time = self.tick_time.min(TICK);
    }

    /// How far the current animation tick has come, from 0 to 1.
    fn partial_tick(&self) -> f32 {
        (self.tick_time.as_secs_f32() / TICK.as_secs_f32()).clamp(0.0, 1.0)
    }

    /// Takes the ping results that arrived since the last frame.
    pub fn poll(&mut self) {
        while let Some(result) = self.pinger.as_ref().and_then(Pinger::poll) {
            self.apply_result(result);
        }
    }

    /// Draws `screen` and returns what the player did. Screens this flow does
    /// not own draw nothing.
    pub fn show(
        &mut self,
        screen: Screen,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        context: &MenuContext<'_>,
    ) -> MenuFrame {
        self.poll();
        let mut panorama = None;
        let command = match screen {
            Screen::Title => self.show_title(ui, atlas, lang, context, &mut panorama),
            Screen::ServerList => self.show_list(ui, atlas, lang),
            Screen::ServerEdit => self.show_edit(ui, atlas, lang),
            Screen::ConfirmDelete => self.show_confirm(ui, atlas, lang),
            Screen::Connecting => Self::show_connecting(ui, atlas, lang),
            Screen::Disconnected => self.show_disconnected(ui, atlas, lang),
            Screen::Stats => self.show_stats(ui, atlas, lang, context),
            _ => None,
        };
        MenuFrame { command, panorama }
    }

    fn show_title(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        context: &MenuContext<'_>,
        panorama: &mut Option<PanoramaView>,
    ) -> Option<MenuCommand> {
        let info = TitleInfo {
            version: context.version,
            panorama: context.panorama,
            partial_tick: self.partial_tick(),
        };
        let (outcome, view) = self.title.show(ui, atlas, lang, &info);
        *panorama = view;
        Some(match outcome? {
            TitleOutcome::Multiplayer => MenuCommand::GoTo(Screen::ServerList),
            TitleOutcome::Options => MenuCommand::OpenOptions { page: Page::Main },
            TitleOutcome::Language => MenuCommand::OpenOptions {
                page: Page::Language,
            },
            TitleOutcome::Quit => MenuCommand::Quit,
        })
    }

    fn show_list(&mut self, ui: &mut Ui, atlas: &FontAtlas, lang: &Lang) -> Option<MenuCommand> {
        let outcome = self.list.show(ui, atlas, lang, &self.rows, None)?;
        match outcome {
            ServerListOutcome::Join(index) => self.join(index),
            ServerListOutcome::DirectConnect => {
                self.edit = Some(ServerEditScreen::direct_connect(
                    &self.last_server,
                    valid_address,
                ));
                self.pending = None;
                Some(MenuCommand::GoTo(Screen::ServerEdit))
            }
            ServerListOutcome::Add => Some(self.begin_add(lang)),
            ServerListOutcome::Edit(index) => self.begin_edit(index),
            ServerListOutcome::Delete(index) => self.begin_delete(index, lang),
            ServerListOutcome::Refresh => {
                self.refresh_pings();
                None
            }
            ServerListOutcome::Cancel => Some(MenuCommand::GoTo(Screen::Title)),
            ServerListOutcome::MoveUp(index) => {
                let moved = self.book.move_up(index);
                self.reorder(moved, index);
                None
            }
            ServerListOutcome::MoveDown(index) => {
                let moved = self.book.move_down(index);
                self.reorder(moved, index);
                None
            }
        }
    }

    fn show_edit(&mut self, ui: &mut Ui, atlas: &FontAtlas, lang: &Lang) -> Option<MenuCommand> {
        let screen = self.edit.as_mut()?;
        let mode = screen.mode();
        match screen.show(ui, atlas, lang)? {
            ServerEditOutcome::Cancel => Some(self.close_edit()),
            ServerEditOutcome::Done { name, address } => self.finish_edit(mode, &name, &address),
        }
    }

    fn show_confirm(&mut self, ui: &mut Ui, atlas: &FontAtlas, lang: &Lang) -> Option<MenuCommand> {
        let outcome = self.confirm.as_ref()?.show(ui, atlas, lang)?;
        self.confirm = None;
        if let (ConfirmOutcome::Yes, Some(Pending::Delete(index))) = (outcome, self.pending) {
            self.delete(index);
        }
        self.pending = None;
        Some(MenuCommand::GoTo(Screen::ServerList))
    }

    fn show_connecting(ui: &mut Ui, atlas: &FontAtlas, lang: &Lang) -> Option<MenuCommand> {
        let outcome =
            ConnectingScreen::new().show(ui, atlas, lang, ConnectStage::Connecting, None)?;
        match outcome {
            ConnectingOutcome::Cancel => Some(MenuCommand::GoTo(Screen::ServerList)),
        }
    }

    fn show_disconnected(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
    ) -> Option<MenuCommand> {
        let screen = self
            .disconnected
            .get_or_insert_with(|| DisconnectedScreen::new("disconnect.lost", ""));
        match screen.show(ui, atlas, lang)? {
            DisconnectedOutcome::BackToList => {
                self.disconnected = None;
                Some(MenuCommand::GoTo(Screen::ServerList))
            }
        }
    }

    fn show_stats(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
        context: &MenuContext<'_>,
    ) -> Option<MenuCommand> {
        let rows = stat_rows(context.stats);
        match self.stats.show(ui, atlas, lang, &rows)? {
            StatsOutcome::Done => Some(MenuCommand::GoTo(Screen::GameMenu)),
        }
    }

    fn join(&self, index: usize) -> Option<MenuCommand> {
        let entry = self.book.get(index)?;
        Some(MenuCommand::Connect(entry.socket_address()))
    }

    fn rebuild_rows(&mut self) {
        self.rows = self.book.iter().map(pinging_row).collect();
    }

    fn refresh_pings(&mut self) {
        self.rebuild_rows();
        let targets = self
            .book
            .iter()
            .enumerate()
            .map(|(id, entry)| PingTarget {
                id,
                address: entry.socket_address(),
            })
            .collect();
        if let Some(pinger) = self.pinger.as_mut() {
            pinger.refresh(targets);
        }
    }

    fn apply_result(&mut self, result: PingResult) {
        if let Some(row) = self.rows.get_mut(result.id) {
            row.status = row_status(&result.outcome);
        }
    }
}

/// Whether the text is a server address the book accepts.
fn valid_address(text: &str) -> bool {
    parse_address(text).is_ok()
}

mod edit;
#[cfg(test)]
mod tests;
