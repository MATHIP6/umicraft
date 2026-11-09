// SPDX-License-Identifier: MIT OR Apache-2.0
//! Offline accounts and their standard UUID.

use md5::{Digest, Md5};

use crate::{
    account::{Account, AccountKind},
    error::AuthError,
    secret::Secret,
    session::Session,
};

/// The UUID a vanilla server derives for an unauthenticated player: the
/// version 3 (MD5) UUID of `"OfflinePlayer:" + name`, undashed lowercase hex.
///
/// Source: `EntityPlayer.getOfflineUUID` in the MCP reference
/// (`mcp/src/minecraft/net/minecraft/entity/player/EntityPlayer.java`).
pub fn offline_uuid(username: &str) -> String {
    let mut bytes: [u8; 16] = Md5::digest(format!("OfflinePlayer:{username}")).into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Builds the offline account for `username`.
///
/// # Errors
/// [`AuthError::InvalidUsername`] when the name is not valid.
pub fn offline_account(username: &str) -> Result<Account, AuthError> {
    let uuid = offline_uuid(username);
    // Reuse the session validation so the two rules cannot drift apart.
    Session::new(username, &uuid, Secret::new(""))?;
    Ok(Account {
        kind: AccountKind::Offline,
        username: username.to_owned(),
        uuid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_known_vanilla_uuid() {
        // Value logged by vanilla servers for an offline "Notch".
        assert_eq!(offline_uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    }

    #[test]
    fn rejects_an_invalid_name() {
        assert!(matches!(
            offline_account("bad name"),
            Err(AuthError::InvalidUsername)
        ));
    }

    #[test]
    fn builds_an_offline_account() {
        let account = offline_account("Pixel").unwrap();
        assert_eq!(account.kind, AccountKind::Offline);
        assert_eq!(account.uuid, offline_uuid("Pixel"));
    }
}
