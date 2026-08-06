//! `node:crypto` — over `docs/reference/node/crypto.md`, scoped to hashing,
//! HMAC, CSPRNG randomness and the three synchronous KDFs (§2.2's own
//! grouping: "Hashing / MAC", "Randomness", "Key derivation").
//!
//! # `Hash`/`Hmac`: one shared prototype, native state in a table
//!
//! A `Hash`/`Hmac` instance is an ordinary object linked to ONE prototype
//! per class (through [`rts_core_rwk::entry::make_prototype`]/
//! `make_instance`, the pattern `fs/dirent.rs` is the worked example for) —
//! `Object.keys(hash)` sees the one hidden id property this module puts
//! there, not the methods, and every `Hash` shares the same `update`/
//! `digest` function values. The digest state itself (`sha2::Sha256` etc,
//! mid-update) is native and cannot live on a JS value, so it lives in a
//! `Mutex`-backed table keyed by that id — the same shape `fs/fd.rs`'s open
//! files and `fs/dir.rs`'s directory cursors already use; see [`hash`]'s and
//! [`hmac`]'s own module docs for the details specific to each.
//!
//! # Bytes: `Uint8Array`, not a `Buffer` instance
//!
//! `randomBytes`/`digest()`-with-no-encoding answer through
//! [`rts_core_rwk::entry::make_buffer`], which is a real `Buffer` — the class the
//! runtime now carries, so `Buffer.isBuffer` of what this answers is true
//! of the bytes — the same divergence `node:fs`'s `readFileSync` already
//! states for the same reason (a `Buffer` instance can only be built through
//! `Buffer`'s own constructor path, which this module has no way to hand raw
//! bytes to directly). `digest("hex"/"base64"/"base64url"/…)` and every
//! other string-encoded answer goes through the SAME codec `Buffer` uses
//! ([`rts_core_rwk::entry::decode_bytes`]/`encode_text`), never a second hex/
//! base64 implementation.
//!
//! # Not implemented, by name
//!
//! Everything in §2 this module does not cover, and the mechanism each
//! waits on:
//! - **Symmetric ciphers** (`Cipheriv`/`Decipheriv`, `createCipheriv`/
//!   `createDecipheriv`, `getCipherInfo`) — out of the task's scope; the
//!   vetted crates (`aes`/`cbc`/`ctr`/`aes-gcm`/`chacha20poly1305`, crates.md
//!   §4.2) are not pulled in by this change. `getCiphers()` answers `[]`
//!   rather than a name list nothing here backs.
//! - **Asymmetric keys and X.509** (`KeyObject`, `Sign`/`Verify`,
//!   `DiffieHellman`/`DiffieHellmanGroup`/`ECDH`, `X509Certificate`,
//!   `generateKeyPair(Sync)`, `createPrivateKey`/`createPublicKey`/
//!   `createSecretKey`, `checkPrime(Sync)`/`generatePrime(Sync)`) — needs
//!   DER/PEM/PKCS8/SPKI encoding infrastructure (crates.md §4.3) this change
//!   does not add; every one of these is a JSON-option-object or
//!   variable-shape-return API the reference's §5.2 already flags as needing
//!   more than the four-argument/scalar-return ceiling this module works
//!   under for a single change.
//! - **`WebCrypto`** (`globalThis.crypto`, `crypto.subtle`/`SubtleCrypto`,
//!   `CryptoKey`) — every `SubtleCrypto` method is Promise-mandatory
//!   (§5.3), and this module has no Promise-settling path (`with_runtime`'s
//!   nested-borrow-aborts rule means a native cannot call `promise.create`
//!   from inside another entry point either) — the same "no user-code
//!   re-entry" limit `fs/mod.rs`'s module doc states blocks it here too.
//! - **`argon2`/`argon2Sync`** — experimental in Node itself (§2.2's own
//!   tag); deferred per "immature-goes-last".
//! - **Callback/async forms of every function above** (`randomBytes(size,
//!   cb)`, `pbkdf2`, `scrypt`, `hkdf`, …) — needs the shared tokio runtime
//!   §5.7 flags as living in `rts-std`, which this crate must not depend on.
//! - **`hash.copy()`**, `HashOptions.outputLength` (SHAKE only, and SHAKE
//!   itself is not implemented — see `digest_algo.rs`), `crypto.constants`,
//!   `crypto.fips`/`getFips`/`setFips`, `setEngine`, `secureHeapUsed` — none
//!   scoped by the task brief.

mod digest_algo;
mod hash;
mod hmac;
mod kdf;
mod random;
mod util;

use rts_core_rwk::entry::{self, Context, Provided};

/// The namespace `node:crypto` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("createHash", hash::create_hash),
        ("hash", hash::hash_oneshot),
        ("getHashes", hash::get_hashes),
        ("createHmac", hmac::create_hmac),
        ("randomBytes", random::random_bytes),
        ("randomFillSync", random::random_fill_sync),
        ("randomInt", random::random_int),
        ("randomUUID", random::random_uuid),
        ("getRandomValues", random::get_random_values),
        ("timingSafeEqual", random::timing_safe_equal),
        ("pbkdf2Sync", kdf::pbkdf2_sync),
        ("scryptSync", kdf::scrypt_sync),
        ("hkdfSync", kdf::hkdf_sync),
        ("getCiphers", get_ciphers),
    ];
    entry::make_namespace(context, members)
}

/// `crypto.getCiphers()` — always `[]`; see this module's "Not implemented"
/// note. Answering an empty list rather than `undefined` matches Node's own
/// return type (`string[]`) on a build with no ciphers compiled in.
extern "C" fn get_ciphers(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| entry::make_array_in(context, Vec::new()))
}
