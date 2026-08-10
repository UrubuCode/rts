//! `node:crypto` — over `docs/reference/node/crypto.md`, scoped to hashing,
//! HMAC, CSPRNG randomness and the three KDFs in both their spellings (§2.2's
//! own grouping: "Hashing / MAC", "Randomness", "Key derivation").
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
//! - **`randomBytes(size, cb)` and the other callback forms outside `kdf`** —
//!   the offload gap `random.rs`'s doc states. `pbkdf2`, `scrypt` and `hkdf` are
//!   NO LONGER on this list: they exist, they derive synchronously and they
//!   deliver on the next pump of the event loop through a loop source, which is
//!   real deferral without the tokio runtime §5.7 puts in `rts-std`. What they
//!   do not get is Node's thread pool — see `kdf/deferred.rs` for what that
//!   costs and why offering them anyway is the better trade.
//! - **`hash.copy()`**, `HashOptions.outputLength` (SHAKE only, and SHAKE
//!   itself is not implemented — see `digest_algo.rs`),
//!   `crypto.fips`/`getFips`/`setFips`, `setEngine`, `secureHeapUsed` — none
//!   scoped by the task brief.
//! - **The OpenSSL half of `crypto.constants`** — `SSL_OP_*`,
//!   `defaultCoreCipherList`, `defaultCipherList`, `POINT_CONVERSION_*`,
//!   `ENGINE_METHOD_*`. Those describe a TLS/engine surface this crate does
//!   not link, so a value for one would name a switch nothing here reads.
//!   [`constants`] carries the RSA padding and PSS salt-length numbers only:
//!   those are protocol constants a program compares against, not handles into
//!   a library. [`CONSTANTS`] is the table itself, lifted out of [`constants`]
//!   so that Node's deprecated flattened `constants` module can spread THIS
//!   list rather than keep a second copy of the same eight numbers.

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
        ("getCiphers", get_ciphers),
    ];
    // The six KDF members come from [`kdf::MEMBERS`] rather than being listed
    // here. `pbkdf2`/`pbkdf2Sync` are two spellings of one agreement, and this
    // list is where a fix could land in one spelling and not the other.
    let all: Vec<(&str, Provided)> = members.iter().chain(kdf::MEMBERS.iter()).copied().collect();
    let namespace = entry::make_namespace(context, &all);
    let constants = constants(context);
    entry::put_member(context, namespace, "constants", constants);
    // The callback forms derive synchronously and DELIVER on the next pump of
    // the event loop, which needs a loop source. Declared here, at install time
    // — `entry::declare_loop_source`'s doc says why not by the host — and
    // idempotent by name, so a second `namespace` call does not pump twice.
    kdf::declare(context);
    namespace
}

/// `crypto.constants` — the RSA padding and PSS salt-length numbers.
///
/// # Why these exist without the ciphers that use them
///
/// They are the argument a program WRITES: `padding:
/// constants.RSA_PKCS1_OAEP_PADDING` is typed against this object long before
/// the call that consumes it, and reading `undefined` there makes the mistake
/// surface as `NaN` inside an unrelated function rather than at the name. The
/// numbers are OpenSSL's `rsa.h` values, which is why they can be stated
/// without linking it — they are wire-level constants, not handles.
///
/// See this module's "Not implemented" note for the rest of Node's own
/// `constants` object and why an OpenSSL option flag is not stated here.
fn constants(context: &mut Context) -> u64 {
    let object = entry::make_object(context);
    for (name, value) in CONSTANTS {
        let number = entry::make_number(*value);
        entry::put_member(context, object, name, number);
    }
    object
}

/// The numbers [`constants`] is built from, lifted out of it for one reason:
/// Node's deprecated `constants` module spreads `crypto.constants` flat beside
/// `fs`'s and `os`'s, and a second list of RSA padding numbers written there
/// is the duplication this crate's reuse rule exists to refuse. Lifting the
/// table costs nothing and removes the option of typing them twice.
pub(crate) static CONSTANTS: &[(&str, f64)] = &[
    ("RSA_PKCS1_PADDING", 1.0),
    ("RSA_NO_PADDING", 3.0),
    ("RSA_PKCS1_OAEP_PADDING", 4.0),
    ("RSA_X931_PADDING", 5.0),
    ("RSA_PKCS1_PSS_PADDING", 6.0),
    // `-1` means "as long as the digest", `-2` "as long as possible".
    ("RSA_PSS_SALTLEN_DIGEST", -1.0),
    ("RSA_PSS_SALTLEN_MAX_SIGN", -2.0),
    ("RSA_PSS_SALTLEN_AUTO", -2.0),
];

/// `crypto.getCiphers()` — always `[]`; see this module's "Not implemented"
/// note. Answering an empty list rather than `undefined` matches Node's own
/// return type (`string[]`) on a build with no ciphers compiled in.
extern "C" fn get_ciphers(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| entry::make_array_in(context, Vec::new()))
}
