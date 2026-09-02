//! X25519 key agreement and XEdDSA signatures, as `node:crypto` members.
//!
//! # These names are not Node's, and that is deliberate
//!
//! Node spells key agreement `generateKeyPairSync('x25519')` →
//! `diffieHellman({ privateKey, publicKey })`, over `KeyObject` values that
//! carry a DER/PEM encoding and an algorithm identifier. This crate has no
//! `KeyObject` — that is the entry `mod.rs` lists under "Asymmetric keys and
//! X.509", and it needs the encoding infrastructure that entry names.
//!
//! So these are `generateX25519KeyPair`, `x25519PublicKey`,
//! `x25519DiffieHellman`, `xeddsaSign` and `xeddsaVerify`: raw 32-byte keys in,
//! raw bytes out. A non-standard name that does exactly what it says is the
//! honest half of the trade — the alternative was `generateKeyPairSync`
//! answering something shaped like a `KeyObject` and missing `export()`,
//! `asymmetricKeyType` and everything else a caller reaches for next, which is
//! the hollow surface CLAUDE.md refuses by name. When `KeyObject` arrives, these
//! stay: they are what the standard names will be built on, not a stand-in for
//! them.
//!
//! XEdDSA has no Node spelling at all — it is Signal's, not the platform's —
//! so no name was displaced by choosing one.
//!
//! # Why the keypair is an object and the rest are arrays
//!
//! `generateX25519KeyPair()` answers `{ privateKey, publicKey }` because two
//! values have to come back together. Everything else answers one `Buffer`.
//! There is a known engine gap around the object form worth stating for anyone
//! writing against it: a FIELD read off an ad-hoc shaped object is not
//! statically proven to be array-typed, so `generateX25519KeyPair().publicKey`
//! passed straight into another native does not always arrive as bytes.
//! `tests/claude-crypto-aes-x25519.test.ts` works around it by deriving the
//! public key with [`x25519_public_key`] instead, and the gap is the emitter's
//! rather than this module's.

mod xeddsa;

use rts_core::entry::{self, Provided};
use x25519_dalek::{PublicKey, StaticSecret};

use super::random;
use super::util;

/// Every member this module contributes to `node:crypto`, so `mod.rs` chains
/// one list rather than five lines that can each be forgotten separately — the
/// same reason `kdf::MEMBERS` exists.
pub(super) const MEMBERS: &[(&str, Provided)] = &[
    ("generateX25519KeyPair", generate_key_pair),
    ("x25519PublicKey", x25519_public_key),
    ("x25519DiffieHellman", x25519_diffie_hellman),
    ("xeddsaSign", xeddsa_sign),
    ("xeddsaVerify", xeddsa_verify),
];

/// A 32-byte argument, or `None` when it is any other length. Every key and
/// public value on this curve is exactly 32 bytes, and a shorter one silently
/// zero-extended would agree on a shared secret with nobody while looking like
/// it worked.
fn key_bytes(context: &rts_core::entry::Context, value: u64) -> Option<[u8; 32]> {
    util::binary_bytes(context, value).try_into().ok()
}

/// `crypto.generateX25519KeyPair()` — `{ privateKey, publicKey }`, both 32-byte
/// `Buffer`s.
///
/// The secret comes from the OS CSPRNG through `random::os_bytes`, the same
/// draw `randomBytes` makes. Not `x25519_dalek`'s own `StaticSecret::random`:
/// that would bind this crate to a `rand_core` version as a second source of
/// randomness, and one CSPRNG per process is the point of having one.
extern "C" fn generate_key_pair(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&random::os_bytes(32));
    let secret = StaticSecret::from(seed);
    let public = PublicKey::from(&secret);
    entry::with_runtime(|context| {
        let pair = entry::make_object(context);
        let private_value = entry::make_buffer(context, &secret.to_bytes());
        entry::put_member(context, pair, "privateKey", private_value);
        let public_value = entry::make_buffer(context, public.as_bytes());
        entry::put_member(context, pair, "publicKey", public_value);
        pair
    })
}

/// `crypto.x25519PublicKey(privateKey)` — the 32-byte public key for a private
/// one. Deterministic, which is what makes it usable to recover a public key
/// from stored credentials rather than storing both.
extern "C" fn x25519_public_key(_e: u64, _this: u64, private: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let bytes = entry::with_runtime(|context| key_bytes(context, private));
    let Some(bytes) = bytes else {
        entry::throw_type_error("privateKey must be 32 bytes");
        return entry::undefined_value();
    };
    let public = PublicKey::from(&StaticSecret::from(bytes));
    entry::with_runtime(|context| entry::make_buffer(context, public.as_bytes()))
}

/// `crypto.x25519DiffieHellman(privateKey, publicKey)` — the 32-byte shared
/// secret. This is the X3DH primitive; a Signal-style handshake calls it four
/// times and feeds the concatenation to HKDF, which `kdf` already provides.
///
/// An all-zero result (a low-order public key) is answered rather than refused,
/// which is what `x25519-dalek` without its `reject-low-order` behaviour does
/// and what every other X25519 implementation on the wire does. A protocol that
/// must reject it checks the output, and this is stated rather than silently
/// chosen.
extern "C" fn x25519_diffie_hellman(
    _e: u64,
    _this: u64,
    private: u64,
    public: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let pair = entry::with_runtime(|context| {
        let private = key_bytes(context, private)?;
        let public = key_bytes(context, public)?;
        Some((private, public))
    });
    let Some((private, public)) = pair else {
        entry::throw_type_error("privateKey and publicKey must each be 32 bytes");
        return entry::undefined_value();
    };
    let shared = StaticSecret::from(private).diffie_hellman(&PublicKey::from(public));
    entry::with_runtime(|context| entry::make_buffer(context, shared.as_bytes()))
}

/// `crypto.xeddsaSign(privateKey, message)` — a 64-byte XEdDSA signature under
/// the X25519 private key, per Signal's specification.
///
/// The 64 random bytes the scheme calls `Z` are drawn HERE, not taken as an
/// argument: a caller supplying them is a caller who can supply the same ones
/// twice, and a repeated `Z` under one key leaks the private key. `xeddsa.rs`
/// takes them as a parameter so its tests can pin a vector; that parameter is
/// not exposed to JavaScript, and this line is why.
extern "C" fn xeddsa_sign(_e: u64, _this: u64, private: u64, message: u64, _a2: u64, _a3: u64) -> u64 {
    let input = entry::with_runtime(|context| {
        let private = key_bytes(context, private)?;
        Some((private, util::binary_bytes(context, message)))
    });
    let Some((private, message)) = input else {
        entry::throw_type_error("privateKey must be 32 bytes");
        return entry::undefined_value();
    };
    let mut nonce = [0u8; 64];
    nonce.copy_from_slice(&random::os_bytes(64));
    let signature = xeddsa::sign(&private, &message, &nonce);
    entry::with_runtime(|context| entry::make_buffer(context, &signature))
}

/// `crypto.xeddsaVerify(publicKey, message, signature)` — `true` or `false`.
///
/// A malformed argument answers `false` rather than throwing, which is the one
/// place in this module where a bad input is not an error: every caller is
/// verifying something that arrived over a network, so "this did not verify" is
/// the answer for a 63-byte signature just as much as for a wrong one. Throwing
/// would push every call site into a `try` whose catch block means the same
/// thing as `false`.
extern "C" fn xeddsa_verify(_e: u64, _this: u64, public: u64, message: u64, signature: u64, _a3: u64) -> u64 {
    let ok = entry::with_runtime(|context| {
        (|| {
            let public = key_bytes(context, public)?;
            let signature: [u8; 64] = util::binary_bytes(context, signature).try_into().ok()?;
            let message = util::binary_bytes(context, message);
            Some(xeddsa::verify(&public, &message, &signature))
        })()
        .unwrap_or(false)
    });
    entry::boolean_value(ok)
}
