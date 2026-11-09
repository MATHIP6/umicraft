// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the account chooser's buttons and the finished jobs do to the manager.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use umicraft_auth::{AuthError, MicrosoftLogin, RenewedSession, Session};
use umicraft_ui::menus::AccountsOutcome;

use super::{worker, AccountsFlow, SessionState};
use crate::screen::Screen;

impl AccountsFlow {
    /// Carries out `outcome`; returns the screen to leave for, if any.
    pub(super) fn apply_outcome(&mut self, outcome: AccountsOutcome) -> Option<Screen> {
        self.screen.set_error(None);
        match outcome {
            AccountsOutcome::Play(index) => return self.choose(index),
            AccountsOutcome::Cancel => return Some(Screen::ServerList),
            AccountsOutcome::AddOffline(name) => {
                let added = self.manager.add_offline(&name);
                self.after_change(added);
            }
            AccountsOutcome::AddMicrosoft => self.start_login(),
            AccountsOutcome::EditOffline { index, username } => {
                let renamed = self
                    .uuid_at(index)
                    .and_then(|uuid| self.manager.rename_offline(&uuid, &username));
                self.after_change(renamed);
            }
            AccountsOutcome::Delete(index) => {
                let removed = self
                    .uuid_at(index)
                    .and_then(|uuid| self.manager.remove(&uuid));
                self.after_change(removed);
            }
            AccountsOutcome::Refresh => self.renew_selected(),
            AccountsOutcome::CancelLogin => self.cancel_login(),
        }
        None
    }

    /// Makes the account at `index` the one to join with.
    fn choose(&mut self, index: usize) -> Option<Screen> {
        let selected = self
            .uuid_at(index)
            .and_then(|uuid| self.manager.select(&uuid));
        self.after_change(selected);
        Some(Screen::ServerList)
    }

    /// Asks Microsoft for a session for the selected account, if it needs one.
    pub(super) fn renew_selected(&mut self) {
        let Some(uuid) = self.manager.selected().map(|a| a.uuid.clone()) else {
            self.session = SessionState::Idle;
            self.session_uuid = None;
            return;
        };
        self.session_uuid = Some(uuid.clone());
        match self.manager.refresh_token(&uuid) {
            Ok(None) => self.session = SessionState::Idle,
            Ok(Some(refresh)) => {
                self.session = SessionState::Renewing;
                worker::spawn_renewal(self.sender.clone(), uuid, refresh);
            }
            Err(error) => {
                self.session = SessionState::Failed;
                self.fail(&error);
            }
        }
    }

    fn start_login(&mut self) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.login_cancel = Some(Arc::clone(&cancel));
        self.screen.set_busy(true);
        worker::spawn_login(self.sender.clone(), cancel);
    }

    fn cancel_login(&mut self) {
        if let Some(cancel) = self.login_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.end_login_dialog();
    }

    fn end_login_dialog(&mut self) {
        self.screen.set_device_code(None);
        self.screen.set_busy(false);
    }

    pub(super) fn finish_login(&mut self, result: Result<MicrosoftLogin, AuthError>) {
        self.login_cancel = None;
        self.end_login_dialog();
        let saved = result.and_then(|login| {
            self.manager.add_microsoft(&login)?;
            self.manager.select(&login.uuid)?;
            let session = Session::new(&login.username, &login.uuid, login.minecraft_token)?;
            self.session = SessionState::Ready(session);
            self.session_uuid = Some(login.uuid.clone());
            Ok(())
        });
        // A cancelled sign-in is not an error to show.
        if matches!(saved, Err(AuthError::LoginCancelled)) {
            return;
        }
        self.after_change(saved);
    }

    pub(super) fn finish_renewal(&mut self, uuid: &str, result: Result<RenewedSession, AuthError>) {
        // The answer of an account that is no longer the selected one is stale.
        if self.manager.selected().map(|a| a.uuid.as_str()) != Some(uuid) {
            return;
        }
        match result.and_then(|renewed| {
            self.manager
                .store_refresh_token(uuid, &renewed.refresh_token)?;
            Ok(renewed.session)
        }) {
            Ok(session) => self.session = SessionState::Ready(session),
            Err(error) => {
                self.session = SessionState::Failed;
                self.fail(&error);
            }
        }
    }

    fn uuid_at(&self, index: usize) -> Result<String, AuthError> {
        self.manager
            .accounts()
            .get(index)
            .map(|a| a.uuid.clone())
            .ok_or(AuthError::UnknownAccount)
    }

    /// Refreshes the rows after a change and shows its error, if it failed.
    /// A change of the selected account also gets its session going.
    fn after_change(&mut self, result: Result<(), AuthError>) {
        if let Err(error) = result {
            self.fail(&error);
        }
        self.sync_screen();
        let selected = self.manager.selected().map(|a| a.uuid.as_str());
        if selected != self.session_uuid.as_deref() {
            self.renew_selected();
        }
    }
}
