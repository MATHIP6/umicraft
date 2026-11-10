// SPDX-License-Identifier: MIT OR Apache-2.0
//! Step 5: Minecraft services: token, ownership check and profile.
//!
//! Source: docs:<https://minecraft.wiki/w/Microsoft_authentication#Authenticate_with_Minecraft>
//! and the entitlement and profile sections after it.

use serde::Deserialize;
use serde_json::json;

use super::{bearer_get, json_post, parse_body, unexpected, xbox::XboxToken};
use crate::{error::AuthError, secret::Secret, transport::HttpTransport};

const LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const ENTITLEMENTS_URL: &str = "https://api.minecraftservices.com/entitlements/mcstore";
const PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
/// Entitlement names that prove ownership of the Java Edition.
const OWNERSHIP_ITEMS: [&str; 2] = ["product_minecraft", "game_minecraft"];

/// A Minecraft services access token and its lifetime.
#[derive(Debug, Clone)]
pub struct MinecraftToken {
    /// The bearer token.
    pub access_token: Secret,
    /// Seconds until it expires.
    pub expires_in_secs: u64,
}

/// The player's Minecraft profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// Profile UUID, 32 hex digits without dashes.
    pub id: String,
    /// Player name.
    pub name: String,
    /// Address of the active skin image, if the player has one.
    pub skin_url: Option<String>,
}

#[derive(Deserialize)]
struct LoginAnswer {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct Entitlements {
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    name: String,
}

/// Exchanges the XSTS token for a Minecraft access token.
///
/// # Errors
/// [`AuthError::Transport`], or [`AuthError::Protocol`] for a non-200 answer.
pub async fn login_with_xbox<T: HttpTransport>(
    transport: &T,
    xsts: &XboxToken,
) -> Result<MinecraftToken, AuthError> {
    let identity = format!("XBL3.0 x={};{}", xsts.user_hash, xsts.token.expose());
    let request = json_post(LOGIN_URL, &json!({ "identityToken": identity }));
    let response = transport.send(request).await?;
    if response.status != 200 {
        return Err(unexpected("minecraft login", response.status));
    }
    let answer: LoginAnswer = parse_body("minecraft login", &response)?;
    Ok(MinecraftToken {
        access_token: Secret::new(answer.access_token),
        expires_in_secs: answer.expires_in,
    })
}

/// Checks that the account owns Minecraft: Java Edition.
///
/// # Errors
/// [`AuthError::NoMinecraftLicence`] when no `product_minecraft` or
/// `game_minecraft` entitlement exists (including an empty list);
/// [`AuthError::Transport`] or [`AuthError::Protocol`] otherwise.
pub async fn check_entitlements<T: HttpTransport>(
    transport: &T,
    token: &MinecraftToken,
) -> Result<(), AuthError> {
    let response = transport
        .send(bearer_get(ENTITLEMENTS_URL, &token.access_token))
        .await?;
    if response.status != 200 {
        return Err(unexpected("entitlements", response.status));
    }
    let entitlements: Entitlements = parse_body("entitlements", &response)?;
    let owns = entitlements
        .items
        .iter()
        .any(|item| OWNERSHIP_ITEMS.contains(&item.name.as_str()));
    if owns {
        Ok(())
    } else {
        Err(AuthError::NoMinecraftLicence)
    }
}

/// Fetches the player's profile (name and UUID).
///
/// # Errors
/// [`AuthError::NoMinecraftProfile`] on HTTP 404 (the account owns the game
/// but never chose a name); [`AuthError::Transport`] or
/// [`AuthError::Protocol`] otherwise.
pub async fn fetch_profile<T: HttpTransport>(
    transport: &T,
    token: &MinecraftToken,
) -> Result<Profile, AuthError> {
    let response = transport
        .send(bearer_get(PROFILE_URL, &token.access_token))
        .await?;
    match response.status {
        200 => {
            #[derive(Deserialize)]
            struct Answer {
                id: String,
                name: String,
                #[serde(default)]
                skins: Vec<Skin>,
            }
            #[derive(Deserialize)]
            struct Skin {
                state: String,
                url: String,
            }
            let answer: Answer = parse_body("profile", &response)?;
            let skin_url = answer
                .skins
                .into_iter()
                .find(|skin| skin.state == "ACTIVE")
                .map(|skin| skin.url);
            Ok(Profile {
                id: answer.id,
                name: answer.name,
                skin_url,
            })
        }
        404 => Err(AuthError::NoMinecraftProfile),
        status => Err(unexpected("profile", status)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        microsoft::testing::{json_body, FakeTransport},
        transport::Body,
    };

    fn token() -> MinecraftToken {
        MinecraftToken {
            access_token: Secret::new("mc-token"),
            expires_in_secs: 86400,
        }
    }

    #[tokio::test]
    async fn login_sends_the_identity_token() {
        let t = FakeTransport::new(vec![(
            200,
            r#"{"access_token":"mc-token","expires_in":86400,"token_type":"Bearer"}"#,
        )]);
        let xsts = XboxToken {
            token: Secret::new("xsts-token"),
            user_hash: "hash9".into(),
        };
        let got = login_with_xbox(&t, &xsts).await.unwrap();
        assert_eq!(got.access_token.expose(), "mc-token");
        assert_eq!(got.expires_in_secs, 86400);
        let r = t.request(0);
        assert_eq!(r.url, LOGIN_URL);
        assert!(r.bearer.is_none());
        assert_eq!(json_body(&r)["identityToken"], "XBL3.0 x=hash9;xsts-token");
    }

    #[tokio::test]
    async fn entitlements_accept_either_ownership_item() {
        for name in OWNERSHIP_ITEMS {
            let body = format!(r#"{{"items":[{{"name":"{name}","signature":"s"}}]}}"#);
            let t = FakeTransport::new(vec![(200, &body)]);
            check_entitlements(&t, &token()).await.unwrap();
            let r = t.request(0);
            assert_eq!(r.url, ENTITLEMENTS_URL);
            assert_eq!(r.method, "GET");
            assert!(matches!(r.body, Body::None));
            assert_eq!(r.bearer.unwrap().expose(), "mc-token");
        }
    }

    #[tokio::test]
    async fn empty_or_unrelated_entitlements_mean_no_licence() {
        for body in [
            r#"{"items":[]}"#,
            r#"{}"#,
            r#"{"items":[{"name":"other"}]}"#,
        ] {
            let t = FakeTransport::new(vec![(200, body)]);
            let err = check_entitlements(&t, &token()).await.err().unwrap();
            assert!(matches!(err, AuthError::NoMinecraftLicence), "{body}");
        }
    }

    async fn profile_of(body: &str) -> Profile {
        let t = FakeTransport::new(vec![(200, body)]);
        fetch_profile(&t, &token()).await.unwrap()
    }

    #[tokio::test]
    async fn the_active_skin_is_the_profile_skin() {
        let profile = profile_of(
            r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","skins":[
                {"id":"1","state":"INACTIVE","url":"http://textures.minecraft.net/texture/old"},
                {"id":"2","state":"ACTIVE","url":"http://textures.minecraft.net/texture/new","variant":"SLIM"}]}"#,
        )
        .await;
        assert_eq!(
            profile.skin_url.as_deref(),
            Some("http://textures.minecraft.net/texture/new")
        );
    }

    #[tokio::test]
    async fn a_profile_without_an_active_skin_has_no_skin_url() {
        for body in [
            r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","skins":[]}"#,
            r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#,
            r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","skins":[{"state":"INACTIVE","url":"u"}]}"#,
        ] {
            assert_eq!(profile_of(body).await.skin_url, None, "{body}");
        }
    }

    #[tokio::test]
    async fn profile_reads_id_and_name_with_bearer() {
        let t = FakeTransport::new(vec![(
            200,
            r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","skins":[]}"#,
        )]);
        let p = fetch_profile(&t, &token()).await.unwrap();
        assert_eq!(p.name, "Notch");
        assert_eq!(p.id, "069a79f444e94726a5befca90e38aaf5");
        let r = t.request(0);
        assert_eq!(r.url, PROFILE_URL);
        assert_eq!(r.bearer.unwrap().expose(), "mc-token");
    }

    #[tokio::test]
    async fn missing_profile_is_typed() {
        let t = FakeTransport::new(vec![(
            404,
            r#"{"path":"/minecraft/profile","errorType":"NOT_FOUND"}"#,
        )]);
        let err = fetch_profile(&t, &token()).await.err().unwrap();
        assert!(matches!(err, AuthError::NoMinecraftProfile));
    }
}
