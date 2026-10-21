// SPDX-License-Identifier: MIT OR Apache-2.0

//! Encryption handshake helpers (NET-4 groundwork for M5 online-mode login).
//!
//! 1.8 quirks honored here:
//!
//! - **Key and IV are the same 16-byte shared secret.** The protocol uses the
//!   raw shared secret twice, as the AES-128 key *and* the CFB8 IV, once for
//!   each stream direction.
//! - **Shared secret and verify token travel RSA-wrapped** (PKCS#1 v1.5
//!   padding, 128-byte blocks at the 1024-bit vanilla key size).
//! - **Server id hash**: `SHA1(shared_secret || server_nonce)` rendered as a
//!   *signed* hex string (leading `-`, no leading zeros), matching what the
//!   Mojang session server looked up on its join endpoint.
//!
//! The CFB8 stream itself is implemented here on top of `aes::Aes128`: CFB8 is
//! a fully specified mode (`c = p XOR AES(K, IV)[0]`, then shift the IV left
//! by one byte with the ciphertext), and the RustCrypto `cfb8` crate's APIs
//! consume the cipher object, which does not match a stateful connection
//! stream. The implementation is pinned to the crate's published test vector
//! in the tests below.
//!
//! Sources: community protocol documentation ("Encryption" and "Protocol
//! Details / Server ID hash" in the pre-1.13 wiki.vg pages); CFB8 definition
//! per NIST SP 800-38A. This module does not authenticate anything by itself;
//! a later auth crate owns the premium flow.

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockEncrypt, KeyInit};
use aes::Aes128;
use num_bigint::BigInt;
use rsa::pkcs8::DecodePublicKey;
use rsa::RsaPublicKey;
use sha1::{Digest, Sha1};

use crate::error::ProtocolError;

/// Length of the AES-128 key and IV. The protocol sends 16 bytes.
pub const SHARED_SECRET_LEN: usize = 16;

/// Stateful AES-128/CFB8 byte stream (one instance per direction).
pub struct AesCfb8Stream {
    aes: Aes128,
    iv: [u8; SHARED_SECRET_LEN],
}

// Keys and IVs must never show up in logs; no derived Debug.
impl std::fmt::Debug for AesCfb8Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AesCfb8Stream(..)")
    }
}

impl AesCfb8Stream {
    /// Builds the stream from a 16-byte key and 16-byte initial IV.
    ///
    /// # Errors
    /// [`ProtocolError::Invariant`] when either slice is not 16 bytes.
    pub fn new(key: &[u8], iv: &[u8]) -> Result<Self, ProtocolError> {
        if key.len() != SHARED_SECRET_LEN || iv.len() != SHARED_SECRET_LEN {
            return Err(ProtocolError::Invariant(format!(
                "AES-128/CFB8 needs 16-byte key and IV, got {}/{}",
                key.len(),
                iv.len()
            )));
        }
        let mut iv_owned = [0u8; SHARED_SECRET_LEN];
        iv_owned.copy_from_slice(iv);
        let aes = Aes128::new_from_slice(key)
            .map_err(|_| ProtocolError::Invariant("AES-128 key must be 16 bytes".to_string()))?;
        Ok(Self { aes, iv: iv_owned })
    }

    /// Encrypts `buf` in place for transmission to the server.
    pub fn encrypt(&mut self, buf: &mut [u8]) {
        self.process(buf, true);
    }

    /// Decrypts `buf` in place after it arrived from the server.
    pub fn decrypt(&mut self, buf: &mut [u8]) {
        self.process(buf, false);
    }

    /// CFB8 core: for every byte, the keystream is the first byte of
    /// `AES(K, IV)`; the IV then shifts left by one byte, appending the
    /// ciphertext byte (identity for both directions, since decryption reads
    /// the same ciphertext and feeds it back — NIST SP 800-38A CFB mode).
    fn process(&mut self, buf: &mut [u8], encrypting: bool) {
        for byte in buf.iter_mut() {
            let mut block = GenericArray::clone_from_slice(&self.iv);
            self.aes.encrypt_block(&mut block);
            let keystream = block[0];

            let out = *byte ^ keystream;
            // Shift the IV left one byte; the ciphertext feeds back in from
            // the right in both directions (it is the input when decrypting).
            self.iv.copy_within(1.., 0);
            self.iv[SHARED_SECRET_LEN - 1] = if encrypting { out } else { *byte };
            *byte = out;
        }
    }
}

/// The two independent ciphers a 1.8 connection keeps after encryption.
pub struct ConnectionCiphers {
    /// Applies to bytes going serverbound.
    pub encrypt: AesCfb8Stream,
    /// Applies to bytes arriving clientbound.
    pub decrypt: AesCfb8Stream,
}

impl std::fmt::Debug for ConnectionCiphers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionCiphers").finish_non_exhaustive()
    }
}

impl ConnectionCiphers {
    /// Derives the pair from the raw shared secret (key == IV == secret).
    ///
    /// # Errors
    /// [`ProtocolError::Invariant`] when the secret is not exactly 16 bytes.
    pub fn derive(shared_secret: &[u8]) -> Result<Self, ProtocolError> {
        if shared_secret.len() != SHARED_SECRET_LEN {
            return Err(ProtocolError::Invariant(format!(
                "shared secret must be {SHARED_SECRET_LEN} bytes, got {}",
                shared_secret.len()
            )));
        }
        // Key == IV == raw shared secret (1.8 protocol quirk, see module docs).
        Ok(Self {
            encrypt: AesCfb8Stream::new(shared_secret, shared_secret)?,
            decrypt: AesCfb8Stream::new(shared_secret, shared_secret)?,
        })
    }
}

/// Encrypts `data` with the server's SPKI-encoded public key (PKCS#1 v1.5).
///
/// # Errors
/// [`ProtocolError::Invariant`] when the DER bytes do not parse as a public
/// key, or when the RSA engine rejects the input.
pub fn rsa_encrypt(public_key_der: &[u8], data: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    let key = RsaPublicKey::from_public_key_der(public_key_der)
        .map_err(|e| ProtocolError::Invariant(format!("malformed server public key: {e}")))?;
    use rsa::rand_core::OsRng;
    key.encrypt(&mut OsRng, rsa::Pkcs1v15Encrypt, data)
        .map_err(|e| ProtocolError::Invariant(format!("rsa encryption failed: {e}")))
}

/// Computes the session-server "server hash string": signed SHA-1 hex of
/// `sha1(shared_secret || server_nonce)`.
#[must_use]
pub fn server_id_hash(shared_secret: &[u8], server_nonce: &[u8]) -> String {
    let mut sha = Sha1::new();
    Digest::update(&mut sha, shared_secret);
    Digest::update(&mut sha, server_nonce);
    let digest = sha.finalize();
    BigInt::from_signed_bytes_be(&digest).to_str_radix(16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cipher_pair_round_trips_basic_streams() {
        let secret = [0x55u8; 16];
        let mut ciphers = ConnectionCiphers::derive(&secret).unwrap();

        let plaintext: Vec<u8> = (0..=255).collect();
        let mut encrypted = plaintext.clone();
        ciphers.encrypt.encrypt(&mut encrypted);

        // CFB8 with our key must not be identity — and it must not be a fixed
        // keystream XOR (the IV feedback makes each byte context-dependent).
        assert_ne!(encrypted, plaintext);

        let mut decrypted = encrypted;
        ciphers.decrypt.decrypt(&mut decrypted);
        assert_eq!(decrypted, plaintext);
    }

    /// Conformance against the vector published in the cfb8 0.8 crate docs
    /// (key 0x42*16, iv 0x24*16); proves this CFB8 loop byte-exactly without
    /// trusting our own round-trip alone.
    #[test]
    fn matches_published_cfb8_vector() {
        let mut stream = AesCfb8Stream::new(&[0x42; 16], &[0x24; 16]).unwrap();
        let mut buf = *b"hello world! this is my plaintext.";
        stream.encrypt(&mut buf);
        assert_eq!(
            buf,
            [
                0x33, 0xb3, 0x56, 0xce, 0x91, 0x84, 0x29, 0x0c, 0x4c, 0x8f, 0xac, 0xc1, 0xc0, 0xb1,
                0xf9, 0x18, 0xd5, 0x47, 0x5a, 0xeb, 0x75, 0xb8, 0x8c, 0x16, 0x1c, 0xa6, 0x5b, 0xdf,
                0x05, 0xc7, 0x13, 0x7f, 0xf4, 0xb0
            ]
        );
    }

    /// CFB8 IV chaining must carry across successive calls, since real
    /// connections encrypt packet-by-packet with the same stream.
    #[test]
    fn iv_state_persists_across_calls() {
        let mut stream = AesCfb8Stream::new(&[0x11; 16], &[0x22; 16]).unwrap();
        let mut first = [0x61u8; 10];
        let mut second = first;
        stream.encrypt(&mut first);
        stream.encrypt(&mut second);
        // Same plaintext twice must differ because the IV advanced.
        assert_ne!(first, second);
    }

    #[test]
    fn wrong_lengths_are_invariant() {
        assert!(matches!(
            AesCfb8Stream::new(&[0u8; 8], &[0u8; 16]),
            Err(ProtocolError::Invariant(_))
        ));
        assert!(matches!(
            ConnectionCiphers::derive(&[0u8; 17]),
            Err(ProtocolError::Invariant(_))
        ));
    }

    #[test]
    fn rsa_wrap_round_trip_through_a_local_key() {
        use rsa::pkcs8::EncodePublicKey;
        let priv_key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 1024).unwrap();
        let public_der = priv_key
            .to_public_key()
            .to_public_key_der()
            .expect("spki der");
        let payload = b"umicraft-shared-secret!!";

        let wrapped = rsa_encrypt(public_der.as_bytes(), payload).unwrap();
        assert_eq!(wrapped.len(), 128); // 1024-bit modulus
        let plain_back = priv_key.decrypt(rsa::Pkcs1v15Encrypt, &wrapped).unwrap();
        assert_eq!(plain_back, payload);
    }

    #[test]
    fn server_hash_matches_signed_bigint_hex() {
        // Deterministic input; the expected value comes from sha1 of the two
        // concatenated slices interpreted as a signed big-endian integer.
        let shared = b"0123456789abcdef";
        let nonce = [0x9Au8; 16];
        let mut sha = Sha1::new();
        Digest::update(&mut sha, shared);
        Digest::update(&mut sha, nonce);
        let digest = sha.finalize();

        let expected = BigInt::from_signed_bytes_be(&digest).to_str_radix(16);
        assert_eq!(server_id_hash(shared, &nonce), expected);

        // High-bit first byte must surface the '-' sign.
        let negative_input = [0xFFu8; 20];
        assert!(
            BigInt::from_signed_bytes_be(&negative_input)
                .to_str_radix(16)
                .starts_with('-'),
            "sanity: sign convention must apply to SHA1-first-byte negatives"
        );
        assert!(server_id_hash(&negative_input, &[]).starts_with('-'));
    }
}
