//! The hash algorithms [`super::hash`]/[`super::hmac`] both dispatch over.
//!
//! One enum rather than a trait object: `digest::Digest` is not object-safe
//! the way this crate would need it (`finalize` consumes `Self` by value),
//! and every algorithm in scope is known at compile time — a `match` over a
//! name, once, at construction, is the whole cost. `getHashes()` reads
//! [`NAMES`] rather than a second list, so the two never drift apart.

use blake2::Digest as _;

/// One streaming digest, mid-update or ready to finalize.
///
/// # Not implemented, by name
///
/// SHAKE128/256 (extendable-output, no fixed digest length — this enum's
/// `finalize` answers a fixed-length `Vec<u8>`, which is the wrong shape for
/// an XOF) and Keccak (Node's own un-padded variant, distinct from the
/// NIST SHA3 family below). Each is a `match` arm away once scoped — the gap
/// is the arm, not the mechanism.
#[derive(Clone)]
pub(super) enum HashState {
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
    Sha384(sha2::Sha384),
    Sha224(sha2::Sha224),
    Sha512_256(sha2::Sha512_256),
    Sha512_224(sha2::Sha512_224),
    Sha1(sha1::Sha1),
    Md5(md5::Md5),
    Sha3_256(sha3::Sha3_256),
    Sha3_224(sha3::Sha3_224),
    Sha3_384(sha3::Sha3_384),
    Sha3_512(sha3::Sha3_512),
    Blake2b512(blake2::Blake2b512),
    Blake2s256(blake2::Blake2s256),
    Ripemd160(ripemd::Ripemd160),
}

/// Every algorithm name [`HashState::new`] recognizes — also `getHashes()`'s
/// answer, so a name accepted here is a name that string always contained.
pub(super) const NAMES: &[&str] = &[
    "sha256",
    "sha512",
    "sha1",
    "md5",
    "sha384",
    "sha224",
    "sha512-256",
    "sha512-224",
    "sha3-256",
    "sha3-224",
    "sha3-384",
    "sha3-512",
    "blake2b512",
    "blake2s256",
    "ripemd160",
];

impl HashState {
    /// A fresh state for a named algorithm, case-insensitive (Node accepts
    /// both `'SHA256'` and `'sha256'`). `None` for a name not in [`NAMES`].
    pub(super) fn new(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "sha256" => Some(Self::Sha256(sha2::Sha256::new())),
            "sha512" => Some(Self::Sha512(sha2::Sha512::new())),
            "sha384" => Some(Self::Sha384(sha2::Sha384::new())),
            "sha224" => Some(Self::Sha224(sha2::Sha224::new())),
            "sha512-256" => Some(Self::Sha512_256(sha2::Sha512_256::new())),
            "sha512-224" => Some(Self::Sha512_224(sha2::Sha512_224::new())),
            "sha1" => Some(Self::Sha1(sha1::Sha1::new())),
            "md5" => Some(Self::Md5(md5::Md5::new())),
            "sha3-256" => Some(Self::Sha3_256(sha3::Sha3_256::new())),
            "sha3-224" => Some(Self::Sha3_224(sha3::Sha3_224::new())),
            "sha3-384" => Some(Self::Sha3_384(sha3::Sha3_384::new())),
            "sha3-512" => Some(Self::Sha3_512(sha3::Sha3_512::new())),
            "blake2b512" => Some(Self::Blake2b512(blake2::Blake2b512::new())),
            "blake2s256" => Some(Self::Blake2s256(blake2::Blake2s256::new())),
            "ripemd160" => Some(Self::Ripemd160(ripemd::Ripemd160::new())),
            _ => None,
        }
    }

    /// Feeds more bytes in. Callable any number of times before
    /// [`Self::finalize`].
    pub(super) fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha256(state) => state.update(data),
            Self::Sha512(state) => state.update(data),
            Self::Sha384(state) => state.update(data),
            Self::Sha224(state) => state.update(data),
            Self::Sha512_256(state) => state.update(data),
            Self::Sha512_224(state) => state.update(data),
            Self::Sha1(state) => state.update(data),
            Self::Md5(state) => state.update(data),
            Self::Sha3_256(state) => state.update(data),
            Self::Sha3_224(state) => state.update(data),
            Self::Sha3_384(state) => state.update(data),
            Self::Sha3_512(state) => state.update(data),
            Self::Blake2b512(state) => state.update(data),
            Self::Blake2s256(state) => state.update(data),
            Self::Ripemd160(state) => state.update(data),
        }
    }

    /// Consumes the state and answers the digest bytes — the same
    /// single-use shape Node's `Hash.digest()` has (a second call has
    /// nothing left to consume; see [`super::hash`] for how that is
    /// answered without a throw).
    pub(super) fn finalize(self) -> Vec<u8> {
        match self {
            Self::Sha256(state) => state.finalize().to_vec(),
            Self::Sha512(state) => state.finalize().to_vec(),
            Self::Sha384(state) => state.finalize().to_vec(),
            Self::Sha224(state) => state.finalize().to_vec(),
            Self::Sha512_256(state) => state.finalize().to_vec(),
            Self::Sha512_224(state) => state.finalize().to_vec(),
            Self::Sha1(state) => state.finalize().to_vec(),
            Self::Md5(state) => state.finalize().to_vec(),
            Self::Sha3_256(state) => state.finalize().to_vec(),
            Self::Sha3_224(state) => state.finalize().to_vec(),
            Self::Sha3_384(state) => state.finalize().to_vec(),
            Self::Sha3_512(state) => state.finalize().to_vec(),
            Self::Blake2b512(state) => state.finalize().to_vec(),
            Self::Blake2s256(state) => state.finalize().to_vec(),
            Self::Ripemd160(state) => state.finalize().to_vec(),
        }
    }
}
