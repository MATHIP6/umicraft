// SPDX-License-Identifier: MIT OR Apache-2.0
//! Step 3: Xbox Live user token (`XBL`).
//!
//! Source: docs:<https://minecraft.wiki/w/Microsoft_authentication#Authenticate_with_Xbox_Live>.

use serde::Deserialize;
use serde_json::json;

use super::{json_post, parse_body, unexpected};
use crate::{
    error::AuthError,
    secret::Secret,
    transport::{HttpTransport, Response},
};

const XBOX_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";

/// An Xbox token with the user hash (`uhs`) that goes with it.
#[derive(Debug, Clone)]
pub struct XboxToken {
    /// The JWT.
    pub token: Secret,
    /// User hash, the `uhs` display claim.
    pub user_hash: String,
}

#[derive(Deserialize)]
struct XboxAnswer {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: Claims,
}

#[derive(Deserialize)]
struct Claims {
    xui: Vec<Xui>,
}

#[derive(Deserialize)]
struct Xui {
    uhs: String,
}

/// Reads the answer of an Xbox endpoint (`XBL` or `XSTS`, same shape).
pub(super) fn token_from(step: &str, response: &Response) -> Result<XboxToken, AuthError> {
    let answer: XboxAnswer = parse_body(step, response)?;
    let xui = answer
        .display_claims
        .xui
        .into_iter()
        .next()
        .ok_or_else(|| unexpected(step, response.status))?;
    Ok(XboxToken {
        token: Secret::new(answer.token),
        user_hash: xui.uhs,
    })
}

/// Exchanges a Microsoft access token for an Xbox Live user token.
///
/// # Errors
/// [`AuthError::Transport`], or [`AuthError::Protocol`] for a non-200 answer
/// or a body without token and user hash.
pub async fn authenticate<T: HttpTransport>(
    transport: &T,
    msa_access_token: &Secret,
) -> Result<XboxToken, AuthError> {
    let body = json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={}", msa_access_token.expose()),
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    });
    let response = transport.send(json_post(XBOX_URL, &body)).await?;
    if response.status != 200 {
        return Err(unexpected("xbox live", response.status));
    }
    token_from("xbox live", &response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        microsoft::testing::{json_body, FakeTransport},
        transport::Body,
    };

    const ANSWER: &str = r#"{"Token":"xbl-token","DisplayClaims":{"xui":[{"uhs":"hash123"}]}}"#;

    #[tokio::test]
    async fn posts_the_rps_ticket_and_reads_token_and_hash() {
        let t = FakeTransport::new(vec![(200, ANSWER)]);
        let xbl = authenticate(&t, &Secret::new("msa-a")).await.unwrap();
        assert_eq!(xbl.token.expose(), "xbl-token");
        assert_eq!(xbl.user_hash, "hash123");
        let r = t.request(0);
        assert_eq!(r.url, XBOX_URL);
        assert!(matches!(r.body, Body::Json(_)));
        let b = json_body(&r);
        assert_eq!(b["Properties"]["RpsTicket"], "d=msa-a");
        assert_eq!(b["Properties"]["AuthMethod"], "RPS");
        assert_eq!(b["Properties"]["SiteName"], "user.auth.xboxlive.com");
        assert_eq!(b["RelyingParty"], "http://auth.xboxlive.com");
        assert_eq!(b["TokenType"], "JWT");
    }

    #[tokio::test]
    async fn missing_claims_is_a_protocol_error() {
        let t = FakeTransport::new(vec![(200, r#"{"Token":"x","DisplayClaims":{"xui":[]}}"#)]);
        let err = authenticate(&t, &Secret::new("a")).await.err().unwrap();
        assert!(matches!(err, AuthError::Protocol(_)));
    }

    #[tokio::test]
    async fn non_200_is_a_protocol_error() {
        let t = FakeTransport::new(vec![(400, "bad")]);
        let err = authenticate(&t, &Secret::new("a")).await.err().unwrap();
        assert!(matches!(err, AuthError::Protocol(_)));
    }
}
