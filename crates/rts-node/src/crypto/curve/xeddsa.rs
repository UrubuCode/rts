//! XEdDSA over Curve25519 — signing with an X25519 key, per Signal's
//! specification (<https://signal.org/docs/specifications/xeddsa/>, §2 and
//! §3.2).
//!
//! # Why this exists when `ed25519-dalek` is already linked
//!
//! They are not the same signature. Ed25519 has its own keypair, derived by
//! hashing a seed; XEdDSA signs with the Montgomery (X25519) private key a
//! program ALREADY has for key agreement, converting it to an Edwards keypair
//! on the fly. Signal's protocol has exactly one identity key per party and it
//! is an X25519 key, so `signedPreKey` and the account signature are XEdDSA —
//! an Ed25519 signature over the same bytes is a different signature, and the
//! server rejects it.
//!
//! That is why this file is arithmetic and not a call: `ed25519_dalek` exposes
//! `SigningKey::from_bytes`, which treats its input as an Ed25519 SEED and
//! hashes it. Handing an X25519 private key to it produces a valid signature
//! under the wrong key. The conversion has to happen at the point/scalar level,
//! which is `curve25519_dalek`.
//!
//! # The one place this is easy to get wrong
//!
//! The sign bit. `kB` lands on either of two Edwards points whose Montgomery
//! `u` is the same, so XEdDSA fixes the convention: the public key always has
//! sign bit 0, and the SCALAR is negated when the derived point had sign bit 1.
//! Skipping that negation still produces a signature — it simply never
//! verifies, half the time, against keys generated the other half. [`sign`]
//! does it in `key_pair`, and the round-trip tests below run enough keys to
//! reach both branches.

use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::montgomery::MontgomeryPoint;
use curve25519_dalek::scalar::Scalar;
use sha2::{Digest, Sha512};

/// Signal's `hash_1` domain separator: `2^b - 1 - i` for `b = 256`, `i = 1`,
/// little-endian. It exists so that the nonce hash can never collide with a
/// plain SHA-512 of the same message — every other hash in the scheme is
/// undomained, and a scalar that is both is a private key recovery.
const HASH1_PREFIX: [u8; 32] = [
    0xfe, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

/// The X25519 clamping every Curve25519 private key is used under. Applied here
/// rather than assumed of the input: `x25519_dalek::StaticSecret::to_bytes`
/// answers the bytes it was GIVEN, so a key that made a round trip through
/// JavaScript is not necessarily clamped, and an unclamped scalar signs under a
/// key that is not the one `x25519PublicKey` derives.
fn clamp(private: &[u8; 32]) -> Scalar {
    let mut bytes = *private;
    bytes[0] &= 248;
    bytes[31] &= 127;
    bytes[31] |= 64;
    Scalar::from_bytes_mod_order(bytes)
}

/// `calculate_key_pair` of the specification: the Edwards public key with sign
/// bit forced to 0, and the scalar that signs under it.
fn key_pair(k: &Scalar) -> (CompressedEdwardsY, Scalar) {
    let point = ED25519_BASEPOINT_TABLE * k;
    let mut encoded = point.compress().to_bytes();
    let negative = encoded[31] >> 7;
    encoded[31] &= 0x7f;
    let scalar = if negative == 1 { -k } else { *k };
    (CompressedEdwardsY(encoded), scalar)
}

fn wide_reduce(hash: Sha512) -> Scalar {
    let digest = hash.finalize();
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&digest);
    Scalar::from_bytes_mod_order_wide(&wide)
}

/// Sign `message` with the 32-byte X25519 private key `private`, answering the
/// 64-byte signature `R || s`.
///
/// `nonce` is the specification's `Z`: 64 bytes of randomness the caller
/// supplies. It is a parameter rather than drawn here for one reason — a test
/// can then FIX it, and a signature over a fixed `Z` is reproducible. XEdDSA is
/// randomized by design (`r` mixes `Z` in), so two signatures over one key and
/// one message differ, and nothing about a run could be compared to anything
/// otherwise. The JavaScript surface does not expose it, and `curve/mod.rs`
/// says why not.
pub(super) fn sign(private: &[u8; 32], message: &[u8], nonce: &[u8; 64]) -> [u8; 64] {
    let k = clamp(private);
    let (public, a) = key_pair(&k);

    let mut hash = Sha512::new();
    hash.update(HASH1_PREFIX);
    hash.update(a.as_bytes());
    hash.update(message);
    hash.update(nonce);
    let r = wide_reduce(hash);

    let big_r = (ED25519_BASEPOINT_TABLE * &r).compress();

    let mut hash = Sha512::new();
    hash.update(big_r.as_bytes());
    hash.update(public.as_bytes());
    hash.update(message);
    let h = wide_reduce(hash);

    let s = r + h * a;

    let mut signature = [0u8; 64];
    signature[..32].copy_from_slice(big_r.as_bytes());
    signature[32..].copy_from_slice(s.as_bytes());
    signature
}

/// Verify a 64-byte XEdDSA signature against the 32-byte X25519 PUBLIC key
/// `public` (a Montgomery `u`, which is what the wire carries).
///
/// Every failure answers `false` — a malformed point, a non-canonical scalar, a
/// wrong signature are one outcome to a caller, and distinguishing them in the
/// return would let a caller branch on which part of an attacker's input was
/// malformed.
pub(super) fn verify(public: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    // The specification's own bounds check: the top bit of `s` must be clear.
    // `from_canonical_bytes` rejects more than that, and rejecting early keeps
    // the two checks from looking like one.
    if signature[63] & 0x80 != 0 {
        return false;
    }
    let Some(point) = MontgomeryPoint(*public).to_edwards(0) else {
        return false;
    };
    let encoded_a = point.compress();

    let mut big_r = [0u8; 32];
    big_r.copy_from_slice(&signature[..32]);
    let big_r = CompressedEdwardsY(big_r);

    let mut s_bytes = [0u8; 32];
    s_bytes.copy_from_slice(&signature[32..]);
    let s: Option<Scalar> = Scalar::from_canonical_bytes(s_bytes).into();
    let Some(s) = s else {
        return false;
    };

    let mut hash = Sha512::new();
    hash.update(big_r.as_bytes());
    hash.update(encoded_a.as_bytes());
    hash.update(message);
    let h = wide_reduce(hash);

    // sB - hA, which reconstructs R when the signature is genuine.
    let recovered = EdwardsPoint::vartime_double_scalar_mul_basepoint(&(-h), &point, &s);
    recovered.compress() == big_r
}

/// The Montgomery public key `x25519PublicKey` would derive, computed the way
/// [`sign`] derives it internally.
///
/// Its only caller is a test, and it is here rather than there because the
/// property it pins is about THIS file: the Edwards key XEdDSA signs under and
/// the Montgomery key X25519 agrees under are the same key seen two ways. A
/// helper in the test module could drift from `key_pair` without anything
/// noticing.
#[cfg(test)]
fn montgomery_public(private: &[u8; 32]) -> [u8; 32] {
    (ED25519_BASEPOINT_TABLE * &clamp(private)).to_montgomery().to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The claim this file makes, checked by something that is not this file.
    ///
    /// An XEdDSA signature IS an Ed25519 signature — over the Edwards public key
    /// the Montgomery key converts to, with sign bit 0. So `ed25519-dalek`,
    /// already linked for `node:tls` and written by neither this repository nor
    /// against this convention, must accept what [`sign`] produces. That is a
    /// real cross-check where a self-generated vector is only a record of what
    /// this code did once: if the sign-bit negation in `key_pair` were dropped,
    /// every test above would still pass for the keys that happen to land on the
    /// positive branch, and THIS one fails for the other half.
    ///
    /// `verify_strict` and not `verify`: it rejects small-order public keys and
    /// pins the cofactorless equation, which is the stricter of the two readings
    /// and the one Signal's verifier uses.
    #[test]
    fn ed25519_dalek_accepts_what_this_signs() {
        for seed in 0u8..16 {
            let private = [seed.wrapping_mul(53).wrapping_add(11); 32];
            let edwards = MontgomeryPoint(montgomery_public(&private))
                .to_edwards(0)
                .expect("a public key derived on this curve converts back");
            let verifying = ed25519_dalek::VerifyingKey::from_bytes(&edwards.compress().to_bytes())
                .expect("the converted point is a valid Ed25519 public key");
            let signature = sign(&private, b"cross-checked", &[seed ^ 0x5a; 64]);
            verifying
                .verify_strict(b"cross-checked", &ed25519_dalek::Signature::from_bytes(&signature))
                .unwrap_or_else(|error| panic!("seed {seed}: {error}"));
        }
    }

    /// The same signature under a message ed25519-dalek was not given must fail
    /// there too — without this, the test above would pass against a verifier
    /// that accepted everything.
    #[test]
    fn ed25519_dalek_rejects_a_different_message() {
        let private = [0x21u8; 32];
        let edwards = MontgomeryPoint(montgomery_public(&private)).to_edwards(0).unwrap();
        let verifying =
            ed25519_dalek::VerifyingKey::from_bytes(&edwards.compress().to_bytes()).unwrap();
        let signature = sign(&private, b"signed", &[0x33u8; 64]);
        assert!(
            verifying
                .verify_strict(b"not signed", &ed25519_dalek::Signature::from_bytes(&signature))
                .is_err()
        );
    }


    /// Both branches of the sign-bit convention, which is the failure this
    /// scheme hides best: sixteen distinct keys is far past the point where all
    /// sixteen land on one branch by chance.
    #[test]
    fn signatures_verify_across_many_keys() {
        for seed in 0u8..16 {
            let private = [seed.wrapping_mul(37).wrapping_add(3); 32];
            let public = montgomery_public(&private);
            let signature = sign(&private, b"message under test", &[seed; 64]);
            assert!(verify(&public, b"message under test", &signature), "seed {seed}");
        }
    }

    #[test]
    fn a_changed_message_does_not_verify() {
        let private = [0x42u8; 32];
        let public = montgomery_public(&private);
        let signature = sign(&private, b"original", &[0x99u8; 64]);
        assert!(verify(&public, b"original", &signature));
        assert!(!verify(&public, b"modified", &signature));
    }

    #[test]
    fn a_changed_signature_does_not_verify() {
        let private = [0x42u8; 32];
        let public = montgomery_public(&private);
        let mut signature = sign(&private, b"payload", &[0x5au8; 64]);
        signature[0] ^= 1;
        assert!(!verify(&public, b"payload", &signature));
    }

    #[test]
    fn another_key_does_not_verify() {
        let signature = sign(&[0x01u8; 32], b"payload", &[0x02u8; 64]);
        assert!(!verify(&montgomery_public(&[0x03u8; 32]), b"payload", &signature));
    }

    /// An unclamped private key must sign under the same public key a clamped
    /// one does — this is what [`clamp`]'s doc says the risk is, stated as a
    /// test rather than as a comment alone.
    #[test]
    fn clamping_is_applied_to_the_input() {
        let mut unclamped = [0x42u8; 32];
        unclamped[0] |= 7;
        unclamped[31] |= 0x80;
        let public = montgomery_public(&[0x42u8; 32]);
        let signature = sign(&unclamped, b"payload", &[0x07u8; 64]);
        assert!(verify(&public, b"payload", &signature));
    }

    /// A scalar `s` with the top bit set is refused before any point
    /// arithmetic happens.
    #[test]
    fn a_non_canonical_scalar_is_refused() {
        let private = [0x42u8; 32];
        let public = montgomery_public(&private);
        let mut signature = sign(&private, b"payload", &[0x11u8; 64]);
        signature[63] |= 0x80;
        assert!(!verify(&public, b"payload", &signature));
    }
}
