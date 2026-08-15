//! `globalThis.crypto` — the WebCrypto `Crypto` object, and the one
//! `SubtleCrypto` method this crate can answer exactly.
//!
//! # What reuse-check found, and the reason that had expired
//!
//! Every byte of hashing here is [`super::digest_algo::HashState`]'s, which
//! `createHash` and `hash` already dispatch over — this file adds no algorithm,
//! no table and no `Digest` impl. The random members are [`super::random`]'s,
//! the same function pointers `node:crypto` exports.
//!
//! `crypto/mod.rs` listed WebCrypto as not implemented because "every
//! `SubtleCrypto` method is Promise-mandatory and this module has no
//! Promise-settling path". That stopped being true: `entry::settled` builds an
//! already-settled promise, `node:buffer`'s `Blob` has used it for its three
//! async readers all along, and a digest over resident bytes has no asynchrony
//! to express beyond the promise SHAPE the signature has.
//!
//! # Why one `SubtleCrypto` method and not a stub for the rest
//!
//! Because a surface that cannot do what its name means must not ship, and
//! `digest` is the one that can: the bytes are here, the algorithm is here, and
//! the answer is exact. `encrypt`, `sign`, `deriveKey`, `importKey` and the rest
//! need `CryptoKey`, key formats and cipher suites this crate does not have, so
//! they are ABSENT — `crypto.subtle.encrypt(...)` fails at the call, naming the
//! callee, rather than resolving to something that looks like ciphertext.
//!
//! # Not implemented, by name
//!
//! - **Every `SubtleCrypto` member except `digest`**, and `CryptoKey` with them.
//! - **A `DOMException` for a rejected digest.** The spec rejects with
//!   `NotSupportedError`; that class lives in `rts-std`, which this crate does
//!   not depend on, so an unsupported algorithm rejects with a plain `Error`
//!   carrying the algorithm name. Named rather than faked with an object that
//!   merely has a `name` property.
//! - **SHA-3 and the rest of [`super::digest_algo::NAMES`] through `subtle`.**
//!   WebCrypto defines exactly four digest algorithms, and accepting a fifth
//!   would make code written here fail in every other engine.

use rts_core::entry::{self, Context, Provided};

use super::digest_algo::HashState;

const CRYPTO_MEMBERS: &[(&str, Provided)] = &[
    ("randomUUID", super::random::random_uuid),
    ("getRandomValues", super::random::get_random_values),
];

const SUBTLE_MEMBERS: &[(&str, Provided)] = &[("digest", digest)];

/// The one `Crypto` object per context.
///
/// # Why `make_prototype` holds a singleton rather than a prototype
///
/// Because what it really keeps is `(name, object)` per context, which is
/// exactly what a singleton needs and the only such place a host module may
/// write — `perf_hooks` keeps the `performance` object the same way, and for the
/// same reason: `globalThis.crypto === (await import("node:crypto")).webcrypto`
/// is observable, the two are reached at different times, and a `static` would
/// be process-global where a context is per-thread.
pub(crate) fn object(context: &mut Context) -> u64 {
    let crypto = entry::make_prototype(context, "Crypto", CRYPTO_MEMBERS);
    let subtle = entry::make_prototype(context, "SubtleCrypto", SUBTLE_MEMBERS);
    entry::put_member(context, crypto, "subtle", subtle);
    crypto
}

/// The `SubtleCrypto` object, for `node:crypto`'s own `subtle` export.
pub(super) fn subtle(context: &mut Context) -> u64 {
    let crypto = object(context);
    entry::get_member(context, crypto, "subtle")
}

/// `crypto.subtle.digest(algorithm, data)` — a settled promise of an
/// `ArrayBuffer`.
///
/// The `ArrayBuffer` is reached through the `buffer` property of a fresh
/// `Uint8Array` rather than built here, which is `Blob.arrayBuffer`'s own
/// reasoning: there is no host entry that mints a bare one, and inventing one
/// would be a second answer to what an `ArrayBuffer` is.
extern "C" fn digest(_e: u64, _this: u64, algorithm: u64, data: u64, _c: u64, _d: u64) -> u64 {
    let named = algorithm_name(algorithm);
    let bytes = entry::with_runtime(|context| entry::bytes_of(context, data)).unwrap_or_default();
    let Some(mut state) = named.as_deref().and_then(supported).and_then(HashState::new) else {
        let asked = named.unwrap_or_default();
        let error = entry::make_named_error("Error", &format!("Unsupported digest algorithm: {asked}"))
            .unwrap_or_else(entry::undefined_value);
        return entry::with_runtime(|context| entry::settled(context, error, true));
    };
    state.update(&bytes);
    let digest = state.finalize();
    entry::with_runtime(|context| {
        let view = entry::make_bytes(context, &digest);
        let buffer = entry::get_member(context, view, "buffer");
        entry::settled(context, buffer, false)
    })
}

/// The algorithm argument, which WebCrypto accepts as a string or as an object
/// with a `name`.
fn algorithm_name(algorithm: u64) -> Option<String> {
    if let Some(name) = entry::with_runtime(|context| entry::string_in(context, algorithm)) {
        return Some(name);
    }
    let named = entry::with_runtime(|context| entry::get_member(context, algorithm, "name"));
    entry::with_runtime(|context| entry::string_in(context, named))
}

/// The [`super::digest_algo`] name for a WebCrypto one — `None` for anything
/// outside the four the standard defines.
///
/// The spelling differs on purpose and this is the whole of the translation:
/// WebCrypto writes `SHA-256`, Node's `createHash` writes `sha256`, and there
/// is one algorithm behind both.
fn supported(name: &str) -> Option<&'static str> {
    match name.to_ascii_uppercase().as_str() {
        "SHA-1" => Some("sha1"),
        "SHA-256" => Some("sha256"),
        "SHA-384" => Some("sha384"),
        "SHA-512" => Some("sha512"),
        _ => None,
    }
}
