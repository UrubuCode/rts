//! node:crypto — the entry points: the module functions (`createHash`/
//! `createHmac`/`hash`/`randomBytes`/`randomUUID`/`randomInt`/
//! `timingSafeEqual`/`getHashes`/…) as `#[rtse::function]` members.
//! `createHash`/`createHmac`/`createCipheriv`/`createDecipheriv` build a
//! `Hash`/`Cipher` instance (`#[rtse::class]`, see `hash.rs`/
//! `cipher_instance.rs`) via `alloc_rtse` directly — those classes are never
//! reached through `new Hash()`/`new Cipher()` in JS, only through these
//! factory functions.

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{alloc_entry, alloc_rtse, Entry};
use rts_engine::heap::shapes::string_word;

use super::algo::{self, Algo};
use super::cipher::CipherAlgo;
use super::cipher_instance::Cipher;
use super::dh;
use super::hash::Hash;
use super::random;
use super::state::{byte_array, read_bytes};

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

fn throw_unknown_algo(name: &str) {
    let msg = format!("Digest method not supported: {name}");
    unsafe { __rtsadp_throw_js_error(b"Error".as_ptr(), 5, msg.as_ptr(), msg.len() as i64) };
}

fn throw_error(kind: &str, msg: &str) {
    unsafe { __rtsadp_throw_js_error(kind.as_ptr(), kind.len() as i64, msg.as_ptr(), msg.len() as i64) };
}

/// `crypto.createHash(algorithm)`.
#[rtse::function(module = "node:crypto", value = "createHash", throws)]
fn create_hash(algorithm: &str) -> Handle {
    match Algo::parse(algorithm) {
        Some(a) => alloc_rtse("Hash", Hash::new(a, None)),
        None => {
            throw_unknown_algo(algorithm);
            0
        }
    }
}

/// `crypto.createHmac(algorithm, key)`.
#[rtse::function(module = "node:crypto", value = "createHmac", throws)]
fn create_hmac(algorithm: &str, key: Handle) -> Handle {
    match Algo::parse(algorithm) {
        Some(a) => alloc_rtse("Hash", Hash::new(a, Some(&read_bytes(key)))),
        None => {
            throw_unknown_algo(algorithm);
            0
        }
    }
}

fn hash_oneshot(name: &str, data: u64, enc: &str) -> String {
    match Algo::parse(name) {
        Some(a) => algo::encode(&algo::hash_bytes(a, &read_bytes(data)), enc),
        None => {
            throw_unknown_algo(name);
            String::new()
        }
    }
}

/// `crypto.hash(algorithm, data)` — single-shot, hex by default.
#[rtse::function(module = "node:crypto", value = "hash", throws)]
fn hash(algorithm: &str, data: Handle) -> String {
    hash_oneshot(algorithm, data, "hex")
}

/// `crypto.hash(algorithm, data, outputEncoding)`.
#[rtse::function(module = "node:crypto", value = "hash", overload = "enc", throws)]
fn hash_enc(algorithm: &str, data: Handle, encoding: &str) -> String {
    hash_oneshot(algorithm, data, encoding)
}

/// `crypto.randomBytes(size)`.
#[rtse::function(module = "node:crypto", value = "randomBytes", throws)]
fn random_bytes(size: i64) -> Handle {
    if size < 0 {
        let msg = "The value of \"size\" is out of range. It must be >= 0";
        unsafe { __rtsadp_throw_js_error(b"RangeError".as_ptr(), 10, msg.as_ptr(), msg.len() as i64) };
        return random::random_bytes(0);
    }
    random::random_bytes(size as usize)
}

/// `crypto.randomUUID()`.
#[rtse::function(module = "node:crypto", value = "randomUUID", throws)]
fn random_uuid() -> String {
    random::random_uuid().to_string()
}

/// A random int in `[min, max)`, throwing RangeError when `max <= min` (Node's
/// "The value of max is out of range" contract).
fn random_int_checked(min: i64, max: i64) -> i64 {
    if max <= min {
        let msg = "The value of \"max\" is out of range. It must be greater than the value of \"min\"";
        unsafe { __rtsadp_throw_js_error(b"RangeError".as_ptr(), 10, msg.as_ptr(), msg.len() as i64) };
        return min;
    }
    random::random_int(min, max)
}

/// `crypto.randomInt(max)`.
#[rtse::function(module = "node:crypto", value = "randomInt", throws)]
fn random_int_max(max: i64) -> i64 {
    random_int_checked(0, max)
}

/// `crypto.randomInt(min, max)`.
#[rtse::function(module = "node:crypto", value = "randomInt", overload = "minmax", throws)]
fn random_int(min: i64, max: i64) -> i64 {
    random_int_checked(min, max)
}

/// `crypto.randomFillSync(buffer)` → the same buffer, bytes overwritten.
#[rtse::function(module = "node:crypto", value = "randomFillSync", throws)]
fn random_fill_sync(buffer: Handle) -> Handle {
    random::random_fill(buffer)
}

/// `crypto.timingSafeEqual(a, b)` — throws RangeError on a length mismatch
/// (matching Node), else returns whether the inputs are byte-equal.
#[rtse::function(module = "node:crypto", value = "timingSafeEqual", throws)]
fn timing_safe_equal(a: Handle, b: Handle) -> bool {
    match random::timing_safe_equal(a, b) {
        Some(eq) => eq,
        None => {
            let msg = "Input buffers must have the same byte length";
            unsafe { __rtsadp_throw_js_error(b"RangeError".as_ptr(), 10, msg.as_ptr(), msg.len() as i64) };
            false
        }
    }
}

/// `crypto.pbkdf2Sync(password, salt, iterations, keylen, digest)` → Buffer.
#[rtse::function(module = "node:crypto", value = "pbkdf2Sync", throws)]
fn pbkdf2_sync(password: Handle, salt: Handle, iterations: i64, keylen: i64, digest: &str) -> Handle {
    match Algo::parse(digest) {
        Some(a) => {
            let dk = algo::pbkdf2(a, &read_bytes(password), &read_bytes(salt), iterations.max(0) as u32, keylen.max(0) as usize);
            byte_array(&dk)
        }
        None => {
            throw_unknown_algo(digest);
            byte_array(&[])
        }
    }
}

fn scrypt_impl(password: u64, salt: u64, keylen: i64, n: u32, r: u32, p: u32) -> u64 {
    match algo::scrypt(&read_bytes(password), &read_bytes(salt), n, r, p, keylen.max(0) as usize) {
        Ok(dk) => byte_array(&dk),
        Err(e) => {
            unsafe { __rtsadp_throw_js_error(b"Error".as_ptr(), 5, e.as_ptr(), e.len() as i64) };
            byte_array(&[])
        }
    }
}

/// `crypto.scryptSync(password, salt, keylen)` — default N=16384, r=8, p=1.
#[rtse::function(module = "node:crypto", value = "scryptSync", throws)]
fn scrypt_sync(password: Handle, salt: Handle, keylen: i64) -> Handle {
    scrypt_impl(password, salt, keylen, 16384, 8, 1)
}

/// `crypto.scryptSync(password, salt, keylen, N, r, p)`.
#[rtse::function(module = "node:crypto", value = "scryptSync", overload = "params", throws)]
fn scrypt_sync_params(password: Handle, salt: Handle, keylen: i64, n: i64, r: i64, p: i64) -> Handle {
    scrypt_impl(password, salt, keylen, n.max(1) as u32, r.max(1) as u32, p.max(1) as u32)
}

/// `crypto.hkdfSync(digest, ikm, salt, info, keylen)` → derived-key bytes.
#[rtse::function(module = "node:crypto", value = "hkdfSync", throws)]
fn hkdf_sync(digest: &str, ikm: Handle, salt: Handle, info: Handle, keylen: i64) -> Handle {
    match Algo::parse(digest) {
        Some(a) => match algo::hkdf(a, &read_bytes(ikm), &read_bytes(salt), &read_bytes(info), keylen.max(0) as usize) {
            Ok(dk) => byte_array(&dk),
            Err(e) => {
                unsafe { __rtsadp_throw_js_error(b"Error".as_ptr(), 5, e.as_ptr(), e.len() as i64) };
                byte_array(&[])
            }
        },
        None => {
            throw_unknown_algo(digest);
            byte_array(&[])
        }
    }
}

/// `crypto.getHashes()`.
#[rtse::function(module = "node:crypto", value = "getHashes", throws)]
fn get_hashes() -> Handle {
    let words: Vec<i64> = algo::hashes().iter().map(|s| string_word(s.as_bytes()) as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// `crypto.createCipheriv(algorithm, key, iv)`.
#[rtse::function(module = "node:crypto", value = "createCipheriv", throws)]
fn create_cipheriv(algorithm: &str, key: Handle, iv: Handle) -> Handle {
    match CipherAlgo::parse(algorithm) {
        Some(a) => alloc_rtse("Cipher", Cipher::new(a, &read_bytes(key), &read_bytes(iv), false)),
        None => {
            throw_error("Error", &format!("Unknown cipher: {algorithm}"));
            0
        }
    }
}

/// `crypto.createDecipheriv(algorithm, key, iv)`.
#[rtse::function(module = "node:crypto", value = "createDecipheriv", throws)]
fn create_decipheriv(algorithm: &str, key: Handle, iv: Handle) -> Handle {
    match CipherAlgo::parse(algorithm) {
        Some(a) => alloc_rtse("Cipher", Cipher::new(a, &read_bytes(key), &read_bytes(iv), true)),
        None => {
            throw_error("Error", &format!("Unknown cipher: {algorithm}"));
            0
        }
    }
}

/// `crypto.generateX25519KeyPair()` → `{ privateKey, publicKey }` (both
/// 32-byte Buffers). Non-standard-named helper (Node's real
/// `generateKeyPairSync("x25519", ...)` returns KeyObjects RTS doesn't model);
/// exposed directly since Signal-protocol code only needs the raw bytes.
#[rtse::function(module = "node:crypto", value = "generateX25519KeyPair", throws)]
fn generate_x25519_key_pair() -> Handle {
    let (private, public) = dh::generate_keypair();
    let pk = rts_engine::heap::shapes::handle_word_auto(byte_array(&private)) as i64;
    let pub_k = rts_engine::heap::shapes::handle_word_auto(byte_array(&public)) as i64;
    rts_engine::heap::shapes::alloc_shaped_object(&["privateKey", "publicKey"], &[pk, pub_k])
}

/// `crypto.x25519PublicKey(privateKey)` → 32-byte Buffer.
#[rtse::function(module = "node:crypto", value = "x25519PublicKey", throws)]
fn x25519_public_key(private_key: Handle) -> Handle {
    match dh::public_from_private(&read_bytes(private_key)) {
        Ok(pk) => byte_array(&pk),
        Err(e) => {
            throw_error("Error", &e);
            byte_array(&[])
        }
    }
}

/// `crypto.diffieHellman({ privateKey, publicKey })` — X25519 shared secret,
/// mirrors Node's two-KeyObject form but takes raw-byte Buffers/handles
/// directly since RTS has no asymmetric KeyObject type.
#[rtse::function(module = "node:crypto", value = "x25519DiffieHellman", throws)]
fn x25519_diffie_hellman(private_key: Handle, public_key: Handle) -> Handle {
    match dh::diffie_hellman(&read_bytes(private_key), &read_bytes(public_key)) {
        Ok(secret) => byte_array(&secret),
        Err(e) => {
            throw_error("Error", &e);
            byte_array(&[])
        }
    }
}

/// `crypto.constants` — the RSA-padding and point-conversion constants (the
/// OpenSSL values, field-accessible via `crypto.constants.RSA_PKCS1_OAEP_PADDING`).
/// Hand-written: `#[rtse::function]` only emits `MemberKind::Function`, not
/// `MemberKind::Constant`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_CONSTANTS() -> u64 {
    let num = |v: f64| v.to_bits() as i64;
    rts_engine::heap::shapes::alloc_shaped_object(
        &[
            "RSA_PKCS1_PADDING",
            "RSA_NO_PADDING",
            "RSA_PKCS1_OAEP_PADDING",
            "RSA_X931_PADDING",
            "RSA_PKCS1_PSS_PADDING",
            "RSA_PSS_SALTLEN_DIGEST",
            "RSA_PSS_SALTLEN_MAX_SIGN",
            "RSA_PSS_SALTLEN_AUTO",
            "POINT_CONVERSION_COMPRESSED",
            "POINT_CONVERSION_UNCOMPRESSED",
            "POINT_CONVERSION_HYBRID",
        ],
        &[num(1.0), num(3.0), num(4.0), num(5.0), num(6.0), num(-1.0), num(-2.0), num(-2.0), num(2.0), num(4.0), num(6.0)],
    )
}

// Hash/Hmac and Cipheriv/Decipheriv instance methods now live on their
// `#[rtse::class]` structs — see `hash.rs` / `cipher_instance.rs`.
