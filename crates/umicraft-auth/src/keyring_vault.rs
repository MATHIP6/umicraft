// SPDX-License-Identifier: MIT OR Apache-2.0
//! [`SecretVault`] in the system keychain (Secret Service on Linux, Keychain
//! on macOS, Credential Manager on Windows), through the `keyring` crate.

use keyring::Entry;

use crate::{error::AuthError, secret::Secret, vault::SecretVault};

/// Service name under which all entries are filed.
const SERVICE: &str = "umicraft";

/// Keeps secrets in the operating system's keychain.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyringVault;

impl KeyringVault {
    /// Checks that a keychain is available, so the caller can fall back to a
    /// [`MemoryVault`](crate::vault::MemoryVault) when it is not.
    ///
    /// # Errors
    /// [`AuthError::Storage`] when no keychain backend can start.
    pub fn probe() -> Result<Self, AuthError> {
        entry("probe")?;
        Ok(Self)
    }
}

fn entry(key: &str) -> Result<Entry, AuthError> {
    Entry::new(SERVICE, key).map_err(storage_error)
}

fn storage_error(error: keyring::Error) -> AuthError {
    AuthError::Storage(format!("system keychain: {error}"))
}

impl SecretVault for KeyringVault {
    fn get(&self, key: &str) -> Result<Option<Secret>, AuthError> {
        match entry(key)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(storage_error(error)),
        }
    }

    fn set(&self, key: &str, value: &Secret) -> Result<(), AuthError> {
        entry(key)?
            .set_password(value.expose())
            .map_err(storage_error)
    }

    fn remove(&self, key: &str) -> Result<(), AuthError> {
        match entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(storage_error(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_errors_keep_their_kind_without_values() {
        let err = storage_error(keyring::Error::NoEntry);
        assert!(matches!(err, AuthError::Storage(_)));
    }
}
