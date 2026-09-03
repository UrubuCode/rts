//! A pure-Rust [`rustls::crypto::CryptoProvider`], assembled from the
//! RustCrypto crates `node:crypto` already vets — the owner decision in
//! `docs/reference/node/crates.md` §6, option (b): build our own rather than
//! adopt `rustls-rustcrypto` (0.0.2-alpha, "not for production"). No `ring`,
//! no `aws-lc-rs`, no C anywhere in this module.
//!
//! # Reuse-check
//!
//! `.claude/skills/reuse-check/SKILL.md`'s search found nothing to call
//! instead: `rts-cranelift` has no TLS/crypto-provider concept (its table is
//! value encodings, shapes, ABI), and nothing in `rts-core`/
//! `rts-node` builds a `CryptoProvider` today — `crypto/` hashes and
//! HMACs directly against `sha2`/`hmac`, with no rustls dependency at all.
//! This is the first thing in the crate that needs one.
//!
//! # Coverage — read before assuming parity with real Node
//!
//! | | covered | not covered (see "Not implemented, by name" below) |
//! |---|---|---|
//! | protocol | TLS 1.3 only | TLS 1.2 (even ECDHE) |
//! | AEAD | AES-128-GCM, AES-256-GCM, ChaCha20-Poly1305, AES-128-CCM, AES-128-CCM_8 | — (all five RFC 8446 §B.4 suites) |
//! | key exchange | X25519 (preferred), P-256 (ECDHE fallback) | P-384 |
//! | verify (peer certs, handshake sigs) | ECDSA P-256/P-384, Ed25519, RSA PKCS#1v1.5 | RSA-PSS, Ed448 |
//! | sign (our own `SecureContext` key) | ECDSA P-256, Ed25519 | RSA |
//!
//! This is the "modern suite set" the task names, cut down further by what a
//! single change can carry — every cut is named here and in the file that
//! makes it, not silently absent. "Exhaustive cipher-suite parity and
//! constant-time review" stay explicitly end-of-plan (crates.md §6).
//!
//! ## Not implemented, by name
//! - **TLS 1.2** — needs a second `Hkdf`-shaped thing (a PRF, not HKDF) and a
//!   second `KeyBlockShape`/`Tls12AeadAlgorithm` wiring; the task's own
//!   ECDHE-only ask still needs that machinery built once, which this change
//!   does not reach.
//! - **P-384 key exchange** — no `SupportedKxGroup` impl written for it;
//!   mechanically the same shape as the P-256 one `kx.rs` has.
//! - **RSA-PSS verification, Ed448, P-384 verification** — no
//!   `SignatureVerificationAlgorithm` impl written for them (`verify.rs`).
//! - **RSA signing** (our own `SecureContext` serving an RSA-keyed cert) —
//!   `sign.rs`'s `KeyProvider` only recognizes ECDSA-P256/Ed25519 PKCS#8.
//!   RSA is still *verified* (a client can reach an RSA-keyed real server);
//!   only serving from one does not work.
//! - **PKCS#8 keys carrying attributes, explicit curve parameters, or
//!   encryption** — `keyparse.rs`'s byte-offset reader is not an ASN.1
//!   parser; see its own doc for exactly what shape it reads.
//!
//! All five RFC 8446 §B.4 TLS 1.3 AEAD suites are wired now: `aead.rs` has
//! the three GCM/ChaCha20-Poly1305 ones, `ccm.rs` the two CCM ones — see that
//! file's module doc for why CCM needed its own crate and its own
//! `Tls13AeadAlgorithm` impl rather than a copy of GCM's.

mod aead;
mod ccm;
mod hash;
mod keyparse;
mod kx;
mod random;
mod sign;
mod verify;

use std::sync::Arc;

use rustls::crypto::tls13::HkdfUsingHmac;
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::pki_types::SignatureVerificationAlgorithm;
use rustls::{CipherSuite, CipherSuiteCommon, SignatureScheme, SupportedCipherSuite, Tls13CipherSuite};

static AES_128_GCM_SHA256: Tls13CipherSuite = Tls13CipherSuite {
    common: CipherSuiteCommon {
        suite: CipherSuite::TLS13_AES_128_GCM_SHA256,
        hash_provider: &hash::SHA256,
        confidentiality_limit: 1 << 24,
    },
    hkdf_provider: &HkdfUsingHmac(&hash::HMAC_SHA256),
    aead_alg: &aead::Aes128GcmTls13,
    quic: None,
};

// `confidentiality_limit: 1 << 24` — copied from rustls's own `ring`
// provider (`crypto/ring/tls13.rs`), the same limit AES-128-GCM above uses:
// GCM's per-key safety bound does not move with key length, only with block
// size, which AES-256 shares with AES-128.
static AES_256_GCM_SHA384: Tls13CipherSuite = Tls13CipherSuite {
    common: CipherSuiteCommon {
        suite: CipherSuite::TLS13_AES_256_GCM_SHA384,
        hash_provider: &hash::SHA384,
        confidentiality_limit: 1 << 24,
    },
    hkdf_provider: &HkdfUsingHmac(&hash::HMAC_SHA384),
    aead_alg: &aead::Aes256GcmTls13,
    quic: None,
};

static CHACHA20_POLY1305_SHA256: Tls13CipherSuite = Tls13CipherSuite {
    common: CipherSuiteCommon {
        suite: CipherSuite::TLS13_CHACHA20_POLY1305_SHA256,
        hash_provider: &hash::SHA256,
        confidentiality_limit: u64::MAX,
    },
    hkdf_provider: &HkdfUsingHmac(&hash::HMAC_SHA256),
    aead_alg: &aead::ChaCha20Poly1305Tls13,
    quic: None,
};

// `confidentiality_limit` reasoning: `ccm.rs`'s own module doc — there is no
// `ring`-provider line to copy for CCM the way GCM's `1 << 24` above was, so
// this is a deliberately conservative reading of a different document than
// RFC 8446 itself, spelled out there rather than repeated here.
static AES_128_CCM_SHA256: Tls13CipherSuite = Tls13CipherSuite {
    common: CipherSuiteCommon {
        suite: CipherSuite::TLS13_AES_128_CCM_SHA256,
        hash_provider: &hash::SHA256,
        confidentiality_limit: 1 << 24,
    },
    hkdf_provider: &HkdfUsingHmac(&hash::HMAC_SHA256),
    aead_alg: &ccm::Aes128CcmTls13,
    quic: None,
};

static AES_128_CCM_8_SHA256: Tls13CipherSuite = Tls13CipherSuite {
    common: CipherSuiteCommon {
        suite: CipherSuite::TLS13_AES_128_CCM_8_SHA256,
        hash_provider: &hash::SHA256,
        confidentiality_limit: 1 << 10,
    },
    hkdf_provider: &HkdfUsingHmac(&hash::HMAC_SHA256),
    aead_alg: &ccm::Aes128Ccm8Tls13,
    quic: None,
};

static VERIFY_ECDSA_P256: verify::EcdsaP256Sha256 = verify::EcdsaP256Sha256;
static VERIFY_ECDSA_P384: verify::EcdsaP384Sha384 = verify::EcdsaP384Sha384;
static VERIFY_ED25519: verify::Ed25519 = verify::Ed25519;
static VERIFY_RSA_PKCS1: verify::RsaPkcs1Sha256 = verify::RsaPkcs1Sha256;

static ALL_VERIFY_ALGS: &[&dyn SignatureVerificationAlgorithm] = &[&VERIFY_ECDSA_P256, &VERIFY_ECDSA_P384, &VERIFY_ED25519, &VERIFY_RSA_PKCS1];

static SCHEME_MAPPING: &[(SignatureScheme, &[&dyn SignatureVerificationAlgorithm])] = &[
    (SignatureScheme::ECDSA_NISTP256_SHA256, &[&VERIFY_ECDSA_P256]),
    (SignatureScheme::ECDSA_NISTP384_SHA384, &[&VERIFY_ECDSA_P384]),
    (SignatureScheme::ED25519, &[&VERIFY_ED25519]),
    (SignatureScheme::RSA_PKCS1_SHA256, &[&VERIFY_RSA_PKCS1]),
];

static X25519: kx::X25519 = kx::X25519;
static SECP256R1: kx::Secp256r1 = kx::Secp256r1;
static RANDOM: random::Random = random::Random;
static KEY_PROVIDER: sign::KeyProvider = sign::KeyProvider;

/// Builds this provider — one call per `SecureContext`/connection config
/// build, per rustls's own `ConfigBuilder::with_provider` shape; cheap,
/// since everything above is `'static`.
pub(crate) fn provider() -> Arc<CryptoProvider> {
    Arc::new(CryptoProvider {
        cipher_suites: vec![
            SupportedCipherSuite::Tls13(&AES_128_GCM_SHA256),
            SupportedCipherSuite::Tls13(&AES_256_GCM_SHA384),
            SupportedCipherSuite::Tls13(&CHACHA20_POLY1305_SHA256),
            SupportedCipherSuite::Tls13(&AES_128_CCM_SHA256),
            SupportedCipherSuite::Tls13(&AES_128_CCM_8_SHA256),
        ],
        kx_groups: vec![&X25519, &SECP256R1],
        signature_verification_algorithms: WebPkiSupportedAlgorithms {
            all: ALL_VERIFY_ALGS,
            mapping: SCHEME_MAPPING,
        },
        secure_random: &RANDOM,
        key_provider: &KEY_PROVIDER,
    })
}

/// The cipher-suite names [`crate::tls`]'s `getCiphers()` answers — lower
/// case, matching Node's own `tls.getCiphers()` casing convention.
pub(crate) fn cipher_names() -> &'static [&'static str] {
    &[
        "tls_aes_128_gcm_sha256",
        "tls_aes_256_gcm_sha384",
        "tls_chacha20_poly1305_sha256",
        "tls_aes_128_ccm_sha256",
        "tls_aes_128_ccm_8_sha256",
    ]
}
