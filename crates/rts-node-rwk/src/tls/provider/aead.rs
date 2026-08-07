//! AES-128-GCM and ChaCha20-Poly1305 for TLS 1.3, over `aes-gcm`/
//! `chacha20poly1305` — the exact crates `crates.md` §6 names, and the same
//! ones `node:crypto`'s symmetric-cipher entry (not yet built, per that
//! module's own "Not implemented") would reach for.
//!
//! AES-256-GCM is not implemented (see `mod.rs`'s "Not implemented, by
//! name"): it is mechanically the same shape as AES-128-GCM with a longer
//! key and is deferred purely for time, not for a technical reason.
//!
//! # Why `*_detached`
//!
//! Both crates implement `aead::AeadInPlace`, whose non-detached form wants
//! a growable `aead::Buffer` to append the tag onto. `rustls::crypto::cipher`
//! hands this code a fixed `&mut [u8]` (a `PrefixedPayload`'s already-sized
//! window on encrypt, a `BorrowedPayload` on decrypt) — exactly what the
//! `_detached` methods want instead, with the tag kept and moved separately.

use aes_gcm::Aes128Gcm;
use aes_gcm::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::ChaCha20Poly1305;
use rustls::crypto::cipher::{
    AeadKey, InboundOpaqueMessage, InboundPlainMessage, Iv, MessageDecrypter, MessageEncrypter,
    Nonce, OutboundOpaqueMessage, OutboundPlainMessage, PrefixedPayload, Tls13AeadAlgorithm, UnsupportedOperationError,
    make_tls13_aad,
};
use rustls::{ConnectionTrafficSecrets, Error};

const TAG_LEN: usize = 16;

pub(crate) struct Aes128GcmTls13;
pub(crate) struct ChaCha20Poly1305Tls13;

impl Tls13AeadAlgorithm for Aes128GcmTls13 {
    fn encrypter(&self, key: AeadKey, iv: Iv) -> Box<dyn MessageEncrypter> {
        Box::new(Tls13Encrypter::Aes128(
            Aes128Gcm::new_from_slice(key.as_ref()).expect("AES-128-GCM key is 16 bytes"),
            iv,
        ))
    }

    fn decrypter(&self, key: AeadKey, iv: Iv) -> Box<dyn MessageDecrypter> {
        Box::new(Tls13Decrypter::Aes128(
            Aes128Gcm::new_from_slice(key.as_ref()).expect("AES-128-GCM key is 16 bytes"),
            iv,
        ))
    }

    fn key_len(&self) -> usize {
        16
    }

    fn extract_keys(&self, key: AeadKey, iv: Iv) -> Result<ConnectionTrafficSecrets, UnsupportedOperationError> {
        Ok(ConnectionTrafficSecrets::Aes128Gcm { key, iv })
    }
}

impl Tls13AeadAlgorithm for ChaCha20Poly1305Tls13 {
    fn encrypter(&self, key: AeadKey, iv: Iv) -> Box<dyn MessageEncrypter> {
        Box::new(Tls13Encrypter::ChaCha(
            ChaCha20Poly1305::new_from_slice(key.as_ref()).expect("ChaCha20-Poly1305 key is 32 bytes"),
            iv,
        ))
    }

    fn decrypter(&self, key: AeadKey, iv: Iv) -> Box<dyn MessageDecrypter> {
        Box::new(Tls13Decrypter::ChaCha(
            ChaCha20Poly1305::new_from_slice(key.as_ref()).expect("ChaCha20-Poly1305 key is 32 bytes"),
            iv,
        ))
    }

    fn key_len(&self) -> usize {
        32
    }

    fn extract_keys(&self, key: AeadKey, iv: Iv) -> Result<ConnectionTrafficSecrets, UnsupportedOperationError> {
        Ok(ConnectionTrafficSecrets::Chacha20Poly1305 { key, iv })
    }
}

enum Tls13Encrypter {
    Aes128(Aes128Gcm, Iv),
    ChaCha(ChaCha20Poly1305, Iv),
}

impl MessageEncrypter for Tls13Encrypter {
    fn encrypt(&mut self, msg: OutboundPlainMessage<'_>, seq: u64) -> Result<OutboundOpaqueMessage, Error> {
        let total_len = self.encrypted_payload_len(msg.payload.len());
        let mut payload = PrefixedPayload::with_capacity(total_len);
        payload.extend_from_chunks(&msg.payload);
        payload.extend_from_slice(&msg.typ.to_array());
        let aad = make_tls13_aad(total_len);

        let tag = match self {
            Self::Aes128(cipher, iv) => {
                let nonce = Nonce::new(iv, seq);
                cipher
                    .encrypt_in_place_detached(nonce.0.as_ref().into(), &aad, payload.as_mut())
                    .map_err(|_| Error::EncryptError)?
            }
            Self::ChaCha(cipher, iv) => {
                let nonce = Nonce::new(iv, seq);
                cipher
                    .encrypt_in_place_detached(nonce.0.as_ref().into(), &aad, payload.as_mut())
                    .map_err(|_| Error::EncryptError)?
            }
        };
        payload.extend_from_slice(&tag);

        Ok(OutboundOpaqueMessage::new(
            rustls::ContentType::ApplicationData,
            rustls::ProtocolVersion::TLSv1_2,
            payload,
        ))
    }

    fn encrypted_payload_len(&self, payload_len: usize) -> usize {
        payload_len + 1 + TAG_LEN
    }
}

enum Tls13Decrypter {
    Aes128(Aes128Gcm, Iv),
    ChaCha(ChaCha20Poly1305, Iv),
}

impl MessageDecrypter for Tls13Decrypter {
    fn decrypt<'a>(&mut self, mut msg: InboundOpaqueMessage<'a>, seq: u64) -> Result<InboundPlainMessage<'a>, Error> {
        let payload = &mut msg.payload;
        if payload.len() < TAG_LEN {
            return Err(Error::DecryptError);
        }
        let aad = make_tls13_aad(payload.len());
        let message_len = payload.len();
        let (body, tag) = payload.split_at_mut(message_len - TAG_LEN);
        let tag_bytes: [u8; TAG_LEN] = tag.try_into().expect("split at TAG_LEN");

        match self {
            Self::Aes128(cipher, iv) => {
                let nonce = Nonce::new(iv, seq);
                cipher
                    .decrypt_in_place_detached(nonce.0.as_ref().into(), &aad, body, &tag_bytes.into())
                    .map_err(|_| Error::DecryptError)?;
            }
            Self::ChaCha(cipher, iv) => {
                let nonce = Nonce::new(iv, seq);
                cipher
                    .decrypt_in_place_detached(nonce.0.as_ref().into(), &aad, body, &tag_bytes.into())
                    .map_err(|_| Error::DecryptError)?;
            }
        }
        payload.truncate(message_len - TAG_LEN);
        msg.into_tls13_unpadded_message()
    }
}
