// SPDX-License-Identifier: MIT OR Apache-2.0
//! The account manager: the store, the vault and the Microsoft login behind one API.

use std::path::PathBuf;

use crate::{
    account::{Account, AccountKind},
    error::AuthError,
    microsoft::{login::refresh_login, MicrosoftLogin},
    offline::offline_account,
    secret::Secret,
    session::Session,
    store::AccountStore,
    transport::HttpTransport,
    vault::SecretVault,
};

/// Owns the saved accounts and keeps `accounts.toml` and the vault in step.
///
/// Every change is saved at once, so the file never lags behind the menu.
pub struct AccountManager<V: SecretVault> {
    path: PathBuf,
    store: AccountStore,
    vault: V,
}

/// The vault key of an account's Microsoft refresh token.
fn refresh_key(uuid: &str) -> String {
    format!("{uuid}/refresh")
}

impl<V: SecretVault> AccountManager<V> {
    /// Opens the manager on the account file `path` (missing = no accounts).
    ///
    /// # Errors
    /// [`AuthError::Storage`] when the file exists but cannot be read.
    pub fn open(path: PathBuf, vault: V) -> Result<Self, AuthError> {
        let store = AccountStore::load(&path)?;
        Ok(Self { path, store, vault })
    }

    /// The saved accounts, in menu order.
    pub fn accounts(&self) -> &[Account] {
        self.store.accounts()
    }

    /// The account used for the next connection.
    pub fn selected(&self) -> Option<&Account> {
        self.store.selected()
    }

    /// Adds an offline account and selects it if it is the first one.
    ///
    /// # Errors
    /// [`AuthError::InvalidUsername`], [`AuthError::DuplicateAccount`] or
    /// [`AuthError::Storage`].
    pub fn add_offline(&mut self, username: &str) -> Result<(), AuthError> {
        self.store.add(offline_account(username)?)?;
        self.save()
    }

    /// Saves a finished Microsoft login. Signing in again with an account that
    /// is already saved only renews its refresh token.
    ///
    /// # Errors
    /// [`AuthError::Storage`] when the file or the vault cannot be written.
    pub fn add_microsoft(&mut self, login: &MicrosoftLogin) -> Result<(), AuthError> {
        self.vault
            .set(&refresh_key(&login.uuid), &login.refresh_token)?;
        let account = Account {
            kind: AccountKind::Microsoft,
            username: login.username.clone(),
            uuid: login.uuid.clone(),
        };
        match self.store.add(account.clone()) {
            Ok(()) => {}
            Err(AuthError::DuplicateAccount) => self.store.replace(&login.uuid, account)?,
            Err(other) => return Err(other),
        }
        self.save()
    }

    /// Renames an offline account (its UUID follows the name).
    ///
    /// # Errors
    /// [`AuthError::UnknownAccount`], [`AuthError::InvalidUsername`],
    /// [`AuthError::DuplicateAccount`] or [`AuthError::Storage`]; a Microsoft
    /// account cannot be renamed here and gives [`AuthError::UnknownAccount`].
    pub fn rename_offline(&mut self, uuid: &str, username: &str) -> Result<(), AuthError> {
        let is_offline = self
            .store
            .accounts()
            .iter()
            .any(|a| a.uuid == uuid && a.kind == AccountKind::Offline);
        if !is_offline {
            return Err(AuthError::UnknownAccount);
        }
        self.store.replace(uuid, offline_account(username)?)?;
        self.save()
    }

    /// Chooses the account for the next connection.
    ///
    /// # Errors
    /// [`AuthError::UnknownAccount`] or [`AuthError::Storage`].
    pub fn select(&mut self, uuid: &str) -> Result<(), AuthError> {
        self.store.select(uuid)?;
        self.save()
    }

    /// Deletes an account and its refresh token.
    ///
    /// # Errors
    /// [`AuthError::UnknownAccount`] or [`AuthError::Storage`].
    pub fn remove(&mut self, uuid: &str) -> Result<(), AuthError> {
        self.store.remove(uuid)?;
        self.vault.remove(&refresh_key(uuid))?;
        self.save()
    }

    /// The session to join a server with.
    ///
    /// An offline account has none (`Ok(None)`: the client connects as an
    /// unauthenticated player, as today). A Microsoft account is renewed from
    /// its refresh token, and the new refresh token replaces the old one.
    ///
    /// # Errors
    /// [`AuthError::UnknownAccount`]; [`AuthError::LoginExpired`] when the
    /// refresh token is missing or refused (the player must sign in again);
    /// otherwise the errors of the Microsoft chain.
    pub async fn session_for<T: HttpTransport>(
        &self,
        uuid: &str,
        transport: &T,
    ) -> Result<Option<Session>, AuthError> {
        let account = self
            .store
            .accounts()
            .iter()
            .find(|a| a.uuid == uuid)
            .ok_or(AuthError::UnknownAccount)?;
        if account.kind == AccountKind::Offline {
            return Ok(None);
        }
        let refresh = self
            .vault
            .get(&refresh_key(uuid))?
            .ok_or(AuthError::LoginExpired)?;
        let login = refresh_login(transport, &refresh).await?;
        self.vault
            .set(&refresh_key(&login.uuid), &login.refresh_token)?;
        let session = Session::new(
            &login.username,
            &login.uuid,
            Secret::new(login.minecraft_token.expose()),
        )?;
        Ok(Some(session))
    }

    fn save(&self) -> Result<(), AuthError> {
        self.store.save(&self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{microsoft::testing::FakeTransport, vault::MemoryVault};

    fn manager() -> (AccountManager<MemoryVault>, PathBuf) {
        let folder = std::env::temp_dir().join(format!(
            "umicraft-manager-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = folder.join("accounts.toml");
        let _ = std::fs::remove_file(&path);
        (
            AccountManager::open(path.clone(), MemoryVault::default()).unwrap(),
            path,
        )
    }

    fn login() -> MicrosoftLogin {
        MicrosoftLogin {
            username: "Notch".into(),
            uuid: "069a79f444e94726a5befca90e38aaf5".into(),
            minecraft_token: Secret::new("mc"),
            minecraft_expires_in_secs: 86400,
            refresh_token: Secret::new("r1"),
        }
    }

    const MSA: &str = r#"{"access_token":"msa-a","refresh_token":"r2"}"#;
    const XBOX: &str = r#"{"Token":"x","DisplayClaims":{"xui":[{"uhs":"h"}]}}"#;
    const MC: &str = r#"{"access_token":"mc-new","expires_in":86400}"#;
    const ENT: &str = r#"{"items":[{"name":"game_minecraft"}]}"#;
    const PROFILE: &str = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#;

    #[test]
    fn changes_are_saved_and_reloaded() {
        let (mut m, path) = manager();
        m.add_offline("Pixel").unwrap();
        m.add_microsoft(&login()).unwrap();
        let reopened = AccountManager::open(path, MemoryVault::default()).unwrap();
        assert_eq!(reopened.accounts(), m.accounts());
        assert_eq!(reopened.selected().unwrap().username, "Pixel");
    }

    #[test]
    fn signing_in_again_does_not_duplicate_the_account() {
        let (mut m, _) = manager();
        m.add_microsoft(&login()).unwrap();
        m.add_microsoft(&login()).unwrap();
        assert_eq!(m.accounts().len(), 1);
    }

    #[test]
    fn rename_only_applies_to_offline_accounts() {
        let (mut m, _) = manager();
        m.add_offline("A").unwrap();
        m.add_microsoft(&login()).unwrap();
        let (offline, microsoft) = (m.accounts()[0].uuid.clone(), m.accounts()[1].uuid.clone());
        m.rename_offline(&offline, "B").unwrap();
        assert_eq!(m.accounts()[0].username, "B");
        assert!(m.rename_offline(&microsoft, "C").is_err());
    }

    #[test]
    fn removing_an_account_wipes_its_refresh_token() {
        let (mut m, _) = manager();
        m.add_microsoft(&login()).unwrap();
        let uuid = m.accounts()[0].uuid.clone();
        m.remove(&uuid).unwrap();
        assert!(m.vault.get(&refresh_key(&uuid)).unwrap().is_none());
    }

    #[tokio::test]
    async fn an_offline_account_has_no_session() {
        let (mut m, _) = manager();
        m.add_offline("Pixel").unwrap();
        let uuid = m.accounts()[0].uuid.clone();
        let t = FakeTransport::new(vec![]);
        assert!(m.session_for(&uuid, &t).await.unwrap().is_none());
        assert_eq!(t.count(), 0);
    }

    #[tokio::test]
    async fn a_microsoft_session_renews_and_rotates_the_refresh_token() {
        let (mut m, _) = manager();
        m.add_microsoft(&login()).unwrap();
        let uuid = m.accounts()[0].uuid.clone();
        let t = FakeTransport::new(vec![
            (200, MSA),
            (200, XBOX),
            (200, XBOX),
            (200, MC),
            (200, ENT),
            (200, PROFILE),
        ]);
        let session = m.session_for(&uuid, &t).await.unwrap().unwrap();
        assert_eq!(session.access_token.expose(), "mc-new");
        let stored = m.vault.get(&refresh_key(&uuid)).unwrap().unwrap();
        assert_eq!(stored.expose(), "r2");
    }

    #[tokio::test]
    async fn a_missing_refresh_token_asks_for_a_new_login() {
        let (mut m, _) = manager();
        m.add_microsoft(&login()).unwrap();
        let uuid = m.accounts()[0].uuid.clone();
        m.vault.remove(&refresh_key(&uuid)).unwrap();
        let t = FakeTransport::new(vec![]);
        let error = m.session_for(&uuid, &t).await.err().unwrap();
        assert!(matches!(error, AuthError::LoginExpired));
    }
}
