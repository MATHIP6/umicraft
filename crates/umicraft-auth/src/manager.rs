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
    path: Option<PathBuf>,
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
        Ok(Self {
            path: Some(path),
            store,
            vault,
        })
    }

    /// A manager that saves nothing to disk: tests, and a client without a
    /// data folder.
    pub fn in_memory(vault: V) -> Self {
        Self {
            path: None,
            store: AccountStore::default(),
            vault,
        }
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

    /// The refresh token to renew `uuid`'s session with.
    ///
    /// An offline account has none (`Ok(None)`: the client connects as an
    /// unauthenticated player). The caller runs [`renew_session`] with the
    /// token, off the thread that owns the manager, then hands the result to
    /// [`AccountManager::store_refresh_token`].
    ///
    /// # Errors
    /// [`AuthError::UnknownAccount`]; [`AuthError::LoginExpired`] when a
    /// Microsoft account has no refresh token left (the player must sign in
    /// again); [`AuthError::Storage`] when the vault fails.
    pub fn refresh_token(&self, uuid: &str) -> Result<Option<Secret>, AuthError> {
        let account = self
            .store
            .accounts()
            .iter()
            .find(|a| a.uuid == uuid)
            .ok_or(AuthError::UnknownAccount)?;
        if account.kind == AccountKind::Offline {
            return Ok(None);
        }
        self.vault
            .get(&refresh_key(uuid))?
            .map(Some)
            .ok_or(AuthError::LoginExpired)
    }

    /// Keeps the refresh token a renewal returned: Microsoft rotates it, and
    /// the old one stops working.
    ///
    /// # Errors
    /// [`AuthError::Storage`] when the vault fails.
    pub fn store_refresh_token(&self, uuid: &str, token: &Secret) -> Result<(), AuthError> {
        self.vault.set(&refresh_key(uuid), token)
    }

    fn save(&self) -> Result<(), AuthError> {
        self.path
            .as_deref()
            .map_or(Ok(()), |path| self.store.save(path))
    }
}

/// A renewed Minecraft session and the refresh token that replaces the old one.
pub struct RenewedSession {
    /// The session to join a server with.
    pub session: Session,
    /// The new refresh token, for [`AccountManager::store_refresh_token`].
    pub refresh_token: Secret,
}

/// Signs in again from a refresh token: Microsoft, Xbox, Minecraft, profile.
///
/// # Errors
/// [`AuthError::LoginExpired`] when Microsoft refuses the token, otherwise
/// the errors of the Microsoft chain.
pub async fn renew_session<T: HttpTransport>(
    transport: &T,
    refresh: &Secret,
) -> Result<RenewedSession, AuthError> {
    let login = refresh_login(transport, refresh).await?;
    let session = Session::new(
        &login.username,
        &login.uuid,
        Secret::new(login.minecraft_token.expose()),
    )?;
    Ok(RenewedSession {
        session,
        refresh_token: login.refresh_token,
    })
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
    fn an_in_memory_manager_writes_no_file() {
        let mut m = AccountManager::in_memory(MemoryVault::default());
        m.add_offline("Pixel").unwrap();
        assert_eq!(m.accounts().len(), 1);
    }

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

    #[test]
    fn an_offline_account_has_no_refresh_token() {
        let (mut m, _) = manager();
        m.add_offline("Pixel").unwrap();
        let uuid = m.accounts()[0].uuid.clone();
        assert!(m.refresh_token(&uuid).unwrap().is_none());
    }

    #[tokio::test]
    async fn a_renewed_session_carries_the_rotated_refresh_token() {
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
        let refresh = m.refresh_token(&uuid).unwrap().unwrap();
        assert_eq!(refresh.expose(), "r1");
        let renewed = renew_session(&t, &refresh).await.unwrap();
        assert_eq!(renewed.session.access_token.expose(), "mc-new");
        m.store_refresh_token(&uuid, &renewed.refresh_token)
            .unwrap();
        let stored = m.vault.get(&refresh_key(&uuid)).unwrap().unwrap();
        assert_eq!(stored.expose(), "r2");
    }

    #[test]
    fn a_missing_refresh_token_asks_for_a_new_login() {
        let (mut m, _) = manager();
        m.add_microsoft(&login()).unwrap();
        let uuid = m.accounts()[0].uuid.clone();
        m.vault.remove(&refresh_key(&uuid)).unwrap();
        let error = m.refresh_token(&uuid).err().unwrap();
        assert!(matches!(error, AuthError::LoginExpired));
    }
}
