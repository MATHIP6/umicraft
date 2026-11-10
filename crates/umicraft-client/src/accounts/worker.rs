// SPDX-License-Identifier: MIT OR Apache-2.0

//! The network jobs of the account manager: signing in with Microsoft and
//! renewing a session. Each job runs on its own short-lived thread with its
//! own runtime and reports through a channel, so the window never waits on the
//! network (the same shape as the server pinger).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use umicraft_auth::{
    full_login, renew_session, request_device_code, AuthError, DeviceCode, MicrosoftLogin,
    RenewedSession, ReqwestTransport, Secret, TokioSleeper,
};

/// What a job tells the window.
pub enum Event {
    /// The player must open the page and type the code.
    DeviceCode(DeviceCode),
    /// The sign-in ended.
    LoggedIn(Result<MicrosoftLogin, AuthError>),
    /// A session renewal ended.
    Renewed {
        /// The account that was renewed.
        uuid: String,
        /// The new session and refresh token.
        result: Result<RenewedSession, AuthError>,
    },
}

/// Runs `job` to completion on a runtime of its own, on a named thread.
///
/// A thread or runtime that cannot start is logged and the job is dropped; the
/// window then keeps showing its busy state, which the cancel button ends.
fn spawn(name: &str, job: impl FnOnce(&tokio::runtime::Runtime) + Send + 'static) {
    let started = std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => job(&runtime),
                Err(error) => tracing::warn!(%error, "could not start the account runtime"),
            }
        });
    if let Err(error) = started {
        tracing::warn!(%error, "could not start the account thread");
    }
}

/// Starts a device-code sign-in. `cancel` stops the polling.
pub fn spawn_login(events: Sender<Event>, cancel: Arc<AtomicBool>) {
    spawn("umicraft-msa-login", move |runtime| {
        let result = runtime.block_on(async {
            let transport = ReqwestTransport::new()?;
            let device = request_device_code(&transport).await?;
            // A closed window drops the receiver; the send error is then moot.
            let _ = events.send(Event::DeviceCode(device.clone()));
            full_login(&transport, &TokioSleeper, &device, || {
                cancel.load(Ordering::Relaxed)
            })
            .await
        });
        let _ = events.send(Event::LoggedIn(result));
    });
}

/// Renews the session of account `uuid` from its refresh token.
pub fn spawn_renewal(events: Sender<Event>, uuid: String, refresh: Secret) {
    spawn("umicraft-msa-renew", move |runtime| {
        let result = runtime.block_on(async {
            let transport = ReqwestTransport::new()?;
            renew_session(&transport, &refresh).await
        });
        let _ = events.send(Event::Renewed { uuid, result });
    });
}
