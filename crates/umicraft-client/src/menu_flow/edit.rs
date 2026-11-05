// SPDX-License-Identifier: MIT OR Apache-2.0

//! The book edits of [`MenuFlow`]: opening the add, edit and delete dialogs
//! for an entry and applying what they report (ADR-0044).

use umicraft_core::lang::Lang;
use umicraft_ui::menus::{ConfirmScreen, EditMode, ServerEditScreen};

use super::{valid_address, MenuCommand, MenuFlow, Pending};
use crate::screen::Screen;
use crate::servers::parse_address;

impl MenuFlow {
    /// Opens the Add Server form with the default name
    /// (`selectServer.defaultName`), as `GuiScreenAddServer` does.
    pub(crate) fn begin_add(&mut self, lang: &Lang) -> MenuCommand {
        let name = lang.tr("selectServer.defaultName");
        self.edit = Some(ServerEditScreen::add(name, valid_address));
        self.pending = None;
        MenuCommand::GoTo(Screen::ServerEdit)
    }
    pub(super) fn begin_edit(&mut self, index: usize) -> Option<MenuCommand> {
        let entry = self.book.get(index)?;
        self.edit = Some(ServerEditScreen::edit(
            entry.name(),
            entry.address(),
            valid_address,
        ));
        self.pending = Some(Pending::Edit(index));
        Some(MenuCommand::GoTo(Screen::ServerEdit))
    }
    pub(super) fn begin_delete(&mut self, index: usize, lang: &Lang) -> Option<MenuCommand> {
        let entry = self.book.get(index)?;
        self.confirm = Some(ConfirmScreen::delete_server(entry.name(), lang));
        self.pending = Some(Pending::Delete(index));
        Some(MenuCommand::GoTo(Screen::ConfirmDelete))
    }
    pub(super) fn close_edit(&mut self) -> MenuCommand {
        self.edit = None;
        self.pending = None;
        MenuCommand::GoTo(Screen::ServerList)
    }
    /// Applies the form: adds or changes an entry, or connects directly.
    pub(super) fn finish_edit(
        &mut self,
        mode: EditMode,
        name: &str,
        address: &str,
    ) -> Option<MenuCommand> {
        let pending = self.pending;
        match (mode, pending) {
            (EditMode::DirectConnect, _) => {
                let target = parse_address(address).ok()?;
                self.last_server = address.trim().to_string();
                self.edit = None;
                Some(MenuCommand::Connect(target))
            }
            (EditMode::Add, _) => {
                let added = self.book.add(name, address);
                if let Ok(index) = added {
                    self.list.set_selected(Some(index));
                }
                self.saved(added.map(|_| ()).map_err(|error| error.to_string()));
                Some(self.close_edit())
            }
            (EditMode::Edit, Some(Pending::Edit(index))) => {
                let edited = self.book.edit(index, name, address);
                self.saved(edited.map_err(|error| error.to_string()));
                Some(self.close_edit())
            }
            (EditMode::Edit, _) => Some(self.close_edit()),
        }
    }
    pub(super) fn delete(&mut self, index: usize) {
        if self.book.remove(index).is_some() {
            self.saved(Ok(()));
        }
    }
    /// Mirrors a swap the book made in the row cache, so the pings already
    /// received stay attached to their servers.
    pub(super) fn reorder(&mut self, moved: Option<usize>, from: usize) {
        let Some(to) = moved else {
            return;
        };
        self.rows.swap(from, to);
        self.persist();
    }
    /// Saves the book after `change` and pings the new list. A failed change
    /// is logged and leaves the book as it was.
    pub(super) fn saved(&mut self, change: Result<(), String>) {
        if let Err(error) = change {
            tracing::warn!(%error, "the server list change was refused");
            return;
        }
        self.persist();
        self.refresh_pings();
    }
    pub(super) fn persist(&self) {
        if let Err(error) = self.book.save() {
            tracing::warn!(%error, "could not save the server list");
        }
    }
}
