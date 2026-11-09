// SPDX-License-Identifier: MIT OR Apache-2.0

//! The list view of the chooser: rows, scrolling, keys and the button bar.

use umicraft_core::lang::Lang;

use crate::context::Ui;
use crate::geom::Rect;
use crate::input::Key;
use crate::menus::background::{draw_list_background, draw_list_overlay, draw_menu_background};
use crate::menus::common::{
    draw_scrollbar, escape_pressed, key_pressed, wheel_scrolled, Gui, TextPlacement, DIM_GRAY,
};
use crate::menus::confirm::ConfirmScreen;
use crate::text::FontAtlas;
use crate::widgets::util::last_primary_press;

use super::add_dialog::AddDialog;
use super::edit_dialog::EditDialog;
use super::layout::{LIST_PADDING, LIST_TOP, SLOT_HEIGHT};
use super::row::{draw_row, RowFlags};
use super::{
    account_row_at, account_row_rect, accounts_layout, offset_to_show, AccountBadge,
    AccountsLayout, AccountsOutcome, AccountsScreen, Dialog, DOUBLE_CLICK_TICKS, ERROR_RED,
};

impl AccountsScreen {
    pub(super) fn show_list(
        &mut self,
        ui: &mut Ui,
        atlas: &FontAtlas,
        lang: &Lang,
    ) -> Option<AccountsOutcome> {
        draw_menu_background(ui);
        let viewport = ui.viewport();
        let gui = Gui::new(viewport);
        let layout = accounts_layout(viewport);
        let view = layout.list.height / gui.scale;
        let content = self.rows.len() as f32 * SLOT_HEIGHT + LIST_PADDING;

        let key_outcome = self.keyboard(ui, view);
        self.offset = wheel_scrolled(ui, &gui, layout.list, content, self.offset);
        let bar = [gui.cx() + 154.0, LIST_TOP, view];
        let max = (content - view).max(0.0);
        self.offset = self
            .bar
            .apply(ui, gui.scale, bar, (content, max), self.offset);
        ui.push_id("accounts");
        let list_id = ui.id("rows");
        let clicked = ui.response(list_id, layout.list).clicked && !self.bar.is_active();
        let click = clicked.then(|| last_primary_press(ui)).flatten();
        let outcome = click.and_then(|point| self.click_row(viewport, point));
        draw_list_background(ui, layout.list, self.offset * gui.scale);
        self.draw_rows(ui, (&gui, atlas, lang));
        draw_list_overlay(ui, layout.list);
        draw_scrollbar(ui, &gui, bar, content, self.offset);
        self.draw_header(ui, (&gui, atlas, lang));
        let button = self.buttons(ui, (&gui, atlas, lang), &layout);
        ui.pop_id();

        outcome
            .or(key_outcome)
            .or(button)
            .or_else(|| escape_pressed(ui).then_some(AccountsOutcome::Cancel))
    }

    /// Selects the row at `point` and plays it on a quick second click.
    fn click_row(&mut self, viewport: Rect, point: [f32; 2]) -> Option<AccountsOutcome> {
        let index =
            account_row_at(viewport, point, self.offset).filter(|&i| i < self.rows.len())?;
        self.selected = Some(index);
        let double = self.last_click.is_some_and(|(row, tick)| {
            row == index && self.ticks.wrapping_sub(tick) <= DOUBLE_CLICK_TICKS
        });
        self.last_click = (!double).then_some((index, self.ticks));
        (double && !self.busy).then_some(AccountsOutcome::Play(index))
    }

    /// Up and Down move the selection (Down selects the first row from
    /// nothing) and scroll it into view; Enter plays the selected row.
    fn keyboard(&mut self, ui: &Ui, view: f32) -> Option<AccountsOutcome> {
        let count = self.rows.len();
        let target = if key_pressed(ui, Key::ArrowDown) {
            self.selected.map_or(0, |i| i + 1)
        } else if key_pressed(ui, Key::ArrowUp) {
            self.selected.unwrap_or(count).saturating_sub(1)
        } else {
            return self
                .selected
                .filter(|_| key_pressed(ui, Key::Enter) && !self.busy)
                .map(AccountsOutcome::Play);
        };
        if target < count {
            self.selected = Some(target);
            self.offset = offset_to_show(target, self.offset, view);
        }
        None
    }

    fn draw_rows(&self, ui: &mut Ui, context: (&Gui, &FontAtlas, &Lang)) {
        let viewport = ui.viewport();
        let list = accounts_layout(viewport).list;
        let hovered = ui
            .pointer()
            .filter(|&p| list.contains(p))
            .and_then(|p| account_row_at(viewport, p, self.offset));
        ui.push_clip(list);
        for (index, row) in self.rows.iter().enumerate() {
            let rect = account_row_rect(viewport, index, self.offset);
            if rect.bottom() < list.y || rect.y > list.bottom() {
                continue;
            }
            let at = context.0.to_gui([rect.x, rect.y]);
            let flags = RowFlags {
                selected: self.selected == Some(index),
                hovered: hovered == Some(index),
            };
            draw_row(ui, context, row, at, flags);
        }
        ui.pop_clip();
    }

    /// The title and, below it, the error or the busy text.
    fn draw_header(&self, ui: &mut Ui, (gui, atlas, lang): (&Gui, &FontAtlas, &Lang)) {
        gui.title(ui, atlas, lang.tr("umicraft.accounts.title"), gui.cx(), 8.0);
        let status = match (&self.error, self.busy) {
            (Some(error), _) => {
                Some((lang.tr_args("umicraft.accounts.error", &[error]), ERROR_RED))
            }
            (None, true) => Some((lang.tr("umicraft.accounts.busy").to_string(), DIM_GRAY)),
            (None, false) => None,
        };
        if let Some((text, color)) = status {
            let place = TextPlacement::centered([gui.cx(), 20.0], color);
            gui.text(ui, atlas, &text, place);
        }
    }

    /// Draws the six buttons and maps a click to an outcome or a dialog.
    fn buttons(
        &mut self,
        ui: &mut Ui,
        (gui, atlas, lang): (&Gui, &FontAtlas, &Lang),
        layout: &AccountsLayout,
    ) -> Option<AccountsOutcome> {
        let chosen = self.selected;
        let idle = !self.busy;
        let offline = chosen
            .and_then(|i| self.rows.get(i))
            .is_some_and(|row| row.badge == AccountBadge::Offline);
        let press = |ui: &mut Ui, id: &str, rect, key: &str, enabled: bool| {
            let id = ui.id(id);
            gui.button(ui, id, rect, atlas, lang.tr(key), enabled)
        };
        let play = press(
            ui,
            "play",
            layout.play,
            "umicraft.accounts.play",
            idle && chosen.is_some(),
        );
        let add = press(ui, "add", layout.add, "umicraft.accounts.add", idle);
        let edit = press(
            ui,
            "edit",
            layout.edit,
            "umicraft.accounts.edit",
            idle && offline,
        );
        let delete = press(
            ui,
            "delete",
            layout.delete,
            "umicraft.accounts.delete",
            idle && chosen.is_some(),
        );
        let refresh = press(
            ui,
            "refresh",
            layout.refresh,
            "umicraft.accounts.refresh",
            idle,
        );
        let cancel = press(
            ui,
            "cancel",
            layout.cancel,
            "umicraft.accounts.cancel",
            true,
        );

        let index = chosen.unwrap_or(0);
        if add {
            self.dialog = Dialog::Add(AddDialog::new());
        } else if let Some(row) = self.rows.get(index).filter(|_| edit) {
            self.dialog = Dialog::Edit(EditDialog::new(index, &row.username));
        } else if delete {
            self.dialog = Dialog::Delete {
                index,
                confirm: self.delete_confirm(index, lang),
            };
        }
        [
            (play, AccountsOutcome::Play(index)),
            (refresh, AccountsOutcome::Refresh),
            (cancel, AccountsOutcome::Cancel),
        ]
        .into_iter()
        .find_map(|(clicked, outcome)| clicked.then_some(outcome))
    }

    /// The "delete this account?" question for row `index`.
    fn delete_confirm(&self, index: usize, lang: &Lang) -> ConfirmScreen {
        let name = self.rows.get(index).map_or("", |row| row.username.as_str());
        ConfirmScreen::new(
            "umicraft.accounts.delete.title",
            lang.tr_args("umicraft.accounts.delete.message", &[name]),
            "umicraft.accounts.delete.confirm",
            "gui.cancel",
        )
    }
}
