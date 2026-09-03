//! AES-128-GCM, AES-256-GCM and ChaCha20-Poly1305 for TLS 1.3, over
//! `aes-gcm`/`chacha20poly1305` — the exact crates `crates.md` §6 names, and
//! the same ones `node:crypto`'s symmetric-cipher entry (not yet built, per
//! that module's own "Not implemented") would reach for.
//!
//! AES-256-GCM is the mechanical copy of AES-128-GCM its own former doc
//! comment here predicted: same `AeadInPlace` calls, same nonce/AAD
//! construction, only the key type and `ConnectionTrafficSecrets` variant
//! differ.
//!
//! # Why `*_detached`
//!
//! Both crates implement `aead::AeadInPlace`, whose non-detached form wants
//! a growable `aead::Buffer` to append the tag onto. `rustls::crypto::cipher`
//! hands this code a fixed `&mut [u8]` (a `PrefixedPayload`'s already-sized
//! window on encrypt, a `BorrowedPayload` on decrypt) — exactly what the
//! `_detached` methods want instead, with the tag kept and moved separately.

use aes_gcm::{Aes128Gcm, Aes256Gcm};
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
pub(crate) struct Aes256GcmTls13;
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

impl Tls13AeadAlgorithm for Aes256GcmTls13 {
    fn encrypter(&self, key: AeadKey, iv: Iv) -> Box<dyn MessageEncrypter> {
        Box::new(Tls13Encrypter::Aes256(
            Aes256Gcm::new_from_slice(key.as_ref()).expect("AES-256-GCM key is 32 bytes"),
            iv,
        ))
    }

    fn decrypter(&self, key: AeadKey, iv: Iv) -> Box<dyn MessageDecrypter> {
        Box::new(Tls13Decrypter::Aes256(
            Aes256Gcm::new_from_slice(key.as_ref()).expect("AES-256-GCM key is 32 bytes"),
            iv,
        ))
    }

    fn key_len(&self) -> usize {
        32
    }

    fn extract_keys(&self, key: AeadKey, iv: Iv) -> Result<ConnectionTrafficSecrets, UnsupportedOperationError> {
        Ok(ConnectionTrafficSecrets::Aes256Gcm { key, iv })
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
    Aes256(Aes256Gcm, Iv),
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
            Self::Aes256(cipher, iv) => {
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
    Aes256(Aes256Gcm, Iv),
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
            Self::Aes256(cipher, iv) => {
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

/// # Why these tests exist, and what they can and cannot prove
///
/// `getCiphers()` listing `"tls_aes_256_gcm_sha384"` is not evidence the AEAD
/// actually works — the CLAUDE.md rule this crate is held to is explicit that
/// a name must not ship ahead of what it names. There is no self-signed-cert
/// fixture anywhere in this repository to drive a real client/server TLS 1.3
/// handshake over THIS specific suite (`node:tls`'s cipher choice is not
/// exposed to a caller either), so what these prove instead: [`round_trips`]
/// exercises [`Tls13Encrypter::Aes256`]/[`Tls13Decrypter::Aes256`] — the code
/// THIS change wrote — end to end; [`tamper_is_rejected`] proves the tag
/// actually authenticates rather than being computed and ignored;
/// [`matches_the_underlying_crate`] proves the framing (nonce, AAD, tag
/// placement) this arm produces is byte-identical to calling `aes-gcm`
/// directly, independent of trusting this module's own dispatch. The
/// underlying primitive (`aes_gcm::Aes256Gcm` itself) is `aes-gcm`'s own
/// concern to test, not this crate's.
#[cfg(test)]
mod tests {
    use super::*;
    use rustls::crypto::cipher::OutboundChunks;
    use rustls::{ContentType, ProtocolVersion};

    fn plain<'a>(chunks: &'a [&'a [u8]]) -> OutboundPlainMessage<'a> {
        OutboundPlainMessage {
            typ: ContentType::ApplicationData,
            version: ProtocolVersion::TLSv1_2,
            payload: OutboundChunks::new(chunks),
        }
    }

    #[test]
    fn round_trips() {
        let key = [0x11u8; 32];
        let iv = [0x22u8; 12];
        let plaintext = b"TLS 1.3 AES-256-GCM round trip";

        let mut encrypter = Aes256GcmTls13.encrypter(AeadKey::from(key), Iv::new(iv));
        let opaque = encrypter.encrypt(plain(&[plaintext]), 3).expect("encrypt");
        let mut wire = opaque.payload.as_ref().to_vec();

        let mut decrypter = Aes256GcmTls13.decrypter(AeadKey::from(key), Iv::new(iv));
        let inbound = InboundOpaqueMessage::new(opaque.typ, opaque.version, &mut wire);
        let recovered = decrypter.decrypt(inbound, 3).expect("decrypt");
        assert_eq!(recovered.payload, plaintext);
        assert_eq!(recovered.typ, ContentType::ApplicationData);
    }

    #[test]
    fn tamper_is_rejected() {
        let key = [0x33u8; 32];
        let iv = [0x44u8; 12];
        let mut encrypter = Aes256GcmTls13.encrypter(AeadKey::from(key), Iv::new(iv));
        let opaque = encrypter.encrypt(plain(&[b"authenticate me"]), 0).expect("encrypt");
        let mut wire = opaque.payload.as_ref().to_vec();
        // Flip one bit inside the ciphertext, not the tag — either way the
        // decrypt must fail, but this proves the CIPHERTEXT is covered too.
        wire[0] ^= 0x01;

        let mut decrypter = Aes256GcmTls13.decrypter(AeadKey::from(key), Iv::new(iv));
        let inbound = InboundOpaqueMessage::new(opaque.typ, opaque.version, &mut wire);
        assert!(decrypter.decrypt(inbound, 0).is_err(), "a tampered ciphertext must not decrypt");
    }

    /// Reconstructs the same ciphertext by calling `aes_gcm::Aes256Gcm`
    /// directly — the nonce/AAD helpers are shared with the already-shipping
    /// AES-128 arm, so what is actually new here is the key length and which
    /// cipher gets constructed, which is exactly what this isolates.
    #[test]
    fn matches_the_underlying_crate() {
        let key = [0x55u8; 32];
        let iv_bytes = [0x66u8; 12];
        let seq = 7u64;
        let plaintext: &[u8] = b"cross-check payload, longer than one block of AES";

        let mut encrypter = Aes256GcmTls13.encrypter(AeadKey::from(key), Iv::new(iv_bytes));
        let opaque = encrypter.encrypt(plain(&[plaintext]), seq).expect("encrypt");
        let wired: &[u8] = opaque.payload.as_ref();

        let cipher = Aes256Gcm::new_from_slice(&key).expect("32-byte key");
        let nonce = Nonce::new(&Iv::new(iv_bytes), seq);
        let mut buffer = plaintext.to_vec();
        buffer.push(ContentType::ApplicationData.into());
        let aad = make_tls13_aad(buffer.len() + TAG_LEN);
        let tag = cipher
            .encrypt_in_place_detached(nonce.0.as_ref().into(), &aad, &mut buffer)
            .expect("encrypt");
        buffer.extend_from_slice(&tag);

        assert_eq!(wired, buffer.as_slice());
    }
}
