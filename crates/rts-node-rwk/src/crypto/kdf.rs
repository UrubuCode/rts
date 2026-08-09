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

/// Which digest each construction here dispatches over, in one place.
///
/// A macro rather than an enum: `pbkdf2_hmac` and `Hkdf` are both generic over
/// the digest TYPE, and an enum would need a variant carrying an instantiated
/// value — which neither of these constructions has, since both take the type
/// and the bytes together. The list is [`super::hmac`]'s seven, and for its
/// reason: both are HMAC underneath, so a name `Hmac<D>` cannot be built over
/// is a name neither of these can use either.
macro_rules! over_digest {
    ($name:expr, $body:ident) => {
        match $name.to_ascii_lowercase().as_str() {
            "sha256" => Some($body!(sha2::Sha256)),
            "sha512" => Some($body!(sha2::Sha512)),
            "sha384" => Some($body!(sha2::Sha384)),
            "sha224" => Some($body!(sha2::Sha224)),
            "sha1" => Some($body!(sha1::Sha1)),
            "md5" => Some($body!(md5::Md5)),
            "ripemd160" => Some($body!(ripemd::Ripemd160)),
            _ => None,
        }
    };
}

/// `crypto.pbkdf2Sync(password, salt, iterations, keylen, digest)`. An
/// unsupported `digest` name, or a `keylen`/`iterations` that does not fit a
/// `u32`, answers all-zero bytes rather than throwing — Node requires an
/// explicit `digest` (§4: "omitting it ... is now a hard `TypeError`"); this
/// module has no throw to raise that with, so that is the stand-in, named here
/// rather than silently matching Node's old SHA1-default behaviour.
///
/// `digest` is Node's FIFTH argument, past the four the calling convention
/// carries — read through [`util::extra_argument`], which is where the
/// activation's own argument vector is. An absent one means SHA-256, which is
/// what this function did unconditionally before the fifth was reachable.
pub(super) extern "C" fn pbkdf2_sync(_e: u64, _this: u64, password: u64, salt: u64, iterations: u64, keylen: u64) -> u64 {
    // Every argument is read BEFORE the borrow is opened, because two of these
    // readers are ambient entry points that take a borrow of their own — see
    // [`util::binary_like`] and [`util::extra_argument`], and `assert/mod.rs`
    // for the same discipline stated over options objects.
    let named = util::extra_argument(4, password, salt, iterations, keylen);
    let digest = entry::with_runtime(|context| util::text(context, named))
        .unwrap_or_else(|| "sha256".to_owned());
    let rounds = entry::number_of(iterations).unwrap_or(0.0).max(0.0) as u32;
    let len = entry::number_of(keylen).unwrap_or(0.0).max(0.0) as usize;
    let password = util::binary_like(password);
    let salt = util::binary_like(salt);
    let mut out = vec![0u8; len];
    macro_rules! derive {
        ($d:ty) => {
            pbkdf2::pbkdf2_hmac::<$d>(&password, &salt, rounds, &mut out)
        };
    }
    let _: Option<()> = over_digest!(digest, derive);
    entry::with_runtime(|context| entry::make_buffer(context, &out))
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

/// `crypto.hkdfSync(digest, ikm, salt, info, keylen)`. Node's argument order
/// exactly: `digest` is the first slot and `keylen` the fifth, read through
/// [`util::extra_argument`].
///
/// This used to read the four slots as `(ikm, salt, info, keylen)` — one
/// position left of Node's — so every call derived from the wrong bytes with
/// the wrong length and answered zeros, which is the shape of wrong answer
/// this repository refuses: it ran, it returned a `Buffer`, and nothing said
/// it was not HKDF.
pub(super) extern "C" fn hkdf_sync(_e: u64, _this: u64, digest: u64, ikm: u64, salt: u64, info: u64) -> u64 {
    // Read before the borrow, for [`pbkdf2_sync`]'s reason.
    let named = util::extra_argument(4, digest, ikm, salt, info);
    let digest = entry::with_runtime(|context| util::text(context, digest))
        .unwrap_or_else(|| "sha256".to_owned());
    let len = entry::number_of(named).unwrap_or(0.0).max(0.0) as usize;
    let ikm = util::binary_like(ikm);
    let salt = util::binary_like(salt);
    let info = util::binary_like(info);
    let mut out = vec![0u8; len];
    macro_rules! expand {
        ($d:ty) => {
            Hkdf::<$d>::new(Some(&salt), &ikm).expand(&info, &mut out).is_ok()
        };
    }
    // An unsupported digest name, or a `keylen` past what the digest can
    // expand to (255 blocks, RFC 5869 §2.3), leaves the zeros: neither can be
    // reported as the `TypeError`/`RangeError` Node raises until a native here
    // can throw. Named rather than silently truncated.
    let _: Option<bool> = over_digest!(digest, expand);
    entry::with_runtime(|context| entry::make_buffer(context, &out))
}
