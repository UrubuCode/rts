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

use rts_engine::AbiType::{self, Bool, Handle, I64, StrPtr};
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
        pure: false,
        intrinsic: None,
        emit: None,
    }
}

/// A module function that can throw (unknown-algorithm errors) → flagged
/// `MemberFlags::THROWS` so the engine routes its pending-error slot to an
/// enclosing `try/catch` (registry_call.rs).
fn func(name: &str, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    let mut member = m(name, MemberKind::Function, args, ret, symbol, ts, fp);
    member.flags = MemberFlags::THROWS;
    member
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

    e.ns("node:crypto")
        .doc("Cryptography (node:crypto): createHash/createHmac, hash, randomBytes/randomUUID/randomInt, timingSafeEqual, getHashes.")
        .member(func("createHash", vec![StrPtr], Handle, "__RTS_FN_NODE_CRYPTO_CREATE_HASH", "createHash(algorithm: string): Hash", s::__RTS_FN_NODE_CRYPTO_CREATE_HASH as *const u8))
        .member(func("createHmac", vec![StrPtr, Handle], Handle, "__RTS_FN_NODE_CRYPTO_CREATE_HMAC", "createHmac(algorithm: string, key: object): Hash", s::__RTS_FN_NODE_CRYPTO_CREATE_HMAC as *const u8))
        .member(func("hash", vec![StrPtr, Handle], Handle, "__RTS_FN_NODE_CRYPTO_HASH", "hash(algorithm: string, data: object): string", s::__RTS_FN_NODE_CRYPTO_HASH as *const u8))
        .member(func("hash", vec![StrPtr, Handle, StrPtr], Handle, "__RTS_FN_NODE_CRYPTO_HASH_ENC", "hash(algorithm: string, data: object, encoding: string): string", s::__RTS_FN_NODE_CRYPTO_HASH_ENC as *const u8))
        .member(func("randomBytes", vec![I64], Handle, "__RTS_FN_NODE_CRYPTO_RANDOM_BYTES", "randomBytes(size: number): number[]", s::__RTS_FN_NODE_CRYPTO_RANDOM_BYTES as *const u8))
        .member(func("randomUUID", vec![], Handle, "__RTS_FN_NODE_CRYPTO_RANDOM_UUID", "randomUUID(): string", s::__RTS_FN_NODE_CRYPTO_RANDOM_UUID as *const u8))
        .member(func("randomInt", vec![I64], I64, "__RTS_FN_NODE_CRYPTO_RANDOM_INT_MAX", "randomInt(max: number): number", s::__RTS_FN_NODE_CRYPTO_RANDOM_INT_MAX as *const u8))
        .member(func("randomInt", vec![I64, I64], I64, "__RTS_FN_NODE_CRYPTO_RANDOM_INT", "randomInt(min: number, max: number): number", s::__RTS_FN_NODE_CRYPTO_RANDOM_INT as *const u8))
        .member(func("randomFillSync", vec![Handle], Handle, "__RTS_FN_NODE_CRYPTO_RANDOM_FILL_SYNC", "randomFillSync(buffer: number[]): number[]", s::__RTS_FN_NODE_CRYPTO_RANDOM_FILL_SYNC as *const u8))
        .member(func("timingSafeEqual", vec![Handle, Handle], Bool, "__RTS_FN_NODE_CRYPTO_TIMING_SAFE_EQUAL", "timingSafeEqual(a: object, b: object): boolean", s::__RTS_FN_NODE_CRYPTO_TIMING_SAFE_EQUAL as *const u8))
        .member(func("pbkdf2Sync", vec![Handle, Handle, I64, I64, StrPtr], Handle, "__RTS_FN_NODE_CRYPTO_PBKDF2", "pbkdf2Sync(password: object, salt: object, iterations: number, keylen: number, digest: string): number[]", s::__RTS_FN_NODE_CRYPTO_PBKDF2 as *const u8))
        .member(func("scryptSync", vec![Handle, Handle, I64], Handle, "__RTS_FN_NODE_CRYPTO_SCRYPT", "scryptSync(password: object, salt: object, keylen: number): number[]", s::__RTS_FN_NODE_CRYPTO_SCRYPT as *const u8))
        .member(func("scryptSync", vec![Handle, Handle, I64, I64, I64, I64], Handle, "__RTS_FN_NODE_CRYPTO_SCRYPT_PARAMS", "scryptSync(password: object, salt: object, keylen: number, N: number, r: number, p: number): number[]", s::__RTS_FN_NODE_CRYPTO_SCRYPT_PARAMS as *const u8))
        .member(func("hkdfSync", vec![StrPtr, Handle, Handle, Handle, I64], Handle, "__RTS_FN_NODE_CRYPTO_HKDF", "hkdfSync(digest: string, ikm: object, salt: object, info: object, keylen: number): number[]", s::__RTS_FN_NODE_CRYPTO_HKDF as *const u8))
        .member(func("getHashes", vec![], Handle, "__RTS_FN_NODE_CRYPTO_GET_HASHES", "getHashes(): string[]", s::__RTS_FN_NODE_CRYPTO_GET_HASHES as *const u8))
        .member(m("constants", MemberKind::Constant, vec![], Handle, "__RTS_FN_NODE_CRYPTO_CONSTANTS", "constants: object", s::__RTS_FN_NODE_CRYPTO_CONSTANTS as *const u8))
        .member(func("createCipheriv", vec![StrPtr, Handle, Handle], Handle, "__RTS_FN_NODE_CRYPTO_CREATE_CIPHERIV", "createCipheriv(algorithm: string, key: object, iv: object): Cipher", s::__RTS_FN_NODE_CRYPTO_CREATE_CIPHERIV as *const u8))
        .member(func("createDecipheriv", vec![StrPtr, Handle, Handle], Handle, "__RTS_FN_NODE_CRYPTO_CREATE_DECIPHERIV", "createDecipheriv(algorithm: string, key: object, iv: object): Cipher", s::__RTS_FN_NODE_CRYPTO_CREATE_DECIPHERIV as *const u8))
        .member(func("generateX25519KeyPair", vec![], Handle, "__RTS_FN_NODE_CRYPTO_X25519_GENERATE_KEYPAIR", "generateX25519KeyPair(): { privateKey: number[]; publicKey: number[] }", s::__RTS_FN_NODE_CRYPTO_X25519_GENERATE_KEYPAIR as *const u8))
        .member(func("x25519PublicKey", vec![Handle], Handle, "__RTS_FN_NODE_CRYPTO_X25519_PUBLIC_KEY", "x25519PublicKey(privateKey: object): number[]", s::__RTS_FN_NODE_CRYPTO_X25519_PUBLIC_KEY as *const u8))
        .member(func("x25519DiffieHellman", vec![Handle, Handle], Handle, "__RTS_FN_NODE_CRYPTO_X25519_DIFFIE_HELLMAN", "x25519DiffieHellman(privateKey: object, publicKey: object): number[]", s::__RTS_FN_NODE_CRYPTO_X25519_DIFFIE_HELLMAN as *const u8))
        .done();
}
