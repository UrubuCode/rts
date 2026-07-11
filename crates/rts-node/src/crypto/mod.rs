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
//! Deferred (need a cipher/KDF/keypair backend, streams, or async): the
//! `Cipheriv`/`Decipheriv` symmetric ciphers, `publicEncrypt`/`sign`/`verify`
//! and the KeyObject/asymmetric surface, `pbkdf2`/`scrypt` KDFs, `createDiffie
//! Hellman`, the WebCrypto `subtle` API, X.509. This is the hashing+random core.
//!
//! Layout: `algo` (digest/HMAC + encoding), `state` (instance object), `random`
//! (CSPRNG helpers), `symbols` (extern points), `mod` (registration).

mod algo;
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
        .member(m("digest", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NODE_CRYPTO_DIGEST", "digest(): number[]", s::__RTS_FN_NODE_CRYPTO_DIGEST as *const u8))
        .member(m("digest", InstanceMethod, vec![Handle, StrPtr], Handle, "__RTS_FN_NODE_CRYPTO_DIGEST_ENC", "digest(encoding: string): string", s::__RTS_FN_NODE_CRYPTO_DIGEST_ENC as *const u8))
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
        .done();
}
