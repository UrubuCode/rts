//! `node:crypto` — the self-contained hashing / HMAC / random surface. Real
//! cryptographic primitives via RustCrypto (`md-5`/`sha1`/`sha2` over
//! `DynDigest`) + OS entropy (`getrandom`). No stubs, no fabricated output.
//!
//! Surface: `createHash`/`createHmac` (returning a `Hash` instance with
//! `update`/`digest`), the single-shot `crypto.hash`, `randomBytes`,
//! `randomUUID`, `randomInt`, `timingSafeEqual`, `getHashes`.
//!
//! `Hash`/`Hmac` are one object-backed Registry class (`__rts_class = "Hash"`,
//! same model as `StringDecoder`): `createHash`/`createHmac` build the instance,
//! its `ts_signature` return type `Hash` lets method dispatch resolve `update`/
//! `digest`. HMAC is a flag on the instance.
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
//! Layout: `algo` (digest/HMAC + encoding), `cipher` (AES-GCM/CBC), `dh`
//! (X25519), `state` (instance objects), `random` (CSPRNG helpers), `symbols`
//! (extern points), `mod` (registration).

mod algo;
mod cipher;
mod dh;
mod random;
mod state;
mod symbols;

use rts_engine::AbiType::{self, Handle, StrPtr};
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

/// Registers the `Hash` class + the `node:crypto` module.
pub fn register(e: &mut Engine) {
    use symbols as s;
    use MemberKind::InstanceMethod;

    // The Hash/Hmac object-backed class: update (chainable) + digest.
    e.class("Hash")
        .doc("Hash/Hmac — incremental digest object (node:crypto).")
        .member(m("update", InstanceMethod, vec![Handle, Handle], Handle, "__RTS_FN_NODE_CRYPTO_UPDATE", "update(data: object): Hash", s::__RTS_FN_NODE_CRYPTO_UPDATE as *const u8))
        .member(m("update", InstanceMethod, vec![Handle, StrPtr, StrPtr], Handle, "__RTS_FN_NODE_CRYPTO_UPDATE_ENC", "update(data: string, inputEncoding: string): Hash", s::__RTS_FN_NODE_CRYPTO_UPDATE_ENC as *const u8))
        .member(m("copy", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NODE_CRYPTO_COPY", "copy(): Hash", s::__RTS_FN_NODE_CRYPTO_COPY as *const u8))
        .member(m("digest", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NODE_CRYPTO_DIGEST", "digest(): number[]", s::__RTS_FN_NODE_CRYPTO_DIGEST as *const u8))
        .member(m("digest", InstanceMethod, vec![Handle, StrPtr], Handle, "__RTS_FN_NODE_CRYPTO_DIGEST_ENC", "digest(encoding: string): string", s::__RTS_FN_NODE_CRYPTO_DIGEST_ENC as *const u8))
        .done();

    // The Cipher/Decipher object-backed class. Unlike real Node streaming,
    // `update` only accumulates (returns an empty Buffer); the full output
    // comes from `final()` alone — GCM needs the whole message before it can
    // authenticate, so there is no correct per-call partial output.
    e.class("Cipher")
        .doc("Cipheriv/Decipheriv — AES-GCM/CBC symmetric cipher object (node:crypto). update() only accumulates; read the full result from final().")
        .member(m("update", InstanceMethod, vec![Handle, Handle], Handle, "__RTS_FN_NODE_CRYPTO_CIPHER_UPDATE", "update(data: object): number[]", s::__RTS_FN_NODE_CRYPTO_CIPHER_UPDATE as *const u8))
        .member(m("setAAD", InstanceMethod, vec![Handle, Handle], Handle, "__RTS_FN_NODE_CRYPTO_CIPHER_SET_AAD", "setAAD(buffer: object): Cipher", s::__RTS_FN_NODE_CRYPTO_CIPHER_SET_AAD as *const u8))
        .member(m("setAuthTag", InstanceMethod, vec![Handle, Handle], Handle, "__RTS_FN_NODE_CRYPTO_CIPHER_SET_AUTH_TAG", "setAuthTag(buffer: object): Cipher", s::__RTS_FN_NODE_CRYPTO_CIPHER_SET_AUTH_TAG as *const u8))
        .member(m("getAuthTag", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NODE_CRYPTO_CIPHER_GET_AUTH_TAG", "getAuthTag(): number[]", s::__RTS_FN_NODE_CRYPTO_CIPHER_GET_AUTH_TAG as *const u8))
        .member({
            let mut mem = m("final", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NODE_CRYPTO_CIPHER_FINAL", "final(): number[]", s::__RTS_FN_NODE_CRYPTO_CIPHER_FINAL as *const u8);
            mem.flags = MemberFlags::THROWS;
            mem
        })
        .done();

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
