// SPDX-License-Identifier: MIT OR Apache-2.0

//! The account flow without a window or a network: job results are injected.

use umicraft_auth::{AuthError, MicrosoftLogin, RenewedSession, Secret, Session};
use umicraft_ui::menus::AccountsOutcome;

use super::*;

const UUID: &str = "069a79f444e94726a5befca90e38aaf5";

fn login() -> MicrosoftLogin {
    MicrosoftLogin {
        username: "Notch".into(),
        uuid: UUID.into(),
        minecraft_token: Secret::new("mc"),
        minecraft_expires_in_secs: 86_400,
        refresh_token: Secret::new("r1"),
    }
}

fn is_default(identity: &Identity) -> bool {
    matches!(identity, Identity::Default)
}

#[test]
fn without_accounts_the_client_keeps_its_default_name() {
    let flow = AccountsFlow::in_memory();
    assert!(is_default(&flow.identity()));
    assert_eq!(flow.active_name(), None);
}

#[test]
fn an_added_offline_account_becomes_the_identity() {
    let mut flow = AccountsFlow::in_memory();
    flow.apply_outcome(AccountsOutcome::AddOffline("Pixel".into()));
    assert!(matches!(flow.identity(), Identity::Offline(name) if name == "Pixel"));
    assert_eq!(flow.active_name().as_deref(), Some("Pixel"));
}

#[test]
fn playing_an_account_selects_it_and_returns_to_the_server_list() {
    let mut flow = AccountsFlow::in_memory();
    flow.apply_outcome(AccountsOutcome::AddOffline("A".into()));
    flow.apply_outcome(AccountsOutcome::AddOffline("B".into()));
    let leave = flow.apply_outcome(AccountsOutcome::Play(1));
    assert_eq!(leave, Some(Screen::ServerList));
    assert_eq!(flow.active_name().as_deref(), Some("B"));
}

#[test]
fn a_finished_microsoft_login_is_saved_selected_and_ready() {
    let mut flow = AccountsFlow::in_memory();
    flow.finish_login(Ok(login()));
    assert!(matches!(flow.identity(), Identity::Online(s) if s.username == "Notch"));
}

#[test]
fn a_cancelled_login_leaves_no_account_and_no_error() {
    let mut flow = AccountsFlow::in_memory();
    flow.finish_login(Err(AuthError::LoginCancelled));
    assert!(is_default(&flow.identity()));
}

#[test]
fn deleting_the_selected_account_falls_back_to_the_next_identity() {
    let mut flow = AccountsFlow::in_memory();
    flow.finish_login(Ok(login()));
    flow.apply_outcome(AccountsOutcome::Delete(0));
    assert!(is_default(&flow.identity()));
}

#[test]
fn a_microsoft_account_is_not_ready_until_its_session_is_renewed() {
    let mut flow = AccountsFlow::in_memory();
    flow.finish_login(Ok(login()));
    flow.session = SessionState::Renewing;
    assert!(matches!(flow.identity(), Identity::NotReady));

    let session = Session::new("Notch", UUID, Secret::new("fresh")).expect("valid");
    flow.finish_renewal(
        UUID,
        Ok(RenewedSession {
            session,
            refresh_token: Secret::new("r2"),
        }),
    );
    assert!(matches!(flow.identity(), Identity::Online(s) if s.access_token.expose() == "fresh"));
}

#[test]
fn a_renewal_of_another_account_is_ignored() {
    let mut flow = AccountsFlow::in_memory();
    flow.finish_login(Ok(login()));
    flow.session = SessionState::Renewing;
    flow.finish_renewal("someone-else", Err(AuthError::LoginExpired));
    assert!(matches!(flow.session, SessionState::Renewing));
}

#[test]
fn a_refused_renewal_makes_the_account_not_ready() {
    let mut flow = AccountsFlow::in_memory();
    flow.finish_login(Ok(login()));
    flow.finish_renewal(UUID, Err(AuthError::LoginExpired));
    assert!(matches!(flow.identity(), Identity::NotReady));
}
