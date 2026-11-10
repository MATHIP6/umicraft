// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed errors of the auth crate. No variant ever carries the access token.

use thiserror::Error;

/// Everything that can go wrong while building or using a session.
#[derive(Debug, Error)]
pub enum AuthError {
    /// The username is not 1 to 16 characters of `[A-Za-z0-9_]`.
    #[error("invalid username: must be 1-16 characters of A-Z, a-z, 0-9 or _")]
    InvalidUsername,
    /// The UUID is not 32 hexadecimal digits (dashes allowed).
    #[error("invalid uuid: must be 32 hexadecimal digits")]
    InvalidUuid,
    /// Only some of the three session values were supplied.
    #[error("incomplete session: missing {}", .missing.join(", "))]
    IncompleteSession {
        /// Names of the missing flags, such as `--uuid`.
        missing: Vec<&'static str>,
    },
    /// The session server answered with something other than 204.
    #[error("session server rejected the join request (HTTP {status})")]
    JoinRejected {
        /// HTTP status code returned by the server.
        status: u16,
    },
    /// The request could not be sent or the connection failed.
    #[error("session server request failed: {0}")]
    Transport(String),
    /// The account file or the secret vault could not be read or written.
    #[error("account storage failed: {0}")]
    Storage(String),
    /// An account with this UUID is already in the store.
    #[error("this account already exists")]
    DuplicateAccount,
    /// No account has this UUID.
    #[error("unknown account")]
    UnknownAccount,
    /// The user refused the sign-in on the Microsoft page.
    #[error("sign-in was declined")]
    LoginDeclined,
    /// The device code or the saved login expired; sign in again.
    #[error("sign-in expired, please sign in again")]
    LoginExpired,
    /// The sign-in was cancelled from the game.
    #[error("sign-in was cancelled")]
    LoginCancelled,
    /// The Microsoft account has no Xbox account.
    #[error("this Microsoft account has no Xbox account")]
    NoXboxAccount,
    /// The account belongs to a child and must be added to a family.
    #[error("this is a child account; it must be added to a Microsoft family")]
    ChildAccount,
    /// Xbox Live is not available in the account's country.
    #[error("Xbox Live is not available in this account's country")]
    XboxUnavailableInCountry,
    /// The account needs adult verification on the Xbox site.
    #[error("this account needs adult verification on the Xbox site")]
    AdultVerificationRequired,
    /// The account is banned from Xbox.
    #[error("this account is banned from Xbox")]
    XboxBanned,
    /// The account does not own Minecraft.
    #[error("this account does not own Minecraft: Java Edition")]
    NoMinecraftLicence,
    /// The account owns Minecraft but has no profile (name) yet.
    #[error("this account has no Minecraft profile yet")]
    NoMinecraftProfile,
    /// A server answered with something unexpected. The text never contains
    /// tokens: only the step and the HTTP status.
    #[error("unexpected answer from the login servers: {0}")]
    Protocol(String),
}
