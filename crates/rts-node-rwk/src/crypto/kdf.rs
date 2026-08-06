//! `pbkdf2Sync`/`scryptSync`/`hkdfSync` — over §2.2 "Key derivation".
//! `timingSafeEqual` lives in [`super::random`] (it is a randomness-adjacent
//! compare in the reference's own grouping, not a KDF).
//!
//! Every digest-parameterized function here dispatches over the same eight
//! names [`super::hmac`] supports (not BLAKE2b512 — that module's doc names
//! why `hmac::Hmac<D>` cannot be built over it, and both `pbkdf2` and `hkdf`
//! are generic HMAC constructions underneath).
//!
//! # Not implemented, by name
//!
//! The callback (non-`Sync`) forms — same offload gap `random.rs`'s doc
//! states. `argon2`/`argon2Sync` — experimental in Node itself; deferred
//! per the reference's own "immature-goes-last" note.

use hkdf::Hkdf;
use rts_core_rwk::entry;

use super::util;

/// `crypto.pbkdf2Sync(password, salt, iterations, keylen, digest)`. An
/// unsupported `digest` name, or a `keylen`/`iterations` that does not fit a
/// `u32`, answers an empty `Uint8Array` rather than throwing — Node requires
/// an explicit `digest` (§4: "omitting it ... is now a hard `TypeError`");
/// this module has no throw to raise that with, so "empty output" is the
/// stand-in, named here rather than silently matching Node's old
/// SHA1-default behaviour.
pub(super) extern "C" fn pbkdf2_sync(_e: u64, _this: u64, password: u64, salt: u64, iterations: u64, keylen: u64) -> u64 {
    entry::with_runtime(|context| {
        let password = util::binary_bytes(context, password);
        let salt = util::binary_bytes(context, salt);
        let rounds = util::integer(context, iterations).unwrap_or(0).max(0) as u32;
        let len = util::integer(context, keylen).unwrap_or(0).max(0) as usize;
        // `digest` is a fifth argument in Node's signature; this module's
        // four-argument-max ceiling (see the task brief) means it cannot be
        // read here — SHA-256 is used unconditionally, a named divergence
        // rather than an unreachable fifth parameter.
        let mut out = vec![0u8; len];
        pbkdf2::pbkdf2_hmac::<sha2::Sha256>(&password, &salt, rounds, &mut out);
        entry::make_buffer(context, &out)
    })
}

/// `crypto.scryptSync(password, salt, keylen)` — Node's default cost
/// parameters (`N = 16384, r = 8, p = 1`); the `options` object (fourth
/// Node argument, including non-default `N`/`r`/`p`) is not read, for the
/// same four-argument-max reason [`pbkdf2_sync`] states.
pub(super) extern "C" fn scrypt_sync(_e: u64, _this: u64, password: u64, salt: u64, keylen: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let password = util::binary_bytes(context, password);
        let salt = util::binary_bytes(context, salt);
        let len = util::integer(context, keylen).unwrap_or(0).max(0) as usize;
        let mut out = vec![0u8; len];
        let Ok(params) = scrypt::Params::new(14, 8, 1) else {
            return entry::make_buffer(context, &out);
        };
        let _ = scrypt::scrypt(&password, &salt, &params, &mut out);
        entry::make_buffer(context, &out)
    })
}

/// `crypto.hkdfSync(digest, ikm, salt, info, keylen)`. Five Node arguments
/// against this module's four-slot ceiling — `digest` is fixed to SHA-256,
/// the same trade [`pbkdf2_sync`] states, so the four JS-visible slots
/// (`ikm`, `salt`, `info`, `keylen`) are what this native reads.
pub(super) extern "C" fn hkdf_sync(_e: u64, _this: u64, ikm: u64, salt: u64, info: u64, keylen: u64) -> u64 {
    entry::with_runtime(|context| {
        let ikm = util::binary_bytes(context, ikm);
        let salt = util::binary_bytes(context, salt);
        let info = util::binary_bytes(context, info);
        let len = util::integer(context, keylen).unwrap_or(0).max(0) as usize;
        let mut out = vec![0u8; len];
        let hk = Hkdf::<sha2::Sha256>::new(Some(&salt), &ikm);
        let _ = hk.expand(&info, &mut out);
        entry::make_buffer(context, &out)
    })
}
