// SPDX-License-Identifier: MIT OR Apache-2.0
//! Step 1: the OAuth device authorization grant.
//!
//! Source: docs:<https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code>.
//! The `consumers` tenant is used because only personal Microsoft accounts
//! own Minecraft: Java Edition.

use std::{future::Future, time::Duration};

use serde::Deserialize;

use super::{client_id, form_post, parse_body, unexpected, DeviceCode, MsaTokens, SCOPE};
use crate::{
    error::AuthError,
    secret::Secret,
    transport::{HttpTransport, Response},
};

const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
pub(super) const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// Extra wait demanded by a `slow_down` answer (RFC 8628 section 3.5).
const SLOW_DOWN_STEP_SECS: u64 = 5;
/// Poll interval when the answer does not give one (RFC 8628 section 3.2).
const DEFAULT_INTERVAL_SECS: u64 = 5;

/// Waits between polls. Injected so tests do not really sleep.
pub trait Sleeper: Send + Sync {
    /// Completes after `duration`.
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> + Send;
}

/// [`Sleeper`] backed by the tokio timer.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioSleeper;

impl Sleeper for TokioSleeper {
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

#[derive(Deserialize)]
struct DeviceCodeAnswer {
    user_code: String,
    verification_uri: String,
    device_code: String,
    interval: Option<u64>,
    expires_in: u64,
}

#[derive(Deserialize)]
struct TokenAnswer {
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize)]
pub(super) struct OauthError {
    pub(super) error: String,
}

/// Asks Microsoft for a device code to show to the player.
///
/// # Errors
/// [`AuthError::Transport`] when the request fails, [`AuthError::Protocol`]
/// for an unexpected answer.
pub async fn request_device_code<T: HttpTransport>(transport: &T) -> Result<DeviceCode, AuthError> {
    let request = form_post(
        DEVICE_CODE_URL,
        &[("client_id", &client_id()), ("scope", SCOPE)],
    );
    let response = transport.send(request).await?;
    if response.status != 200 {
        return Err(unexpected("device code", response.status));
    }
    let answer: DeviceCodeAnswer = parse_body("device code", &response)?;
    Ok(DeviceCode {
        user_code: answer.user_code,
        verification_uri: answer.verification_uri,
        device_code: Secret::new(answer.device_code),
        interval_secs: answer.interval.unwrap_or(DEFAULT_INTERVAL_SECS),
        expires_in_secs: answer.expires_in,
    })
}

/// Polls the token endpoint until the player finishes (or refuses) the
/// sign-in.
///
/// `cancel` is checked before every request; when it returns `true` the poll
/// stops with [`AuthError::LoginCancelled`]. `authorization_pending` waits one
/// interval, `slow_down` adds five seconds to the interval for good.
///
/// # Errors
/// [`AuthError::LoginDeclined`], [`AuthError::LoginExpired`],
/// [`AuthError::LoginCancelled`], [`AuthError::Transport`], or
/// [`AuthError::Protocol`] for any other OAuth error.
pub async fn poll_token<T: HttpTransport, S: Sleeper>(
    transport: &T,
    sleeper: &S,
    device: &DeviceCode,
    cancel: impl Fn() -> bool + Send,
) -> Result<MsaTokens, AuthError> {
    let mut interval = device.interval_secs;
    loop {
        if cancel() {
            return Err(AuthError::LoginCancelled);
        }
        let request = form_post(
            TOKEN_URL,
            &[
                ("grant_type", DEVICE_GRANT),
                ("client_id", &client_id()),
                ("device_code", device.device_code.expose()),
            ],
        );
        let response = transport.send(request).await?;
        if response.status == 200 {
            return tokens_from(&response);
        }
        let error: OauthError = parse_body("token poll", &response)?;
        match error.error.as_str() {
            "authorization_pending" => {}
            "slow_down" => interval += SLOW_DOWN_STEP_SECS,
            "authorization_declined" => return Err(AuthError::LoginDeclined),
            "bad_verification_code" | "expired_token" => return Err(AuthError::LoginExpired),
            _ => return Err(unexpected("token poll", response.status)),
        }
        sleeper.sleep(Duration::from_secs(interval)).await;
    }
}

pub(super) fn tokens_from(response: &Response) -> Result<MsaTokens, AuthError> {
    let answer: TokenAnswer = parse_body("token", response)?;
    Ok(MsaTokens {
        access_token: Secret::new(answer.access_token),
        refresh_token: Secret::new(answer.refresh_token),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::microsoft::testing::{form_field, FakeTransport};

    #[derive(Default)]
    struct FakeSleeper(Mutex<Vec<u64>>);

    impl Sleeper for FakeSleeper {
        async fn sleep(&self, duration: Duration) {
            self.0.lock().unwrap().push(duration.as_secs());
        }
    }

    const PENDING: &str = r#"{"error":"authorization_pending"}"#;
    const TOKENS: &str = r#"{"access_token":"msa-a","refresh_token":"msa-r"}"#;

    fn device() -> DeviceCode {
        DeviceCode {
            user_code: "ABCD".into(),
            verification_uri: "https://microsoft.com/link".into(),
            device_code: Secret::new("dev-code"),
            interval_secs: 5,
            expires_in_secs: 900,
        }
    }

    async fn poll(t: &FakeTransport, s: &FakeSleeper) -> Result<MsaTokens, AuthError> {
        poll_token(t, s, &device(), || false).await
    }

    #[tokio::test]
    async fn requests_a_device_code() {
        let t = FakeTransport::new(vec![(
            200,
            r#"{"user_code":"ABCD","device_code":"d","verification_uri":"https://microsoft.com/link","expires_in":900,"interval":5}"#,
        )]);
        let code = request_device_code(&t).await.unwrap();
        assert_eq!(code.user_code, "ABCD");
        assert_eq!(code.device_code.expose(), "d");
        assert_eq!((code.interval_secs, code.expires_in_secs), (5, 900));
        let r = t.request(0);
        assert_eq!(r.url, DEVICE_CODE_URL);
        assert_eq!(form_field(&r, "client_id"), client_id());
        assert_eq!(form_field(&r, "scope"), SCOPE);
    }

    #[tokio::test]
    async fn pending_twice_then_success() {
        let t = FakeTransport::new(vec![(400, PENDING), (400, PENDING), (200, TOKENS)]);
        let s = FakeSleeper::default();
        let tokens = poll(&t, &s).await.unwrap();
        assert_eq!(tokens.access_token.expose(), "msa-a");
        assert_eq!(tokens.refresh_token.expose(), "msa-r");
        assert_eq!(*s.0.lock().unwrap(), vec![5, 5]);
        let r = t.request(0);
        assert_eq!(r.url, TOKEN_URL);
        assert_eq!(form_field(&r, "grant_type"), DEVICE_GRANT);
        assert_eq!(form_field(&r, "device_code"), "dev-code");
    }

    #[tokio::test]
    async fn slow_down_adds_five_seconds_for_good() {
        let slow = r#"{"error":"slow_down"}"#;
        let t = FakeTransport::new(vec![(400, slow), (400, PENDING), (200, TOKENS)]);
        let s = FakeSleeper::default();
        poll(&t, &s).await.unwrap();
        assert_eq!(*s.0.lock().unwrap(), vec![10, 10]);
    }

    #[tokio::test]
    async fn declined_is_typed() {
        let t = FakeTransport::new(vec![(400, r#"{"error":"authorization_declined"}"#)]);
        let err = poll(&t, &FakeSleeper::default()).await.err().unwrap();
        assert!(matches!(err, AuthError::LoginDeclined));
    }

    #[tokio::test]
    async fn expired_codes_are_typed() {
        for code in ["expired_token", "bad_verification_code"] {
            let body = format!(r#"{{"error":"{code}"}}"#);
            let t = FakeTransport::new(vec![(400, &body)]);
            let err = poll(&t, &FakeSleeper::default()).await.err().unwrap();
            assert!(matches!(err, AuthError::LoginExpired), "{code}");
        }
    }

    #[tokio::test]
    async fn unknown_oauth_error_is_a_protocol_error() {
        let t = FakeTransport::new(vec![(400, r#"{"error":"invalid_client"}"#)]);
        let err = poll(&t, &FakeSleeper::default()).await.err().unwrap();
        assert!(matches!(err, AuthError::Protocol(_)));
    }

    #[tokio::test]
    async fn cancel_stops_before_any_request() {
        let t = FakeTransport::new(vec![]);
        let result = poll_token(&t, &FakeSleeper::default(), &device(), || true).await;
        assert!(matches!(result, Err(AuthError::LoginCancelled)));
        assert_eq!(t.count(), 0);
    }
}
