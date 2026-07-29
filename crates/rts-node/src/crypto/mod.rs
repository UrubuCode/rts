//! `node:crypto` — the self-contained hashing / HMAC / random surface. Real
//! cryptographic primitives via RustCrypto (`md-5`/`sha1`/`sha2` over
//! `DynDigest`) + OS entropy (`getrandom`). No stubs, no fabricated output.
//!
//! Surface: `createHash`/`createHmac` (returning a `Hash` instance with
//! `update`/`digest`), the single-shot `crypto.hash`, `randomBytes`,
//! `randomUUID`, `randomInt`, `timingSafeEqual`, `getHashes`.
//!
//! `Hash`/`Hmac` and `Cipheriv`/`Decipheriv` are `#[rtse::class]` structs
//! (`hash.rs` / `cipher_instance.rs`, `Entry::Rtse` instances):
//! `createHash`/`createHmac`/`createCipheriv`/`createDecipheriv` build them
//! directly via `alloc_rtse` (none is reached through `new X()` in JS).
//!
//! `Cipheriv`/`Decipheriv` cover AES-256/128-GCM (AEAD) and AES-256/128-CBC
//! (PKCS#7). X25519 Diffie-Hellman covers Signal-protocol-style key exchange
//! (`generateX25519KeyPair`/`x25519PublicKey`/`diffieHellman`) — non-standard
//! names since RTS has no asymmetric KeyObject type to match Node's real
//! `generateKeyPairSync`/`createDiffieHellman` surface.
//!
//! Deferred (need an asymmetric KeyObject / streams / async backend):
//! `publicEncrypt`/`sign`/`verify` and the general KeyObject surface, the
//! WebCrypto `subtle` API, X.509.
//!
//! Layout: `algo` (digest/HMAC + encoding), `cipher` (AES-GCM/CBC math),
//! `cipher_instance` (the `Cipher` class), `hash` (the `Hash` class), `dh`
//! (X25519), `state` (byte-marshalling helpers), `random` (CSPRNG helpers),
//! `symbols` (free-function extern points), `mod` (registration).

mod algo;
mod cipher;
mod cipher_instance;
mod dh;
mod hash;
mod random;
mod state;
mod symbols;

use rts_engine::AbiType::{self, Handle};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

#[allow(clippy::too_many_arguments)]
fn m(name: &str, kind: MemberKind, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registers the `Hash`/`Cipher` classes + the `node:crypto` module.
pub fn register(e: &mut Engine) {
    use symbols as s;

    // `Hash`/`Hmac` and `Cipheriv`/`Decipheriv` are `#[rtse::class]` structs
    // now (see `hash.rs` / `cipher_instance.rs`) — the macro generates their
    // `register`, every ABI symbol, and the ctor-less factory allocation path.
    hash::register(e);
    cipher_instance::register(e);

    e.module("node:crypto", |mo| {
        mo.doc("Cryptography (node:crypto): createHash/createHmac, hash, randomBytes/randomUUID/randomInt, timingSafeEqual, getHashes.");
        mo.registry(s::create_hash_entry());
        mo.registry(s::create_hmac_entry());
        mo.registry(s::hash_entry());
        mo.registry(s::hash_enc_entry());
        mo.registry(s::random_bytes_entry());
        mo.registry(s::random_uuid_entry());
        mo.registry(s::random_int_max_entry());
        mo.registry(s::random_int_entry());
        mo.registry(s::random_fill_sync_entry());
        mo.registry(s::timing_safe_equal_entry());
        mo.registry(s::pbkdf2_sync_entry());
        mo.registry(s::scrypt_sync_entry());
        mo.registry(s::scrypt_sync_params_entry());
        mo.registry(s::hkdf_sync_entry());
        mo.registry(s::get_hashes_entry());
        mo.member(m("constants", MemberKind::Constant, vec![], Handle, "__RTS_FN_NODE_CRYPTO_CONSTANTS", "constants: object", s::__RTS_FN_NODE_CRYPTO_CONSTANTS as *const u8));
        mo.registry(s::create_cipheriv_entry());
        mo.registry(s::create_decipheriv_entry());
        mo.registry(s::generate_x25519_key_pair_entry());
        mo.registry(s::x25519_public_key_entry());
        mo.registry(s::x25519_diffie_hellman_entry());
    });
}
