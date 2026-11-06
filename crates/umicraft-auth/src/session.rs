// SPDX-License-Identifier: MIT OR Apache-2.0
//! The validated player session.

use crate::{error::AuthError, secret::Secret};

/// A player identity plus the token proving it to the session server.
///
/// `Debug` is safe to log: the token is a [`Secret`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// Player name, 1 to 16 characters of `[A-Za-z0-9_]`.
    pub username: String,
    /// Profile UUID as 32 lowercase hex digits, no dashes (the form the
    /// session server expects for `selectedProfile`).
    pub uuid: String,
    /// Minecraft access token.
    pub access_token: Secret,
}

impl Session {
    /// Validates and builds a session. A dashed UUID is normalised to the
    /// undashed lowercase form.
    ///
    /// # Errors
    /// [`AuthError::InvalidUsername`] or [`AuthError::InvalidUuid`].
    pub fn new(username: &str, uuid: &str, access_token: Secret) -> Result<Self, AuthError> {
        let valid_name = (1..=16).contains(&username.len())
            && username
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid_name {
            return Err(AuthError::InvalidUsername);
        }
        let uuid: String = uuid.chars().filter(|&c| c != '-').collect();
        if uuid.len() != 32 || !uuid.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AuthError::InvalidUuid);
        }
        Ok(Self {
            username: username.to_owned(),
            uuid: uuid.to_ascii_lowercase(),
            access_token,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "069a79f4-44e9-4726-a5be-fca90e38aaf5";

    #[test]
    fn normalises_dashed_uuid() {
        let s = Session::new("Notch", UUID, Secret::new("t")).unwrap();
        assert_eq!(s.uuid, "069a79f444e94726a5befca90e38aaf5");
    }

    #[test]
    fn rejects_bad_usernames() {
        for name in ["", "seventeen_chars_xx", "bad name", "é"] {
            let r = Session::new(name, UUID, Secret::new("t"));
            assert!(matches!(r, Err(AuthError::InvalidUsername)), "{name}");
        }
    }

    #[test]
    fn rejects_bad_uuids() {
        for id in ["", "1234", "zz9a79f444e94726a5befca90e38aaf5"] {
            let r = Session::new("Notch", id, Secret::new("t"));
            assert!(matches!(r, Err(AuthError::InvalidUuid)), "{id}");
        }
    }

    #[test]
    fn debug_hides_token() {
        let s = Session::new("Notch", UUID, Secret::new("super-secret")).unwrap();
        assert!(!format!("{s:?}").contains("super-secret"));
    }
}
