// SPDX-License-Identifier: MIT OR Apache-2.0
//! Step 6: the whole chain, from a Microsoft token to a Minecraft session.

use super::{
    device_code::{poll_token, Sleeper},
    minecraft::{check_entitlements, fetch_profile, login_with_xbox},
    refresh::refresh_tokens,
    xbox, xsts, DeviceCode, MicrosoftLogin, MsaTokens,
};
use crate::{error::AuthError, secret::Secret, session::Session, transport::HttpTransport};

/// Runs Xbox Live, XSTS, Minecraft login, ownership check and profile for a
/// Microsoft token pair.
///
/// The profile name and UUID are validated with [`Session::new`], so a
/// [`MicrosoftLogin`] always holds a usable session identity.
///
/// # Errors
/// Any step's error, such as [`AuthError::ChildAccount`],
/// [`AuthError::NoMinecraftLicence`] or [`AuthError::NoMinecraftProfile`];
/// [`AuthError::InvalidUsername`] or [`AuthError::InvalidUuid`] for a profile
/// the game cannot use.
pub async fn login_from_msa_token<T: HttpTransport>(
    transport: &T,
    tokens: MsaTokens,
) -> Result<MicrosoftLogin, AuthError> {
    let xbl = xbox::authenticate(transport, &tokens.access_token).await?;
    let xsts = xsts::authorize(transport, &xbl).await?;
    let minecraft = login_with_xbox(transport, &xsts).await?;
    check_entitlements(transport, &minecraft).await?;
    let profile = fetch_profile(transport, &minecraft).await?;
    let session = Session::new(
        &profile.name,
        &profile.id,
        Secret::new(minecraft.access_token.expose()),
    )?;
    Ok(MicrosoftLogin {
        username: session.username,
        uuid: session.uuid,
        minecraft_token: minecraft.access_token,
        minecraft_expires_in_secs: minecraft.expires_in_secs,
        refresh_token: tokens.refresh_token,
    })
}

/// Finishes a device-code sign-in: polls until the player approves, then runs
/// the chain. `device` comes from
/// [`request_device_code`](super::device_code::request_device_code) and has
/// already been shown to the player.
///
/// # Errors
/// The errors of [`poll_token`] and [`login_from_msa_token`].
pub async fn full_login<T: HttpTransport, S: Sleeper>(
    transport: &T,
    sleeper: &S,
    device: &DeviceCode,
    cancel: impl Fn() -> bool + Send,
) -> Result<MicrosoftLogin, AuthError> {
    let tokens = poll_token(transport, sleeper, device, cancel).await?;
    login_from_msa_token(transport, tokens).await
}

/// Signs in again from a saved refresh token. The result holds the rotated
/// refresh token, which replaces the saved one.
///
/// # Errors
/// [`AuthError::LoginExpired`] when the refresh token no longer works, plus
/// the errors of [`login_from_msa_token`].
pub async fn refresh_login<T: HttpTransport>(
    transport: &T,
    refresh_token: &Secret,
) -> Result<MicrosoftLogin, AuthError> {
    let tokens = refresh_tokens(transport, refresh_token).await?;
    login_from_msa_token(transport, tokens).await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::microsoft::testing::FakeTransport;

    struct NoSleep;

    impl Sleeper for NoSleep {
        async fn sleep(&self, _: Duration) {}
    }

    const XBOX: &str = r#"{"Token":"xbl-t","DisplayClaims":{"xui":[{"uhs":"h"}]}}"#;
    const MC: &str = r#"{"access_token":"mc-t","expires_in":86400}"#;
    const ENT: &str = r#"{"items":[{"name":"game_minecraft"}]}"#;
    const PROFILE: &str = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#;
    const MSA: &str = r#"{"access_token":"msa-a","refresh_token":"msa-r"}"#;

    fn chain() -> Vec<(u16, &'static str)> {
        vec![
            (200, XBOX),
            (200, XBOX),
            (200, MC),
            (200, ENT),
            (200, PROFILE),
        ]
    }

    #[tokio::test]
    async fn chain_produces_a_login() {
        let t = FakeTransport::new(chain());
        let tokens = MsaTokens {
            access_token: Secret::new("msa-a"),
            refresh_token: Secret::new("msa-r"),
        };
        let login = login_from_msa_token(&t, tokens).await.unwrap();
        assert_eq!(login.username, "Notch");
        assert_eq!(login.uuid, "069a79f444e94726a5befca90e38aaf5");
        assert_eq!(login.minecraft_token.expose(), "mc-t");
        assert_eq!(login.minecraft_expires_in_secs, 86400);
        assert_eq!(login.refresh_token.expose(), "msa-r");
        assert_eq!(t.count(), 5);
    }

    #[tokio::test]
    async fn full_login_polls_then_chains() {
        let mut answers = vec![(400, r#"{"error":"authorization_pending"}"#), (200, MSA)];
        answers.extend(chain());
        let t = FakeTransport::new(answers);
        let device = DeviceCode {
            user_code: "A".into(),
            verification_uri: "u".into(),
            device_code: Secret::new("d"),
            interval_secs: 5,
            expires_in_secs: 900,
        };
        let login = full_login(&t, &NoSleep, &device, || false).await.unwrap();
        assert_eq!(login.username, "Notch");
        assert_eq!(t.count(), 7);
    }

    #[tokio::test]
    async fn refresh_login_rotates_the_refresh_token() {
        let mut answers = vec![(200, MSA)];
        answers.extend(chain());
        let t = FakeTransport::new(answers);
        let login = refresh_login(&t, &Secret::new("old")).await.unwrap();
        assert_eq!(login.refresh_token.expose(), "msa-r");
    }

    #[tokio::test]
    async fn an_unusable_profile_name_is_rejected() {
        let bad = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"bad name"}"#;
        let t = FakeTransport::new(vec![
            (200, XBOX),
            (200, XBOX),
            (200, MC),
            (200, ENT),
            (200, bad),
        ]);
        let tokens = MsaTokens {
            access_token: Secret::new("a"),
            refresh_token: Secret::new("r"),
        };
        let err = login_from_msa_token(&t, tokens).await.err().unwrap();
        assert!(matches!(err, AuthError::InvalidUsername));
    }

    #[test]
    fn no_error_message_contains_a_token() {
        let errors = [
            AuthError::LoginDeclined,
            AuthError::LoginExpired,
            AuthError::LoginCancelled,
            AuthError::NoXboxAccount,
            AuthError::ChildAccount,
            AuthError::XboxUnavailableInCountry,
            AuthError::AdultVerificationRequired,
            AuthError::XboxBanned,
            AuthError::NoMinecraftLicence,
            AuthError::NoMinecraftProfile,
            crate::microsoft::unexpected("xsts", 500),
        ];
        for e in errors {
            let text = e.to_string();
            assert!(
                !text.contains("XBL3.0") && !text.contains("Bearer"),
                "{text}"
            );
        }
    }

    #[tokio::test]
    async fn failing_steps_never_leak_tokens_in_errors() {
        // Every step answers with a body that holds a token but is not the
        // expected shape.
        let body = r#"{"Token":"LEAK-ME","access_token":"LEAK-ME"}"#;
        let t = FakeTransport::new(vec![(200, body)]);
        let tokens = MsaTokens {
            access_token: Secret::new("LEAK-ME"),
            refresh_token: Secret::new("LEAK-ME"),
        };
        let err = login_from_msa_token(&t, tokens).await.err().unwrap();
        assert!(!err.to_string().contains("LEAK-ME"));
        assert!(!format!("{err:?}").contains("LEAK-ME"));
    }
}
