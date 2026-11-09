// SPDX-License-Identifier: MIT OR Apache-2.0
//! The metadata of a saved account. Tokens never live here (see [`crate::vault`]).

use serde::{Deserialize, Serialize};

/// How an account proves its identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    /// A name and its derived UUID; joins offline-mode servers only.
    Offline,
    /// A Microsoft account with a Minecraft licence.
    Microsoft,
}

/// A saved account: everything the menu shows, nothing secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// Offline or Microsoft.
    pub kind: AccountKind,
    /// Player name, as validated by [`crate::Session::new`].
    pub username: String,
    /// Profile UUID, 32 lowercase hex digits. It is also the account's key in
    /// the store and in the vault.
    pub uuid: String,
}
