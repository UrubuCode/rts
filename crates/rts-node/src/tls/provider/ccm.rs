//! `TLS_AES_128_CCM_SHA256` and `TLS_AES_128_CCM_8_SHA256` for TLS 1.3, over
//! the `ccm` crate (RustCrypto) — added to `crates.md` §6 alongside
//! `aes-gcm`/`chacha20poly1305`, the two crates `aead.rs` already uses. Its
//! own module doc explains why THIS file exists separately from `aead.rs`
//! rather than growing that one past the workspace's 500-line ceiling, and
//! why CCM needed a real implementation rather than a copy of GCM's.
//!
//! # CCM is not GCM with a different name
//!
//! GCM is CTR-mode encryption plus a universal hash (GHASH) over a Galois
//! field — one pass, the tag falls out of the same field multiplication that
//! processes the ciphertext. CCM is CTR-mode encryption plus a **separate**
//! CBC-MAC pass over the plaintext (RFC 3610) — the tag is computed first, in
//! its own pass, from an entirely different construction than the one that
//! then encrypts it. `aes-gcm` and `ccm` therefore share no code beneath the
//! `aead::AeadInPlace` trait either; picking the CCM instance is a distinct
//! `Tls13AeadAlgorithm` impl, not a parameter on the GCM one.
//!
//! # The one place the two suites genuinely differ: tag length
//! `TLS_AES_128_CCM_SHA256` uses the full 16-octet CCM tag
//! ([`ccm::consts::U16`]); `TLS_AES_128_CCM_8_SHA256` truncates it to 8
//! octets ([`ccm::consts::U8`]) — RFC 8446 §5.5 lists a fourth and fifth
//! suite for exactly this reason, not two names for one construction. Both
//! use a 12-byte nonce ([`ccm::consts::U12`]), same as every TLS 1.3 AEAD
//! `aead.rs` already builds ([`rustls::crypto::cipher::Nonce`] is fixed at 12
//! bytes), and SHA-256 — reused from `hash.rs`, not duplicated here.
//!
//! # `confidentiality_limit`: no RFC 8446 number to copy, so none is claimed
//! `AES_128_GCM_SHA256`/`AES_256_GCM_SHA384` in `mod.rs` copy `1 << 24` from
//! rustls's own `ring` provider (an approximation of the GCM confidentiality
//! bound RFC 8446 §5.5 states as 2^24.5 full-size records). `ring` never
//! implemented CCM, so there is no equivalent line anywhere in rustls to
//! copy, and RFC 8446 §5.5's own text gives a number for AES-GCM and for
//! ChaCha20-Poly1305 but not for CCM/CCM_8 — that split is treated in the
//! IRTF CFRG "Usage Limits on AEAD Algorithms" analysis instead
//! (`draft-irtf-cfrg-aead-limits`), which is a *different*, later document.
//! Its own worked example, at TLS-record-shaped parameters (l ≈ 27 blocks,
//! forgery probability 2^-50 — looser than the 2^-57 TLS actually targets, so
//! the true TLS-safe number is smaller still), puts full-tag CCM's usage
//! bound near 2^30 and CCM_8's INTEGRITY bound — attempted forgeries
//! tolerated, not the same axis as `confidentiality_limit` — down near 2^14,
//! a `2^64` reduction from the full tag caused by the truncated tag alone.
//! Rather than dress an order-of-magnitude reading of someone else's worked
//! example up as an RFC 8446 citation it is not, the two suites here pick
//! numbers that are *deliberately conservative* relative to that reading —
//! forcing a `KeyUpdate` earlier than the analysis requires costs a
//! renegotiation, never a security margin, and CCM_8's already-thin 64-bit
//! forgery resistance gets the more conservative treatment of the two:
//! `1 << 24` (same as this file's GCM neighbours — comfortably under the
//! ~2^30 figure above) for the full-tag suite, `1 << 10` (three orders of
//! magnitude under that ~2^14 reading) for CCM_8.

use aes::Aes128;
use ccm::Ccm;
use ccm::aead::{AeadInPlace, KeyInit};
use ccm::consts::{U8, U12, U16};
use rustls::crypto::cipher::{
    AeadKey, InboundOpaqueMessage, InboundPlainMessage, Iv, MessageDecrypter, MessageEncrypter,
    Nonce, OutboundOpaqueMessage, OutboundPlainMessage, PrefixedPayload, Tls13AeadAlgorithm, UnsupportedOperationError,
    make_tls13_aad,
};
use rustls::{ConnectionTrafficSecrets, Error};

type Aes128Ccm = Ccm<Aes128, U16, U12>;
type Aes128Ccm8 = Ccm<Aes128, U8, U12>;

const TAG_LEN_FULL: usize = 16;
const TAG_LEN_8: usize = 8;

pub(crate) struct Aes128CcmTls13;
pub(crate) struct Aes128Ccm8Tls13;

impl Tls13AeadAlgorithm for Aes128CcmTls13 {
    fn encrypter(&self, key: AeadKey, iv: Iv) -> Box<dyn MessageEncrypter> {
        Box::new(Tls13Encrypter::Full(
            Aes128Ccm::new_from_slice(key.as_ref()).expect("AES-128-CCM key is 16 bytes"),
            iv,
        ))
    }

    fn decrypter(&self, key: AeadKey, iv: Iv) -> Box<dyn MessageDecrypter> {
        Box::new(Tls13Decrypter::Full(
            Aes128Ccm::new_from_slice(key.as_ref()).expect("AES-128-CCM key is 16 bytes"),
            iv,
        ))
    }

    fn key_len(&self) -> usize {
        16
    }

    // `rustls::ConnectionTrafficSecrets` has no CCM variant (only the GCM and
    // ChaCha20-Poly1305 arms `aead.rs` already returns) — a caller asking to
    // export the raw traffic secret for this suite gets a named refusal
    // rather than a value borrowed from a construction this isn't, which is
    // what `UnsupportedOperationError` is for.
    fn extract_keys(&self, _key: AeadKey, _iv: Iv) -> Result<ConnectionTrafficSecrets, UnsupportedOperationError> {
        Err(UnsupportedOperationError)
    }
}

impl Tls13AeadAlgorithm for Aes128Ccm8Tls13 {
    fn encrypter(&self, key: AeadKey, iv: Iv) -> Box<dyn MessageEncrypter> {
        Box::new(Tls13Encrypter::Short(
            Aes128Ccm8::new_from_slice(key.as_ref()).expect("AES-128-CCM_8 key is 16 bytes"),
            iv,
        ))
    }

    fn decrypter(&self, key: AeadKey, iv: Iv) -> Box<dyn MessageDecrypter> {
        Box::new(Tls13Decrypter::Short(
            Aes128Ccm8::new_from_slice(key.as_ref()).expect("AES-128-CCM_8 key is 16 bytes"),
            iv,
        ))
    }

    fn key_len(&self) -> usize {
        16
    }

    fn extract_keys(&self, _key: AeadKey, _iv: Iv) -> Result<ConnectionTrafficSecrets, UnsupportedOperationError> {
        Err(UnsupportedOperationError)
    }
}

enum Tls13Encrypter {
    Full(Aes128Ccm, Iv),
    Short(Aes128Ccm8, Iv),
}

impl MessageEncrypter for Tls13Encrypter {
    fn encrypt(&mut self, msg: OutboundPlainMessage<'_>, seq: u64) -> Result<OutboundOpaqueMessage, Error> {
        let total_len = self.encrypted_payload_len(msg.payload.len());
        let mut payload = PrefixedPayload::with_capacity(total_len);
        payload.extend_from_chunks(&msg.payload);
        payload.extend_from_slice(&msg.typ.to_array());
        let aad = make_tls13_aad(total_len);

        match self {
            Self::Full(cipher, iv) => {
                let nonce = Nonce::new(iv, seq);
                let tag = cipher
                    .encrypt_in_place_detached(nonce.0.as_ref().into(), &aad, payload.as_mut())
                    .map_err(|_| Error::EncryptError)?;
                payload.extend_from_slice(&tag);
            }
            Self::Short(cipher, iv) => {
                let nonce = Nonce::new(iv, seq);
                let tag = cipher
                    .encrypt_in_place_detached(nonce.0.as_ref().into(), &aad, payload.as_mut())
                    .map_err(|_| Error::EncryptError)?;
                payload.extend_from_slice(&tag);
            }
        }

        Ok(OutboundOpaqueMessage::new(
            rustls::ContentType::ApplicationData,
            rustls::ProtocolVersion::TLSv1_2,
            payload,
        ))
    }

    fn encrypted_payload_len(&self, payload_len: usize) -> usize {
        let tag_len = match self {
            Self::Full(..) => TAG_LEN_FULL,
            Self::Short(..) => TAG_LEN_8,
        };
        payload_len + 1 + tag_len
    }
}

enum Tls13Decrypter {
    Full(Aes128Ccm, Iv),
    Short(Aes128Ccm8, Iv),
}

impl MessageDecrypter for Tls13Decrypter {
    fn decrypt<'a>(&mut self, mut msg: InboundOpaqueMessage<'a>, seq: u64) -> Result<InboundPlainMessage<'a>, Error> {
        let tag_len = match self {
            Self::Full(..) => TAG_LEN_FULL,
            Self::Short(..) => TAG_LEN_8,
        };
        let payload = &mut msg.payload;
        if payload.len() < tag_len {
            return Err(Error::DecryptError);
        }
        let aad = make_tls13_aad(payload.len());
        let message_len = payload.len();
        let (body, tag) = payload.split_at_mut(message_len - tag_len);

        match self {
            Self::Full(cipher, iv) => {
                let nonce = Nonce::new(iv, seq);
                let tag_arr: [u8; TAG_LEN_FULL] = tag.try_into().expect("split at TAG_LEN_FULL");
                cipher
                    .decrypt_in_place_detached(nonce.0.as_ref().into(), &aad, body, &tag_arr.into())
                    .map_err(|_| Error::DecryptError)?;
            }
            Self::Short(cipher, iv) => {
                let nonce = Nonce::new(iv, seq);
                let tag_arr: [u8; TAG_LEN_8] = tag.try_into().expect("split at TAG_LEN_8");
                cipher
                    .decrypt_in_place_detached(nonce.0.as_ref().into(), &aad, body, &tag_arr.into())
                    .map_err(|_| Error::DecryptError)?;
            }
        }
        payload.truncate(message_len - tag_len);
        msg.into_tls13_unpadded_message()
    }
}

/// # Why these tests exist, and what they can and cannot prove
///
/// Same discipline as `aead.rs`'s test module doc: `getCiphers()` naming
/// these two suites is not evidence the AEAD works, and there is no
/// self-signed-cert fixture in this repository to drive a real handshake
/// over either. [`full_round_trips`]/[`short_round_trips`] exercise the
/// `MessageEncrypter`/`MessageDecrypter` wiring THIS file wrote — the nonce
/// derivation, AAD construction, and tag placement/length dispatch in
/// [`Tls13Encrypter::encrypt`]/[`Tls13Decrypter::decrypt`] — end to end, for
/// both tag lengths; [`full_tamper_is_rejected`]/[`short_tamper_is_rejected`]
/// prove the tag actually authenticates; [`full_matches_the_underlying_crate`]
/// proves the framing is byte-identical to calling `ccm::Ccm` directly,
/// independent of trusting this module's own dispatch — the CCM primitive
/// itself (`ccm::Ccm`, `aes::Aes128`) is upstream's concern, not this
/// crate's; [`short_matches_the_underlying_crate`] is the same check for the
/// `_8` variant, closing a gap the first version of this file actually had
/// (only the full-tag suite got a cross-check). One extra check
/// ([`ciphertext_matches_across_tag_lengths_but_tag_does_not`]) that GCM's
/// test module has no analogue for, because GCM has no tag-length variant to
/// compare: the two suites' CIPHERTEXT is byte-identical for the same
/// key/nonce/plaintext (`extend_nonce` — the counter-mode keystream — never
/// reads the tag-size type parameter), but their TAGS are not one a prefix
/// of the other, which is easy to assume wrongly (an earlier version of this
/// file asserted exactly that, and the test caught it): `ccm::Ccm::calc_mac`
/// folds `M::get_m_tick()` (the tag-length field) into the flags byte of the
/// very FIRST CBC-MAC block, so the two tag lengths run two genuinely
/// different MAC chains, not one chain with a later truncation.
///
/// # What these tests do NOT reach, and why: `AeadKey`'s 32-byte-only door
///
/// Every test below builds a [`Tls13Encrypter`]/[`Tls13Decrypter`] directly
/// (`Aes128Ccm::new_from_slice`/`Iv::new`) rather than going through
/// [`Aes128CcmTls13::encrypter`]/`::decrypter` the way production code and
/// `aead.rs`'s own `Aes256GcmTls13` tests do. That is not a style choice:
/// `rustls::crypto::cipher::AeadKey` has exactly one public constructor,
/// `From<[u8; 32]>` (its `MAX_LEN`, and the crate-private `with_length` that
/// trims a key to fewer bytes, are both `pub(crate)` inside rustls, not
/// visible here) — so from OUTSIDE the rustls crate, a real `AeadKey` can
/// only ever be built at exactly 32 bytes. That is why `aead.rs`'s test
/// module tests `Aes256GcmTls13` (32-byte key) and has never had a test for
/// `Aes128GcmTls13` or `ChaCha20Poly1305Tls13` — the same wall this file hit
/// for its own 16-byte AES-128 key. In real use this is not a gap: rustls's
/// own key-schedule (`tls13/key_schedule.rs`) builds the full 32-byte buffer
/// and calls the crate-private `with_length(suite.aead_alg.key_len())`
/// itself, so a live TLS 1.3 connection over either CCM suite DOES reach
/// [`Aes128CcmTls13::encrypter`]/`::decrypter` with a correctly-sized key —
/// only a unit test sitting outside the crate cannot construct one to call
/// them directly. What stays unit-tested here is everything past that: the
/// one-line factories themselves are `Cipher::new_from_slice(key.as_ref())`
/// plus an enum wrapper, small enough to read correctly by inspection, and
/// they run for real whenever `tests/node_tls_full.test.ts` completes a
/// handshake.
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

    fn full_pair(key: [u8; 16], iv: [u8; 12]) -> (Tls13Encrypter, Tls13Decrypter) {
        (
            Tls13Encrypter::Full(Aes128Ccm::new_from_slice(&key).expect("16-byte key"), Iv::new(iv)),
            Tls13Decrypter::Full(Aes128Ccm::new_from_slice(&key).expect("16-byte key"), Iv::new(iv)),
        )
    }

    fn short_pair(key: [u8; 16], iv: [u8; 12]) -> (Tls13Encrypter, Tls13Decrypter) {
        (
            Tls13Encrypter::Short(Aes128Ccm8::new_from_slice(&key).expect("16-byte key"), Iv::new(iv)),
            Tls13Decrypter::Short(Aes128Ccm8::new_from_slice(&key).expect("16-byte key"), Iv::new(iv)),
        )
    }

    #[test]
    fn full_round_trips() {
        let (mut encrypter, mut decrypter) = full_pair([0x11u8; 16], [0x22u8; 12]);
        let plaintext = b"TLS 1.3 AES-128-CCM round trip";

        let opaque = encrypter.encrypt(plain(&[plaintext]), 3).expect("encrypt");
        let mut wire = opaque.payload.as_ref().to_vec();

        let inbound = InboundOpaqueMessage::new(opaque.typ, opaque.version, &mut wire);
        let recovered = decrypter.decrypt(inbound, 3).expect("decrypt");
        assert_eq!(recovered.payload, plaintext);
        assert_eq!(recovered.typ, ContentType::ApplicationData);
    }

    #[test]
    fn short_round_trips() {
        let (mut encrypter, mut decrypter) = short_pair([0x33u8; 16], [0x44u8; 12]);
        let plaintext = b"TLS 1.3 AES-128-CCM_8 round trip";

        let opaque = encrypter.encrypt(plain(&[plaintext]), 9).expect("encrypt");
        let mut wire = opaque.payload.as_ref().to_vec();
        // The whole point of CCM_8 is a shorter tag: prove that landed, not
        // just that round-tripping worked (which would pass at any length).
        assert_eq!(wire.len(), plaintext.len() + 1 + TAG_LEN_8);

        let inbound = InboundOpaqueMessage::new(opaque.typ, opaque.version, &mut wire);
        let recovered = decrypter.decrypt(inbound, 9).expect("decrypt");
        assert_eq!(recovered.payload, plaintext);
    }

    #[test]
    fn full_tamper_is_rejected() {
        let (mut encrypter, mut decrypter) = full_pair([0x55u8; 16], [0x66u8; 12]);
        let opaque = encrypter.encrypt(plain(&[b"authenticate me"]), 0).expect("encrypt");
        let mut wire = opaque.payload.as_ref().to_vec();
        wire[0] ^= 0x01;

        let inbound = InboundOpaqueMessage::new(opaque.typ, opaque.version, &mut wire);
        assert!(decrypter.decrypt(inbound, 0).is_err(), "a tampered CCM ciphertext must not decrypt");
    }

    #[test]
    fn short_tamper_is_rejected() {
        let (mut encrypter, mut decrypter) = short_pair([0x77u8; 16], [0x88u8; 12]);
        let opaque = encrypter.encrypt(plain(&[b"authenticate me too"]), 0).expect("encrypt");
        let mut wire = opaque.payload.as_ref().to_vec();
        // Flip a byte inside the (truncated) tag itself, not just the
        // ciphertext — CCM_8's whole risk profile is that an 8-byte tag
        // gives an attacker a much better guessing chance, so this is the
        // arm worth checking specifically rather than trusting the
        // ciphertext-tamper case (already covered by `full_tamper_is_rejected`
        // and by `short_round_trips` for the equivalent short-suite case) to
        // stand in for it.
        let last = wire.len() - 1;
        wire[last] ^= 0x01;

        let inbound = InboundOpaqueMessage::new(opaque.typ, opaque.version, &mut wire);
        assert!(decrypter.decrypt(inbound, 0).is_err(), "a tampered CCM_8 tag must not verify");
    }

    /// Reconstructs the same ciphertext by calling `ccm::Ccm` directly — the
    /// nonce/AAD helpers are shared with `aead.rs`'s already-shipping suites,
    /// so what is actually new here is the CCM construction itself, which is
    /// exactly what this isolates.
    #[test]
    fn full_matches_the_underlying_crate() {
        let key = [0x99u8; 16];
        let iv_bytes = [0xAAu8; 12];
        let seq = 5u64;
        let plaintext: &[u8] = b"cross-check payload for CCM, longer than one AES block";

        let (mut encrypter, _) = full_pair(key, iv_bytes);
        let opaque = encrypter.encrypt(plain(&[plaintext]), seq).expect("encrypt");
        let wired: &[u8] = opaque.payload.as_ref();

        let cipher = Aes128Ccm::new_from_slice(&key).expect("16-byte key");
        let nonce = Nonce::new(&Iv::new(iv_bytes), seq);
        let mut buffer = plaintext.to_vec();
        buffer.push(ContentType::ApplicationData.into());
        let aad = make_tls13_aad(buffer.len() + TAG_LEN_FULL);
        let tag = cipher
            .encrypt_in_place_detached(nonce.0.as_ref().into(), &aad, &mut buffer)
            .expect("encrypt");
        buffer.extend_from_slice(&tag);

        assert_eq!(wired, buffer.as_slice());
    }

    /// Same cross-check as [`full_matches_the_underlying_crate`], for the
    /// `_8` variant — the gap named in the module doc that the first version
    /// of this file actually had.
    #[test]
    fn short_matches_the_underlying_crate() {
        let key = [0xDDu8; 16];
        let iv_bytes = [0xEEu8; 12];
        let seq = 11u64;
        let plaintext: &[u8] = b"cross-check payload for CCM_8, longer than one AES block";

        let (mut encrypter, _) = short_pair(key, iv_bytes);
        let opaque = encrypter.encrypt(plain(&[plaintext]), seq).expect("encrypt");
        let wired: &[u8] = opaque.payload.as_ref();

        let cipher = Aes128Ccm8::new_from_slice(&key).expect("16-byte key");
        let nonce = Nonce::new(&Iv::new(iv_bytes), seq);
        let mut buffer = plaintext.to_vec();
        buffer.push(ContentType::ApplicationData.into());
        let aad = make_tls13_aad(buffer.len() + TAG_LEN_8);
        let tag = cipher
            .encrypt_in_place_detached(nonce.0.as_ref().into(), &aad, &mut buffer)
            .expect("encrypt");
        buffer.extend_from_slice(&tag);

        assert_eq!(wired, buffer.as_slice());
    }

    /// Pins the fact the module doc's `calc_mac`/flags-byte paragraph
    /// explains: same ciphertext, but a genuinely different (not merely
    /// truncated) tag between the two suites — this is the test that first
    /// found that an earlier version of this file assumed wrongly.
    #[test]
    fn ciphertext_matches_across_tag_lengths_but_tag_does_not() {
        let key = [0xBBu8; 16];
        let iv = [0xCCu8; 12];
        let seq = 42u64;
        let plaintext: &[u8] = b"same inputs, two tag lengths";

        let (mut full_encrypter, _) = full_pair(key, iv);
        let full_wire = full_encrypter.encrypt(plain(&[plaintext]), seq).expect("full encrypt");
        let (mut short_encrypter, _) = short_pair(key, iv);
        let short_wire = short_encrypter.encrypt(plain(&[plaintext]), seq).expect("short encrypt");

        let full_bytes: &[u8] = full_wire.payload.as_ref();
        let short_bytes: &[u8] = short_wire.payload.as_ref();
        let ciphertext_len = plaintext.len() + 1; // +1 for the ContentType byte both suites append
        assert_eq!(
            &full_bytes[..ciphertext_len],
            &short_bytes[..ciphertext_len],
            "ciphertext must not depend on tag length — the CTR keystream (`extend_nonce`) never reads it"
        );
        assert_ne!(
            &full_bytes[ciphertext_len..][..TAG_LEN_8],
            &short_bytes[ciphertext_len..],
            "CCM_8's tag is a DIFFERENT MAC computation from CCM's, not a truncation of it — \
             the tag-length field is folded into the first CBC-MAC block, so equality here \
             would mean this file's two suites had somehow collapsed onto one MAC chain"
        );
    }
}
