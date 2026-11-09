// SPDX-License-Identifier: MIT OR Apache-2.0
//! Step 4: XSTS token for the Minecraft services, and its error codes.
//!
//! Source: docs:<https://minecraft.wiki/w/Microsoft_authentication#Authenticate_with_XSTS>
//! (the `XErr` table).

use serde::Deserialize;
use serde_json::json;

use super::{
    json_post, unexpected,
    xbox::{token_from, XboxToken},
};
use crate::{error::AuthError, transport::HttpTransport};

const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";

#[derive(Deserialize)]
struct XstsError {
    #[serde(rename = "XErr")]
    code: u64,
}

/// Maps an XSTS `XErr` code to the typed error, `None` for unknown codes.
fn error_for(code: u64) -> Option<AuthError> {
    Some(match code {
        2_148_916_227 => AuthError::XboxBanned,
        2_148_916_233 => AuthError::NoXboxAccount,
        2_148_916_235 => AuthError::XboxUnavailableInCountry,
        2_148_916_236 | 2_148_916_237 => AuthError::AdultVerificationRequired,
        2_148_916_238 => AuthError::ChildAccount,
        _ => return None,
    })
}

/// Exchanges the Xbox Live user token for an XSTS token.
///
/// # Errors
/// [`AuthError::XboxBanned`], [`AuthError::NoXboxAccount`],
/// [`AuthError::XboxUnavailableInCountry`],
/// [`AuthError::AdultVerificationRequired`] or [`AuthError::ChildAccount`]
/// from the 401 `XErr` codes; [`AuthError::Transport`] or
/// [`AuthError::Protocol`] otherwise.
pub async fn authorize<T: HttpTransport>(
    transport: &T,
    xbl: &XboxToken,
) -> Result<XboxToken, AuthError> {
    let body = json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl.token.expose()],
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT",
    });
    let response = transport.send(json_post(XSTS_URL, &body)).await?;
    match response.status {
        200 => token_from("xsts", &response),
        401 => Err(serde_json::from_str::<XstsError>(&response.body)
            .ok()
            .and_then(|e| error_for(e.code))
            .unwrap_or_else(|| unexpected("xsts", 401))),
        status => Err(unexpected("xsts", status)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        microsoft::testing::{json_body, FakeTransport},
        secret::Secret,
    };

    fn xbl() -> XboxToken {
        XboxToken {
            token: Secret::new("xbl-token"),
            user_hash: "h".into(),
        }
    }

    #[tokio::test]
    async fn posts_the_user_token_and_reads_the_result() {
        let t = FakeTransport::new(vec![(
            200,
            r#"{"Token":"xsts-token","DisplayClaims":{"xui":[{"uhs":"hash9"}]}}"#,
        )]);
        let xsts = authorize(&t, &xbl()).await.unwrap();
        assert_eq!(xsts.token.expose(), "xsts-token");
        assert_eq!(xsts.user_hash, "hash9");
        let r = t.request(0);
        assert_eq!(r.url, XSTS_URL);
        let b = json_body(&r);
        assert_eq!(b["Properties"]["SandboxId"], "RETAIL");
        assert_eq!(b["Properties"]["UserTokens"][0], "xbl-token");
        assert_eq!(b["RelyingParty"], "rp://api.minecraftservices.com/");
        assert_eq!(b["TokenType"], "JWT");
    }

    #[tokio::test]
    async fn every_xerr_code_maps_to_its_error() {
        let cases: [(u64, AuthError); 6] = [
            (2_148_916_227, AuthError::XboxBanned),
            (2_148_916_233, AuthError::NoXboxAccount),
            (2_148_916_235, AuthError::XboxUnavailableInCountry),
            (2_148_916_236, AuthError::AdultVerificationRequired),
            (2_148_916_237, AuthError::AdultVerificationRequired),
            (2_148_916_238, AuthError::ChildAccount),
        ];
        for (code, expected) in cases {
            let body = format!(r#"{{"Identity":"0","XErr":{code},"Message":"","Redirect":"x"}}"#);
            let t = FakeTransport::new(vec![(401, &body)]);
            let err = authorize(&t, &xbl()).await.err().unwrap();
            assert_eq!(
                std::mem::discriminant(&err),
                std::mem::discriminant(&expected),
                "{code}: {err}"
            );
        }
    }

    #[tokio::test]
    async fn unknown_xerr_is_a_protocol_error() {
        let t = FakeTransport::new(vec![(401, r#"{"XErr":1}"#)]);
        let err = authorize(&t, &xbl()).await.err().unwrap();
        assert!(matches!(err, AuthError::Protocol(_)));
    }
}
