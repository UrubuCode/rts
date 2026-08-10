//! `pbkdf2Sync`, `scryptSync`, `hkdfSync` — the glue, and nothing else.
//!
//! Each function here reads arguments through [`super::args`], calls one
//! function in [`super::derive`], and answers a `Buffer` or raises. It performs
//! no arithmetic of its own on purpose: everything an RFC has an opinion about
//! lives in `derive.rs`, where a `#[test]` can reach it without a runtime.
//!
//! # Raising, not answering zeros
//!
//! Every failure path raises through `entry::throw_type_error` (rule 8 of
//! `rts-core`'s README; `throw_type_error`'s own doc says why the class is
//! `TypeError` where Node raises `ERR_CRYPTO_INVALID_SCRYPT_PARAMS` or
//! `ERR_OUT_OF_RANGE` — a host crate cannot name another). The version this
//! replaces answered a zero-filled `Buffer` for an unknown digest, for a
//! `keylen` HKDF cannot expand to, and for scrypt parameters the crate refused.
//! That is the failure mode `NODE_COMPATIBLE.md` ranks eighth and calls "worse
//! than returning none": it ran, it returned a `Buffer`, and nothing said the
//! key was not a key.

use rts_core::entry;

use super::args::{HkdfArguments, Pbkdf2Arguments, ScryptArguments};
use super::derive;

/// A derivation's answer as a JavaScript value: a `Buffer`, or a raise and
/// `undefined`.
///
/// One function so the three entry points cannot disagree about what a failure
/// looks like. The `Buffer` is minted inside a fresh borrow, AFTER the raise
/// decision, because `throw_type_error` takes the borrow itself.
pub(super) fn answer(derived: Result<Vec<u8>, String>) -> u64 {
    match derived {
        Ok(bytes) => entry::with_runtime(|context| entry::make_buffer(context, &bytes)),
        Err(message) => {
            entry::throw_type_error(&message);
            entry::undefined_value()
        }
    }
}

/// The digest name, or the refusal Node raises when it is missing.
///
/// Node's `pbkdf2`/`pbkdf2Sync` require an explicit `digest` (`crypto.md` §4).
/// Defaulting is what the previous version did — to SHA-256, quietly — and it
/// is the same class of mistake as the SHA-1 default Node itself removed.
pub(super) fn required_digest(name: Option<String>) -> Result<String, String> {
    name.ok_or_else(|| {
        "The \"digest\" argument must be of type string. Received undefined".to_owned()
    })
}

/// `crypto.pbkdf2Sync(password, salt, iterations, keylen, digest)`.
pub(super) extern "C" fn pbkdf2_sync(
    _e: u64,
    _this: u64,
    password: u64,
    salt: u64,
    iterations: u64,
    keylen: u64,
) -> u64 {
    let read = Pbkdf2Arguments::read(password, salt, iterations, keylen);
    answer(derive_pbkdf2(&read))
}

/// Shared by [`pbkdf2_sync`] and its callback form, so the two cannot derive
/// different bytes from the same arguments.
pub(super) fn derive_pbkdf2(read: &Pbkdf2Arguments) -> Result<Vec<u8>, String> {
    let digest = required_digest(read.digest.clone())?;
    derive::pbkdf2_bytes(&digest, &read.password, &read.salt, read.rounds, read.keylen)
}

/// `crypto.scryptSync(password, salt, keylen[, options])`.
pub(super) extern "C" fn scrypt_sync(
    _e: u64,
    _this: u64,
    password: u64,
    salt: u64,
    keylen: u64,
    options: u64,
) -> u64 {
    let read = ScryptArguments::read(password, salt, keylen, options);
    answer(derive_scrypt(&read))
}

/// Shared by [`scrypt_sync`] and its callback form.
pub(super) fn derive_scrypt(read: &ScryptArguments) -> Result<Vec<u8>, String> {
    derive::scrypt_bytes(
        &read.password,
        &read.salt,
        read.options.n,
        read.options.r,
        read.options.p,
        read.options.maxmem,
        read.keylen,
    )
}

/// `crypto.hkdfSync(digest, ikm, salt, info, keylen)`.
///
/// # A `Buffer` where Node answers an `ArrayBuffer`
///
/// `crypto.md` §3 types this `=> ArrayBuffer`, and this answers a `Buffer` —
/// the same divergence `digest()` and `randomBytes` already state in
/// `super::super`'s module doc, for the same reason: `entry::make_buffer` is the
/// only raw-bytes constructor a host crate can reach, and there is no
/// `make_array_buffer` beside it. The bytes are identical and
/// `new Uint8Array(hkdfSync(…))` reads the same values off either, which is the
/// shape almost every caller uses. `instanceof ArrayBuffer` is false, and that
/// is the cost.
pub(super) extern "C" fn hkdf_sync(
    _e: u64,
    _this: u64,
    digest: u64,
    ikm: u64,
    salt: u64,
    info: u64,
) -> u64 {
    let read = HkdfArguments::read(digest, ikm, salt, info);
    answer(derive_hkdf(&read))
}

/// Shared by [`hkdf_sync`] and its callback form.
pub(super) fn derive_hkdf(read: &HkdfArguments) -> Result<Vec<u8>, String> {
    let digest = required_digest(read.digest.clone())?;
    derive::hkdf_bytes(&digest, &read.ikm, &read.salt, &read.info, read.keylen)
}
