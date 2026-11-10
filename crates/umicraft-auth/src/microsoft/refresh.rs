// SPDX-License-Identifier: MIT OR Apache-2.0
//! Step 2: sign in again from a saved refresh token.
//!
//! Source: docs:<https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-auth-code-flow#refresh-the-access-token>.

use super::{
    client_id,
    device_code::{tokens_from, OauthError, TOKEN_URL},
    form_post, parse_body, unexpected, MsaTokens, SCOPE,
};
use crate::{error::AuthError, secret::Secret, transport::HttpTransport};

/// Trades `refresh_token` for a new token pair. Microsoft rotates refresh
/// tokens, so the caller must store the returned one.
///
/// # Errors
/// [`AuthError::LoginExpired`] when the refresh token is no longer valid
/// (`invalid_grant`), [`AuthError::Transport`] or [`AuthError::Protocol`]
/// otherwise.
pub async fn refresh_tokens<T: HttpTransport>(
    transport: &T,
    refresh_token: &Secret,
) -> Result<MsaTokens, AuthError> {
    let request = form_post(
        TOKEN_URL,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.expose()),
            ("client_id", &client_id()),
            ("scope", SCOPE),
        ],
    );
    let response = transport.send(request).await?;
    if response.status == 200 {
        return tokens_from(&response);
    }
    match parse_body::<OauthError>("refresh", &response) {
        Ok(e) if e.error == "invalid_grant" => Err(AuthError::LoginExpired),
        _ => Err(unexpected("refresh", response.status)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::microsoft::testing::{form_field, FakeTransport};

    #[tokio::test]
    async fn sends_the_refresh_grant_and_returns_new_tokens() {
        let t = FakeTransport::new(vec![(
            200,
            r#"{"access_token":"new-a","refresh_token":"new-r"}"#,
        )]);
        let tokens = refresh_tokens(&t, &Secret::new("old-r")).await.unwrap();
        assert_eq!(tokens.access_token.expose(), "new-a");
        assert_eq!(tokens.refresh_token.expose(), "new-r");
        let r = t.request(0);
        assert_eq!(r.url, TOKEN_URL);
        assert_eq!(form_field(&r, "grant_type"), "refresh_token");
        assert_eq!(form_field(&r, "refresh_token"), "old-r");
        assert_eq!(form_field(&r, "scope"), SCOPE);
        assert_eq!(form_field(&r, "client_id"), client_id());
    }

    #[tokio::test]
    async fn invalid_grant_means_sign_in_again() {
        let t = FakeTransport::new(vec![(400, r#"{"error":"invalid_grant"}"#)]);
        let err = refresh_tokens(&t, &Secret::new("x")).await.err().unwrap();
        assert!(matches!(err, AuthError::LoginExpired));
    }

    #[tokio::test]
    async fn other_failures_are_protocol_errors() {
        let t = FakeTransport::new(vec![(500, "oops")]);
        let err = refresh_tokens(&t, &Secret::new("x")).await.err().unwrap();
        assert!(matches!(err, AuthError::Protocol(_)));
    }
}
