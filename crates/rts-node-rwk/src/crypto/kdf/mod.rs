//! Key derivation — `pbkdf2`, `scrypt`, `hkdf` and their `*Sync` twins, over
//! `docs/reference/node/crypto.md` §3 and §2.2's "Key derivation" grouping.
//!
//! # What this replaces, and why it is a folder now
//!
//! `NODE_COMPATIBLE.md` ranks these eighth of everything missing from
//! `node:crypto`, and not because they were absent: "these currently return
//! confidently wrong bytes, which is worse than returning none". The three
//! failures it measured were `pbkdf2Sync` ignoring `digest`, `scryptSync`
//! discarding `N`/`r`/`p` and not being scrypt, and `hkdfSync` answering zero
//! bytes. All three were one file that mixed argument reading, cost-parameter
//! arithmetic and the derivation itself, which is why they could only be found
//! by running a JavaScript program and comparing hex by eye.
//!
//! The split is what makes them checkable:
//!
//! | module | holds | tested by |
//! |---|---|---|
//! | [`derive`] | the derivations, as slices → `Vec<u8>` | RFC vectors, in-crate |
//! | [`args`] | Node's argument order and defaults | — |
//! | [`sync`] | the `*Sync` entry points | — |
//! | [`deferred`] | the callback forms and their delivery | — |
//! | [`vectors`] | RFC 6070, RFC 7914 §12, RFC 5869 §A | itself |
//!
//! Nothing in `sync`/`deferred` does arithmetic an RFC has an opinion about. A
//! future wrong answer therefore has one file it can be in.
//!
//! # Reuse-check (`.claude/skills/reuse-check`)
//!
//! Stated per module where it bites: [`derive`] for the digest name list (which
//! is `super::hmac`'s seven and says why it is not `super::digest_algo`'s
//! fifteen), and [`deferred`] for the deferral (which is
//! `entry::declare_loop_source`, not a second queue). Nothing here mints a
//! number, so reuse-check §3 has nothing to point at.
//!
//! # Not implemented, by name
//!
//! - **`argon2`/`argon2Sync`** — experimental in Node itself (`crypto.md` §2.2
//!   tags it), and no `argon2` crate is a dependency. Adding one is a manifest
//!   change this does not make silently.
//! - **A `KeyObject` or `CryptoKey` as `hkdf`'s `ikm`** — `crypto.md` §3 types
//!   it `BinaryLike | KeyObject | CryptoKey`, and neither key class exists in
//!   this crate. A `KeyObject` passed here reads as empty bytes through
//!   `util::binary_like`, which is a wrong ACCEPT; it cannot be refused without
//!   a brand this crate has no way to check.
//! - **`ERR_CRYPTO_INVALID_SCRYPT_PARAMS` and `ERR_OUT_OF_RANGE` as themselves**
//!   — every refusal here is a `TypeError` carrying Node's message, because
//!   `entry::throw_type_error` is the only raise a host crate can reach and its
//!   own doc says why.
//! - **`options.maxmem` on `pbkdf2`** — Node has no such option there; named
//!   only because `scrypt` does and the asymmetry looks like an omission.

mod args;
mod derive;
mod deferred;
mod sync;
#[cfg(test)]
mod vectors;

use rts_core_rwk::entry::Provided;

pub(super) use deferred::declare;

/// Every member `node:crypto` installs from this module, with Node's names.
///
/// A list rather than six `pub(super)` re-exports: the sync and callback forms
/// of one KDF are two spellings of one agreement, and keeping them adjacent is
/// what stops a fix landing in one and not the other. `super::namespace` splices
/// this in rather than restating the names.
pub(super) const MEMBERS: &[(&str, Provided)] = &[
    ("pbkdf2", deferred::pbkdf2),
    ("pbkdf2Sync", sync::pbkdf2_sync),
    ("scrypt", deferred::scrypt),
    ("scryptSync", sync::scrypt_sync),
    ("hkdf", deferred::hkdf),
    ("hkdfSync", sync::hkdf_sync),
];

/// `crypto.getHashes()`-style list of the digests the KDFs here accept.
///
/// Not installed as a member — Node has no such function — but read by
/// [`vectors`] to assert [`derive::DIGESTS`] and the dispatch cannot drift.
#[cfg(test)]
pub(super) fn digests() -> &'static [&'static str] {
    derive::DIGESTS
}
