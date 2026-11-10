// SPDX-License-Identifier: MIT OR Apache-2.0
//! Microsoft sign-in for Minecraft: Java Edition (ADR-0067).
//!
//! The chain, one step per module, all generic over [`HttpTransport`]:
//!
//! 1. [`device_code`]: the player enters a short code on a Microsoft page
//!    (OAuth 2.0 device authorization grant) and we poll for an MSA token.
//! 2. [`refresh`]: later runs trade the saved refresh token for a new one.
//! 3. [`xbox`]: MSA token -> Xbox Live user token.
//! 4. [`xsts`]: user token -> XSTS token for the Minecraft services.
//! 5. [`minecraft`]: XSTS token -> Minecraft access token, ownership check,
//!    profile.
//! 6. [`login`]: the chain as two entry points, [`login::full_login`] and
//!    [`login::refresh_login`].
//!
//! Sources: docs:<https://minecraft.wiki/w/Microsoft_authentication> (endpoints,
//! bodies, XSTS errors) and
//! docs:<https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code>
//! (device flow). No type here prints tokens: they are [`Secret`]s and error
//! texts carry only a step name and an HTTP status.

pub mod device_code;
pub mod login;
pub mod minecraft;
pub mod refresh;
pub mod xbox;
pub mod xsts;

use serde::de::DeserializeOwned;

use crate::{
    error::AuthError,
    secret::Secret,
    transport::{Body, Request, Response},
};

/// Application (client) id of the Azure app used for sign-in. This is the
/// Prism Launcher id, by owner decision (ADR-0067); it must be replaced by an
/// own app before any public release.
pub const CLIENT_ID: &str = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb";

/// OAuth scopes: Xbox Live sign-in plus a refresh token.
pub const SCOPE: &str = "XboxLive.SignIn XboxLive.offline_access";

/// Environment variable that overrides [`CLIENT_ID`].
pub const CLIENT_ID_ENV: &str = "UMICRAFT_MSA_CLIENT_ID";

/// The client id to use: `UMICRAFT_MSA_CLIENT_ID` when set and not empty,
/// otherwise [`CLIENT_ID`].
pub fn client_id() -> String {
    client_id_or_default(std::env::var(CLIENT_ID_ENV).ok())
}

fn client_id_or_default(override_id: Option<String>) -> String {
    override_id
        .map(|id| id.trim().to_owned())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| CLIENT_ID.to_owned())
}

/// What the player must do to sign in, from the device-code request.
#[derive(Debug, Clone)]
pub struct DeviceCode {
    /// Short code the player types on the verification page.
    pub user_code: String,
    /// Page where the code is entered, such as `https://microsoft.com/link`.
    pub verification_uri: String,
    /// Opaque code used to poll for the token.
    pub device_code: Secret,
    /// Seconds to wait between polls.
    pub interval_secs: u64,
    /// Seconds after which the code stops working.
    pub expires_in_secs: u64,
}

/// A Microsoft account token pair, from the device flow or a refresh.
#[derive(Debug, Clone)]
pub struct MsaTokens {
    /// Short-lived OAuth access token, used only to start the Xbox chain.
    pub access_token: Secret,
    /// Long-lived token to sign in again without the player.
    pub refresh_token: Secret,
}

/// A finished sign-in: a Minecraft session plus the token to renew it.
#[derive(Debug, Clone)]
pub struct MicrosoftLogin {
    /// Minecraft profile name.
    pub username: String,
    /// Profile UUID, 32 lowercase hex digits.
    pub uuid: String,
    /// Address of the active skin image, if the player has one.
    pub skin_url: Option<String>,
    /// Minecraft access token.
    pub minecraft_token: Secret,
    /// Seconds the Minecraft token stays valid.
    pub minecraft_expires_in_secs: u64,
    /// Microsoft refresh token, to store in the vault.
    pub refresh_token: Secret,
}

pub(crate) fn form_post(url: &str, fields: &[(&str, &str)]) -> Request {
    Request {
        method: "POST",
        url: url.to_owned(),
        bearer: None,
        body: Body::Form(
            fields
                .iter()
                .map(|&(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        ),
    }
}

pub(crate) fn json_post(url: &str, body: &serde_json::Value) -> Request {
    Request {
        method: "POST",
        url: url.to_owned(),
        bearer: None,
        body: Body::Json(body.to_string()),
    }
}

pub(crate) fn bearer_get(url: &str, token: &Secret) -> Request {
    Request {
        method: "GET",
        url: url.to_owned(),
        bearer: Some(token.clone()),
        body: Body::None,
    }
}

/// Parses a JSON answer. The error names the `step` and the status only,
/// never the body, which may hold tokens.
pub(crate) fn parse_body<D: DeserializeOwned>(
    step: &str,
    response: &Response,
) -> Result<D, AuthError> {
    serde_json::from_str(&response.body).map_err(|_| unexpected(step, response.status))
}

pub(crate) fn unexpected(step: &str, status: u16) -> AuthError {
    AuthError::Protocol(format!("{step} answered HTTP {status}"))
}

#[cfg(test)]
pub(crate) mod testing {
    //! A fake transport that records requests and replays canned answers.

    use std::{collections::VecDeque, sync::Mutex};

    use super::*;
    use crate::transport::HttpTransport;

    pub struct FakeTransport {
        answers: Mutex<VecDeque<Response>>,
        sent: Mutex<Vec<Request>>,
    }

    impl FakeTransport {
        pub fn new(answers: Vec<(u16, &str)>) -> Self {
            Self {
                answers: Mutex::new(
                    answers
                        .into_iter()
                        .map(|(status, body)| Response {
                            status,
                            body: body.to_owned(),
                        })
                        .collect(),
                ),
                sent: Mutex::default(),
            }
        }

        pub fn request(&self, index: usize) -> Request {
            self.sent.lock().unwrap()[index].clone()
        }

        pub fn count(&self) -> usize {
            self.sent.lock().unwrap().len()
        }
    }

    impl HttpTransport for FakeTransport {
        async fn send(&self, request: Request) -> Result<Response, AuthError> {
            self.sent.lock().unwrap().push(request);
            self.answers
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| AuthError::Transport("no canned answer".into()))
        }
    }

    pub fn form_field(request: &Request, key: &str) -> String {
        match &request.body {
            Body::Form(pairs) => pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default(),
            _ => panic!("not a form body"),
        }
    }

    pub fn json_body(request: &Request) -> serde_json::Value {
        match &request.body {
            Body::Json(text) => serde_json::from_str(text).unwrap(),
            _ => panic!("not a json body"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_id_defaults_and_overrides() {
        assert_eq!(client_id_or_default(None), CLIENT_ID);
        assert_eq!(client_id_or_default(Some("  ".into())), CLIENT_ID);
        assert_eq!(client_id_or_default(Some(" my-id ".into())), "my-id");
    }

    #[test]
    fn parse_error_hides_the_body() {
        let response = Response {
            status: 502,
            body: "secret-token-body".into(),
        };
        let err = parse_body::<serde_json::Value>("xbox", &response).unwrap_err();
        assert!(!err.to_string().contains("secret-token"));
        assert!(err.to_string().contains("502"));
    }
}
