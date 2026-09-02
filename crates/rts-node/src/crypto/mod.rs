//! `node:crypto` — over `docs/reference/node/crypto.md`, scoped to hashing,
//! HMAC, CSPRNG randomness and the three KDFs in both their spellings (§2.2's
//! own grouping: "Hashing / MAC", "Randomness", "Key derivation").
//!
//! # `Hash`/`Hmac`: one shared prototype, native state in a table
//!
//! A `Hash`/`Hmac` instance is an ordinary object linked to ONE prototype
//! per class (through [`rts_core::entry::make_prototype`]/
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
//! [`rts_core::entry::make_buffer`], which is a real `Buffer` — the class the
//! runtime now carries, so `Buffer.isBuffer` of what this answers is true
//! of the bytes — the same divergence `node:fs`'s `readFileSync` already
//! states for the same reason (a `Buffer` instance can only be built through
//! `Buffer`'s own constructor path, which this module has no way to hand raw
//! bytes to directly). `digest("hex"/"base64"/"base64url"/…)` and every
//! other string-encoded answer goes through the SAME codec `Buffer` uses
//! ([`rts_core::entry::decode_bytes`]/`encode_text`), never a second hex/
//! base64 implementation.
//!
//! # Not implemented, by name
//!
//! Everything in §2 this module does not cover, and the mechanism each
//! waits on:
//! - **Symmetric ciphers beyond AES-128/256 in GCM and CBC** — CTR, ECB,
//!   ChaCha20-Poly1305 and the `-wrap` family. `createCipheriv`/
//!   `createDecipheriv` and `getCipherInfo`'s companion `getCiphers()` now
//!   answer over the four names `cipher/algo.rs` genuinely computes, and that
//!   file's `CipherAlgo::NAMES` is the one list both the parser and
//!   `getCiphers()` read — a name accepted by one and refused by the other was
//!   the shape this used to have (`getCiphers()` answered `[]` while nothing
//!   backed any name at all). `getCipherInfo` itself is still absent.
//! - **Asymmetric keys and X.509** (`KeyObject`, `Sign`/`Verify`,
//!   `DiffieHellman`/`DiffieHellmanGroup`/`ECDH`, `X509Certificate`,
//!   `generateKeyPair(Sync)`, `createPrivateKey`/`createPublicKey`/
//!   `createSecretKey`, `checkPrime(Sync)`/`generatePrime(Sync)`) — needs
//!   DER/PEM/PKCS8/SPKI encoding infrastructure (crates.md §4.3) this change
//!   does not add; every one of these is a JSON-option-object or
//!   variable-shape-return API the reference's §5.2 already flags as needing
//!   more than the four-argument/scalar-return ceiling this module works
//!   under for a single change.
//!
//!   Curve25519 is the one exception, and it is deliberately spelled OUTSIDE
//!   Node's names rather than approximating them: [`curve`] carries
//!   `generateX25519KeyPair`, `x25519PublicKey`, `x25519DiffieHellman`,
//!   `xeddsaSign` and `xeddsaVerify` over raw 32-byte keys. Its module doc has
//!   the reasoning; the short form is that a `generateKeyPairSync` answering
//!   something shaped like a `KeyObject` and missing `export()` is the hollow
//!   surface this repository refuses, and a non-standard name that does exactly
//!   what it says is not.
//! - **Every `SubtleCrypto` method except `digest`**, and `CryptoKey` with them.
//!   This entry said the whole of WebCrypto was blocked because "every
//!   `SubtleCrypto` method is Promise-mandatory and this module has no
//!   Promise-settling path" — which had expired: `entry::settled` is what
//!   `node:buffer`'s `Blob` answers its three async readers with, and a digest
//!   over resident bytes has no asynchrony beyond the promise shape. So
//!   `globalThis.crypto`, `crypto.subtle` and `crypto.webcrypto` exist and
//!   [`webcrypto`] says which member of them does not.
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

mod cipher;
mod curve;
mod digest_algo;
mod hash;
mod hmac;
mod kdf;
mod random;
mod util;
pub(crate) mod webcrypto;

use rts_core::entry::{self, Context, Provided};

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
        ("createCipheriv", cipher::create_cipheriv),
        ("createDecipheriv", cipher::create_decipheriv),
        ("getCiphers", cipher::get_ciphers),
    ];
    // The six KDF members come from [`kdf::MEMBERS`] rather than being listed
    // here. `pbkdf2`/`pbkdf2Sync` are two spellings of one agreement, and this
    // list is where a fix could land in one spelling and not the other.
    let all: Vec<(&str, Provided)> = members
        .iter()
        .chain(kdf::MEMBERS.iter())
        .chain(curve::MEMBERS.iter())
        .copied()
        .collect();
    let namespace = entry::make_namespace(context, &all);
    let constants = constants(context);
    entry::put_member(context, namespace, "constants", constants);
    // The callback forms derive synchronously and DELIVER on the next pump of
    // the event loop, which needs a loop source. Declared here, at install time
    // — `entry::declare_loop_source`'s doc says why not by the host — and
    // idempotent by name, so a second `namespace` call does not pump twice.
    kdf::declare(context);
    // `crypto.webcrypto` and `crypto.subtle` are the SAME objects the `crypto`
    // global carries, not a second pair: `webcrypto::object` is memoized per
    // context, which is what makes `globalThis.crypto === require('node:crypto')
    // .webcrypto` hold the way it does in Node.
    let web = webcrypto::object(context);
    entry::put_member(context, namespace, "webcrypto", web);
    let subtle = webcrypto::subtle(context);
    entry::put_member(context, namespace, "subtle", subtle);
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

