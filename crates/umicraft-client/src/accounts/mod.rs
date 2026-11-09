// SPDX-License-Identifier: MIT OR Apache-2.0

//! The accounts the player joins servers with (ROADMAP F1, ADR-0067).
//!
//! [`AccountsFlow`] owns the [`AccountManager`], the account-chooser screen and
//! the network jobs. The rest of the client only asks it who to connect as
//! ([`Identity`]) and draws its screen; it touches no socket on the window
//! thread, and a Microsoft session is renewed in the background as soon as the
//! account is chosen, so a click on a server never waits for it.

mod actions;
mod worker;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use umicraft_auth::{
    AccountKind, AccountManager, AuthError, KeyringVault, MemoryVault, SecretVault, Session,
};
use umicraft_core::lang::Lang;
use umicraft_ui::menus::{AccountBadge, AccountRow, AccountsScreen, DeviceCodePrompt};
use umicraft_ui::{FontAtlas, Ui};

use crate::screen::Screen;
use worker::Event;

/// Who the next connection is made as.
pub enum Identity {
    /// No account is saved: the client's own default name, offline.
    Default,
    /// An offline account.
    Offline(String),
    /// A Microsoft account whose session is ready.
    Online(Session),
    /// A Microsoft account whose session is still being renewed, or could not
    /// be: joining would fail with "Invalid session".
    NotReady,
}

/// The state of the selected Microsoft account's session.
enum SessionState {
    /// Nothing to renew (no account, or an offline one).
    Idle,
    Renewing,
    Ready(Session),
    Failed,
}

/// The account manager with its screen and jobs.
pub struct AccountsFlow {
    manager: AccountManager<Box<dyn SecretVault>>,
    screen: AccountsScreen,
    session: SessionState,
    /// The account `session` belongs to.
    session_uuid: Option<String>,
    events: Receiver<Event>,
    sender: Sender<Event>,
    login_cancel: Option<Arc<AtomicBool>>,
}

impl AccountsFlow {
    /// Opens the saved accounts in the data folder and starts renewing the
    /// selected Microsoft session. Without a data folder, or when the file
    /// cannot be read, the accounts live in memory only (logged).
    #[must_use]
    pub fn open_default() -> Self {
        let vault: Box<dyn SecretVault> = match KeyringVault::probe() {
            Ok(keyring) => Box::new(keyring),
            Err(error) => {
                tracing::warn!(%error, "no system keychain: Microsoft sign-ins last for this run");
                Box::new(MemoryVault::default())
            }
        };
        let path = umicraft_assets::cache::data_dir().map(|dir| dir.join("accounts.toml"));
        Self::with_manager(open_manager(path, vault))
    }

    /// A flow that keeps its accounts in memory: tests and demos.
    #[must_use]
    pub fn in_memory() -> Self {
        let vault: Box<dyn SecretVault> = Box::new(MemoryVault::default());
        Self::with_manager(AccountManager::in_memory(vault))
    }

    fn with_manager(manager: AccountManager<Box<dyn SecretVault>>) -> Self {
        let (sender, events) = channel();
        let mut flow = Self {
            manager,
            screen: AccountsScreen::new(Vec::new(), None),
            session: SessionState::Idle,
            session_uuid: None,
            events,
            sender,
            login_cancel: None,
        };
        flow.sync_screen();
        flow.renew_selected();
        flow
    }

    /// Who to connect as.
    #[must_use]
    pub fn identity(&self) -> Identity {
        let Some(account) = self.manager.selected() else {
            return Identity::Default;
        };
        match (&account.kind, &self.session) {
            (AccountKind::Offline, _) => Identity::Offline(account.username.clone()),
            (AccountKind::Microsoft, SessionState::Ready(session)) => {
                Identity::Online(session.clone())
            }
            (AccountKind::Microsoft, _) => Identity::NotReady,
        }
    }

    /// The name shown on the server list's account button.
    #[must_use]
    pub fn active_name(&self) -> Option<String> {
        self.manager.selected().map(|a| a.username.clone())
    }

    /// Tells the screen why a connection could not start as the chosen
    /// account.
    pub fn explain_not_ready(&mut self) {
        let reason = match self.session {
            SessionState::Failed => "Microsoft session expired: sign in again",
            _ => "Microsoft session is still loading, try again in a moment",
        };
        self.screen.set_error(Some(reason.to_owned()));
    }

    /// Applies what the background jobs reported since the last frame.
    pub fn poll(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.apply_event(event);
        }
    }

    /// Draws the chooser and applies what the player did. Returns the screen to
    /// go to when the player leaves it.
    pub fn show(&mut self, ui: &mut Ui, atlas: &FontAtlas, lang: &Lang) -> Option<Screen> {
        self.poll();
        let outcome = self.screen.show(ui, atlas, lang)?;
        self.apply_outcome(outcome)
    }

    /// Advances the screen's animations by one game tick.
    pub fn tick(&mut self) {
        self.screen.tick();
    }

    /// Rebuilds the screen's rows and selection from the manager.
    fn sync_screen(&mut self) {
        let rows = self
            .manager
            .accounts()
            .iter()
            .map(|account| AccountRow {
                username: account.username.clone(),
                badge: match account.kind {
                    AccountKind::Offline => AccountBadge::Offline,
                    AccountKind::Microsoft => AccountBadge::Microsoft,
                },
            })
            .collect();
        self.screen.set_rows(rows);
    }

    fn apply_event(&mut self, event: Event) {
        match event {
            Event::DeviceCode(device) => self.screen.set_device_code(Some(DeviceCodePrompt {
                user_code: device.user_code,
                url: device.verification_uri,
            })),
            Event::LoggedIn(result) => self.finish_login(result),
            Event::Renewed { uuid, result } => self.finish_renewal(&uuid, result),
        }
    }

    fn fail(&mut self, error: &AuthError) {
        tracing::warn!(%error, "account job failed");
        self.screen.set_error(Some(error.to_string()));
    }
}

/// Opens the account file, or an in-memory manager when there is none or it
/// cannot be read (the next change would then replace a damaged file).
fn open_manager(
    path: Option<PathBuf>,
    vault: Box<dyn SecretVault>,
) -> AccountManager<Box<dyn SecretVault>> {
    let Some(path) = path else {
        return AccountManager::in_memory(vault);
    };
    match AccountManager::open(path, vault) {
        Ok(manager) => manager,
        Err(error) => {
            tracing::warn!(%error, "ignoring unreadable accounts file");
            AccountManager::in_memory(Box::new(MemoryVault::default()))
        }
    }
}

#[cfg(test)]
mod tests;
