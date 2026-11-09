// SPDX-License-Identifier: MIT OR Apache-2.0
//! Player session handling for Umicraft.
//!
//! As the vanilla 1.8.8 client, Umicraft receives a session (username, UUID,
//! access token) from the launcher through `--username`, `--uuid` and
//! `--accessToken` (see ADR-0053). This crate parses it ([`launch`]), keeps
//! the token out of logs ([`Secret`]), and performs the session-server `join`
//! call needed by the online-mode handshake ([`join`]).
//!
//! No session means an offline session. F1 (ADR-0067) adds saved accounts
//! ([`account`], [`store`], [`vault`], [`offline`]) and the Microsoft login.

pub mod account;
pub mod error;
pub mod join;
pub mod keyring_vault;
pub mod launch;
pub mod manager;
pub mod microsoft;
pub mod offline;
pub mod reqwest_transport;
pub mod secret;
pub mod session;
pub mod store;
pub mod transport;
pub mod vault;

pub use account::{Account, AccountKind};
pub use error::AuthError;
pub use join::{HttpSessionServer, SessionServer};
pub use keyring_vault::KeyringVault;
pub use launch::{session_from_launch, strip_session_args};
pub use manager::AccountManager;
pub use microsoft::{
    device_code::{poll_token, request_device_code, Sleeper, TokioSleeper},
    login::{full_login, login_from_msa_token, refresh_login},
    DeviceCode, MicrosoftLogin, MsaTokens,
};
pub use offline::{offline_account, offline_uuid};
pub use reqwest_transport::ReqwestTransport;
pub use secret::Secret;
pub use session::Session;
pub use store::AccountStore;
pub use transport::{Body, HttpTransport, Request, Response};
pub use vault::{MemoryVault, SecretVault};
