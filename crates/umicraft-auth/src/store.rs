// SPDX-License-Identifier: MIT OR Apache-2.0
//! The account list and the selected account, saved as `accounts.toml`.

use std::{fs, io, path::Path};

use serde::{Deserialize, Serialize};

use crate::{account::Account, error::AuthError};

/// Saved accounts. Order is the order shown in the menu.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountStore {
    /// UUID of the account used for the next connection.
    #[serde(default)]
    selected: Option<String>,
    #[serde(default, rename = "account")]
    accounts: Vec<Account>,
}

impl AccountStore {
    /// Reads the store; a missing file is an empty store.
    ///
    /// # Errors
    /// [`AuthError::Storage`] when the file exists but cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, AuthError> {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|e| AuthError::Storage(e.to_string())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(AuthError::Storage(e.to_string())),
        }
    }

    /// Writes the store through a temporary file so a crash cannot truncate it.
    ///
    /// # Errors
    /// [`AuthError::Storage`] on any I/O or serialisation failure.
    pub fn save(&self, path: &Path) -> Result<(), AuthError> {
        let storage = |e: &dyn std::fmt::Display| AuthError::Storage(e.to_string());
        let text = toml::to_string_pretty(self).map_err(|e| storage(&e))?;
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder).map_err(|e| storage(&e))?;
        }
        let temporary = path.with_extension("toml.tmp");
        fs::write(&temporary, text).map_err(|e| storage(&e))?;
        fs::rename(&temporary, path).map_err(|e| storage(&e))
    }

    /// The accounts in menu order.
    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    /// The selected account, if one is selected and still exists.
    pub fn selected(&self) -> Option<&Account> {
        let uuid = self.selected.as_deref()?;
        self.accounts.iter().find(|a| a.uuid == uuid)
    }

    /// Adds `account`; the first account added becomes the selected one.
    ///
    /// # Errors
    /// [`AuthError::DuplicateAccount`] when its UUID is already saved.
    pub fn add(&mut self, account: Account) -> Result<(), AuthError> {
        if self.accounts.iter().any(|a| a.uuid == account.uuid) {
            return Err(AuthError::DuplicateAccount);
        }
        if self.selected.is_none() {
            self.selected = Some(account.uuid.clone());
        }
        self.accounts.push(account);
        Ok(())
    }

    /// Selects the account with `uuid`.
    ///
    /// # Errors
    /// [`AuthError::UnknownAccount`] when there is none.
    pub fn select(&mut self, uuid: &str) -> Result<(), AuthError> {
        if !self.accounts.iter().any(|a| a.uuid == uuid) {
            return Err(AuthError::UnknownAccount);
        }
        self.selected = Some(uuid.to_owned());
        Ok(())
    }

    /// Removes the account with `uuid`; if it was selected, the first
    /// remaining account (if any) becomes selected.
    ///
    /// # Errors
    /// [`AuthError::UnknownAccount`] when there is none.
    pub fn remove(&mut self, uuid: &str) -> Result<Account, AuthError> {
        let index = self
            .accounts
            .iter()
            .position(|a| a.uuid == uuid)
            .ok_or(AuthError::UnknownAccount)?;
        let removed = self.accounts.remove(index);
        if self.selected.as_deref() == Some(uuid) {
            self.selected = self.accounts.first().map(|a| a.uuid.clone());
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offline::offline_account;

    fn store_with(names: &[&str]) -> AccountStore {
        let mut store = AccountStore::default();
        for name in names {
            store.add(offline_account(name).unwrap()).unwrap();
        }
        store
    }

    #[test]
    fn first_account_is_selected_and_duplicates_are_refused() {
        let mut store = store_with(&["A", "B"]);
        assert_eq!(store.selected().unwrap().username, "A");
        let again = offline_account("A").unwrap();
        assert!(matches!(store.add(again), Err(AuthError::DuplicateAccount)));
    }

    #[test]
    fn removing_the_selected_account_selects_the_first_remaining() {
        let mut store = store_with(&["A", "B"]);
        let a = store.selected().unwrap().uuid.clone();
        store.remove(&a).unwrap();
        assert_eq!(store.selected().unwrap().username, "B");
        assert!(matches!(store.remove(&a), Err(AuthError::UnknownAccount)));
    }

    #[test]
    fn select_rejects_an_unknown_uuid() {
        let mut store = store_with(&["A"]);
        assert!(matches!(
            store.select("nope"),
            Err(AuthError::UnknownAccount)
        ));
    }

    #[test]
    fn save_then_load_round_trips_and_a_missing_file_is_empty() {
        let folder = std::env::temp_dir().join(format!("umicraft-store-{}", std::process::id()));
        let path = folder.join("accounts.toml");
        assert_eq!(AccountStore::load(&path).unwrap(), AccountStore::default());
        let store = store_with(&["A", "B"]);
        store.save(&path).unwrap();
        assert_eq!(AccountStore::load(&path).unwrap(), store);
        fs::remove_dir_all(folder).unwrap();
    }
}
