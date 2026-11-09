// SPDX-License-Identifier: MIT OR Apache-2.0
//! Where account tokens are kept, away from `accounts.toml`.

use std::{collections::HashMap, sync::Mutex};

use crate::{error::AuthError, secret::Secret};

/// Key-value storage for secrets. Keys are `"<account uuid>/<name>"`, for
/// example `"069a.../refresh"`.
pub trait SecretVault: Send + Sync {
    /// Returns the secret stored under `key`, if any.
    ///
    /// # Errors
    /// [`AuthError::Storage`] when the backend fails.
    fn get(&self, key: &str) -> Result<Option<Secret>, AuthError>;

    /// Stores `value` under `key`, replacing any previous one.
    ///
    /// # Errors
    /// [`AuthError::Storage`] when the backend fails.
    fn set(&self, key: &str, value: &Secret) -> Result<(), AuthError>;

    /// Removes `key`; removing a missing key is not an error.
    ///
    /// # Errors
    /// [`AuthError::Storage`] when the backend fails.
    fn remove(&self, key: &str) -> Result<(), AuthError>;
}

/// A vault that lives in memory only: tests, and machines without a keychain.
#[derive(Default)]
pub struct MemoryVault(Mutex<HashMap<String, Secret>>);

impl MemoryVault {
    fn entries(&self) -> std::sync::MutexGuard<'_, HashMap<String, Secret>> {
        // A poisoned lock only means another thread panicked mid-insert; the
        // map itself is still a valid map.
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl SecretVault for MemoryVault {
    fn get(&self, key: &str) -> Result<Option<Secret>, AuthError> {
        Ok(self.entries().get(key).cloned())
    }

    fn set(&self, key: &str, value: &Secret) -> Result<(), AuthError> {
        self.entries().insert(key.to_owned(), value.clone());
        Ok(())
    }

    fn remove(&self, key: &str) -> Result<(), AuthError> {
        self.entries().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_remove_round_trip() {
        let vault = MemoryVault::default();
        assert!(vault.get("a/refresh").unwrap().is_none());
        vault.set("a/refresh", &Secret::new("tok")).unwrap();
        assert_eq!(vault.get("a/refresh").unwrap().unwrap().expose(), "tok");
        vault.remove("a/refresh").unwrap();
        vault.remove("a/refresh").unwrap();
        assert!(vault.get("a/refresh").unwrap().is_none());
    }
}
