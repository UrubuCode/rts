//! Certificate-chain and handshake signature verification, over `p256`
//! (ECDSA P-256), `ed25519-dalek` and `rsa` — the same three `crates.md` §6
//! names for TLS signatures, and the same crates `node:crypto` would use for
//! its own (not yet built) asymmetric surface.
//!
//! `webpki`'s own certificate-chain walk (pulled in unconditionally by
//! `rustls` — not a new dependency; see `mod.rs`) calls
//! [`rustls::pki_types::SignatureVerificationAlgorithm::verify_signature`]
//! with the raw `subjectPublicKey` bytes and does the ASN.1 chain-walking
//! itself, so none of this needs an X.509 parser (`x509-parser`, which
//! `crypto/mod.rs` already declined to add) — only the three key types'
//! own SPKI decoders, which `p256`/`ed25519-dalek`/`rsa` carry already.
//!
//! RSA verification carries RUSTSEC-2023-0071 (Marvin, a timing side
//! channel) on RSA **decryption**; this is verification, which the advisory
//! does not cover, but the module doc names the crate-wide caveat once
//! rather than nowhere.
//!
//! RSA-PSS verification is not implemented — see `mod.rs`'s "Not implemented,
//! by name". P-384 É verificado ([`EcdsaP384Sha384`]).

use p256::ecdsa::signature::Verifier;
use rsa::pkcs1::DecodeRsaPublicKey;
use rustls::pki_types::{AlgorithmIdentifier, InvalidSignature, SignatureVerificationAlgorithm, alg_id};

/// The RFC 8017 §9.2 `DigestInfo` prefix for SHA-256, ahead of the 32-byte
/// digest itself — see [`RsaPkcs1Sha256`]'s own doc for why this is spelled
/// out by hand rather than built from a `Digest + AssociatedOid` type: `rsa`'s
/// own `sha2` re-export (which supplies that combination) sits behind a
/// `sha2` Cargo feature this crate's `rsa` dependency does not enable, and
/// this crate's OWN direct `sha2 = "0.11"` (`node:crypto`'s) is a different
/// major version of the `Digest` trait than the `digest 0.10` `rsa`/`ecdsa`/
/// `p256` are pinned to — so the two can't be mixed. `Pkcs1v15Sign`'s public
/// fields (`hash_len`, `prefix`) exist precisely to be filled by hand.
const SHA256_DIGESTINFO_PREFIX: &[u8] = &[
    0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20,
];

#[derive(Debug)]
pub(crate) struct EcdsaP256Sha256;

impl SignatureVerificationAlgorithm for EcdsaP256Sha256 {
    fn verify_signature(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<(), InvalidSignature> {
        let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(public_key).map_err(|_| InvalidSignature)?;
        let sig = p256::ecdsa::Signature::from_der(signature).map_err(|_| InvalidSignature)?;
        key.verify(message, &sig).map_err(|_| InvalidSignature)
    }

    fn public_key_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::ECDSA_P256
    }

    fn signature_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::ECDSA_SHA256
    }
}

/// ECDSA sobre a curva P-384 com SHA-384.
///
/// Não é exotismo: é o que uma boa parte da web serve hoje, e sem isto o
/// handshake morria com `UnsupportedSignatureAlgorithmContext` no certificado
/// — a `pt.wikipedia.org` é um exemplo. O `p384` já era dependência deste
/// crate (§6, para ECDH), por isso não entra crate nova para o suportar.
#[derive(Debug)]
pub(crate) struct EcdsaP384Sha384;

impl SignatureVerificationAlgorithm for EcdsaP384Sha384 {
    fn verify_signature(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<(), InvalidSignature> {
        use p384::ecdsa::signature::Verifier as _;
        let key = p384::ecdsa::VerifyingKey::from_sec1_bytes(public_key).map_err(|_| InvalidSignature)?;
        let sig = p384::ecdsa::Signature::from_der(signature).map_err(|_| InvalidSignature)?;
        key.verify(message, &sig).map_err(|_| InvalidSignature)
    }

    fn public_key_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::ECDSA_P384
    }

    fn signature_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::ECDSA_SHA384
    }
}

#[derive(Debug)]
pub(crate) struct Ed25519;

impl SignatureVerificationAlgorithm for Ed25519 {
    fn verify_signature(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<(), InvalidSignature> {
        let key_bytes: [u8; 32] = public_key.try_into().map_err(|_| InvalidSignature)?;
        let sig_bytes: [u8; 64] = signature.try_into().map_err(|_| InvalidSignature)?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&key_bytes).map_err(|_| InvalidSignature)?;
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
        key.verify_strict(message, &sig).map_err(|_| InvalidSignature)
    }

    fn public_key_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::ED25519
    }

    fn signature_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::ED25519
    }
}

/// See this module's own doc for the Marvin-timing caveat this reuses `rsa`
/// for anyway (decryption, not verification, is what the advisory covers).
#[derive(Debug)]
pub(crate) struct RsaPkcs1Sha256;

impl SignatureVerificationAlgorithm for RsaPkcs1Sha256 {
    fn verify_signature(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<(), InvalidSignature> {
        let key = rsa::RsaPublicKey::from_pkcs1_der(public_key).map_err(|_| InvalidSignature)?;
        // This crate's own `sha2` (0.11) — see `SHA256_DIGESTINFO_PREFIX`'s
        // doc for why `rsa`'s `Digest`-generic constructor isn't used here.
        let hashed = <sha2::Sha256 as sha2::Digest>::digest(message);
        let scheme = rsa::pkcs1v15::Pkcs1v15Sign {
            hash_len: Some(32),
            prefix: SHA256_DIGESTINFO_PREFIX.to_vec().into_boxed_slice(),
        };
        key.verify(scheme, &hashed, signature).map_err(|_| InvalidSignature)
    }

    fn public_key_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::RSA_ENCRYPTION
    }

    fn signature_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::RSA_PKCS1_SHA256
    }
}
