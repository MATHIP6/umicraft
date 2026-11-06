// SPDX-License-Identifier: MIT OR Apache-2.0

//! The encrypting login mode of the fake server (NET-4 tests).
//!
//! Plays the server half of the 1.8 key exchange for real: a cached RSA-1024
//! key, an `Encryption Request` carrying its SPKI DER form and a random verify
//! token, then decryption and verification of the `Encryption Response`
//! before AES/CFB8 is switched on. The shared secret is never stored; only
//! its session-server hash is, so tests can compare it with the hash the
//! client registered at the (fake) session server.
//!
//! Source: community protocol documentation ("Encryption" page, pre-1.13).

use std::sync::{Arc, Mutex, OnceLock};

use rsa::pkcs8::EncodePublicKey;
use rsa::rand_core::{OsRng, RngCore};
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};
use umicraft_net::Connection;
use umicraft_protocol::encryption::server_id_hash;
use umicraft_protocol::packets::login::clientbound::EncryptionRequest;
use umicraft_protocol::{LoginClientbound, LoginServerbound};

use crate::capture::Capture;
use crate::fake_server::{observe, read_packet, send_packet, Shared};

/// Script for the real key exchange.
#[derive(Debug, Clone, Default)]
pub struct EncryptionScript {
    /// `server_id` of the request (vanilla servers send an empty string).
    pub server_id: String,
}

/// RSA-1024 is slow to generate in debug builds, so all tests share one key.
fn server_key() -> &'static RsaPrivateKey {
    static KEY: OnceLock<RsaPrivateKey> = OnceLock::new();
    KEY.get_or_init(|| {
        RsaPrivateKey::new(&mut OsRng, 1024).expect("RSA-1024 key generation succeeds")
    })
}

/// Runs the exchange and enables encryption on `conn`; `false` means the
/// session must end (socket trouble or a failed check, recorded in `observed`).
pub(crate) async fn negotiate(
    conn: &mut Connection,
    capture: &Option<Arc<Mutex<Capture>>>,
    observed: &Shared,
    script: &EncryptionScript,
) -> bool {
    let key = server_key();
    let Ok(public_key) = RsaPublicKey::from(key).to_public_key_der() else {
        return false;
    };
    let public_key = public_key.as_bytes().to_vec();
    let mut verify_token = vec![0u8; 4];
    OsRng.fill_bytes(&mut verify_token);

    let request = LoginClientbound::EncryptionRequest(EncryptionRequest {
        server_id: script.server_id.clone(),
        public_key: public_key.clone(),
        verify_token: verify_token.clone(),
    });
    if !send_packet(conn, capture, &request).await {
        return false;
    }
    let Some((id, body)) = read_packet(conn, capture).await else {
        return false;
    };
    let Ok(LoginServerbound::EncryptionResponse(response)) =
        LoginServerbound::decode(id, &mut body.as_slice())
    else {
        tracing::trace!(packet_id = id, "fake server: expected encryption response");
        return false;
    };
    observe(observed, |state| state.encryption_responses += 1);

    let secret = key.decrypt(Pkcs1v15Encrypt, &response.shared_secret);
    let token = key.decrypt(Pkcs1v15Encrypt, &response.nonce);
    let accepted = match (secret, token) {
        (Ok(secret), Ok(token)) if token == verify_token => {
            conn.enable_encryption(&secret).is_ok().then(|| {
                let hash = server_id_hash(&script.server_id, &secret, &public_key);
                observe(observed, |state| state.encryption_hash = Some(hash));
            })
        }
        _ => None,
    };
    if accepted.is_none() {
        observe(observed, |state| state.encryption_failures += 1);
    }
    accepted.is_some()
}
