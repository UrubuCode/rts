//! node:crypto — the `extern "C"` entry points: the module functions
//! (`createHash`/`createHmac`/`hash`/`randomBytes`/`randomUUID`/`randomInt`/
//! `timingSafeEqual`/`getHashes`) and the `Hash` class methods (`update`/
//! `digest`).

use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::string_word;

use super::algo::{self, Algo};
use super::random;
use super::state::{append, build_instance, byte_array, finalize_state, read_bytes};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }.unwrap_or("").to_string()
}

fn throw_unknown_algo(name: &str) {
    let msg = format!("Digest method not supported: {name}");
    unsafe { __rtsadp_throw_js_error(b"Error".as_ptr(), 5, msg.as_ptr(), msg.len() as i64) };
}

/// `crypto.createHash(algorithm)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_CREATE_HASH(p: *const u8, l: i64) -> u64 {
    let name = read(p, l);
    match Algo::parse(&name) {
        Some(a) => build_instance(a, None),
        None => {
            throw_unknown_algo(&name);
            0
        }
    }
}

/// `crypto.createHmac(algorithm, key)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_CREATE_HMAC(p: *const u8, l: i64, key: u64) -> u64 {
    let name = read(p, l);
    match Algo::parse(&name) {
        Some(a) => build_instance(a, Some(&read_bytes(key))),
        None => {
            throw_unknown_algo(&name);
            0
        }
    }
}

/// `crypto.hash(algorithm, data)` — single-shot, hex by default.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_HASH(p: *const u8, l: i64, data: u64) -> u64 {
    hash_oneshot(&read(p, l), data, "hex")
}

/// `crypto.hash(algorithm, data, outputEncoding)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_HASH_ENC(p: *const u8, l: i64, data: u64, ep: *const u8, el: i64) -> u64 {
    hash_oneshot(&read(p, l), data, &read(ep, el))
}

fn hash_oneshot(name: &str, data: u64, enc: &str) -> u64 {
    match Algo::parse(name) {
        Some(a) => intern(&algo::encode(&algo::hash_bytes(a, &read_bytes(data)), enc)),
        None => {
            throw_unknown_algo(name);
            intern("")
        }
    }
}

/// `crypto.randomBytes(size)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_RANDOM_BYTES(size: i64) -> u64 {
    random::random_bytes(size.max(0) as usize)
}

/// `crypto.randomUUID()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_RANDOM_UUID() -> u64 {
    intern(&random::random_uuid())
}

/// `crypto.randomInt(max)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_RANDOM_INT_MAX(max: i64) -> i64 {
    random::random_int(0, max)
}

/// `crypto.randomInt(min, max)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_RANDOM_INT(min: i64, max: i64) -> i64 {
    random::random_int(min, max)
}

/// `crypto.timingSafeEqual(a, b)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_TIMING_SAFE_EQUAL(a: u64, b: u64) -> i64 {
    random::timing_safe_equal(a, b) as i64
}

/// `crypto.pbkdf2Sync(password, salt, iterations, keylen, digest)` → Buffer.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_PBKDF2(
    password: u64,
    salt: u64,
    iterations: i64,
    keylen: i64,
    dp: *const u8,
    dl: i64,
) -> u64 {
    let name = read(dp, dl);
    match Algo::parse(&name) {
        Some(a) => {
            let dk = algo::pbkdf2(a, &read_bytes(password), &read_bytes(salt), iterations.max(0) as u32, keylen.max(0) as usize);
            byte_array(&dk)
        }
        None => {
            throw_unknown_algo(&name);
            byte_array(&[])
        }
    }
}

/// `crypto.scryptSync(password, salt, keylen)` — default N=16384, r=8, p=1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_SCRYPT(password: u64, salt: u64, keylen: i64) -> u64 {
    scrypt_impl(password, salt, keylen, 16384, 8, 1)
}

/// `crypto.scryptSync(password, salt, keylen, N, r, p)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_SCRYPT_PARAMS(password: u64, salt: u64, keylen: i64, n: i64, r: i64, p: i64) -> u64 {
    scrypt_impl(password, salt, keylen, n.max(1) as u32, r.max(1) as u32, p.max(1) as u32)
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

/// `crypto.getHashes()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_GET_HASHES() -> u64 {
    let words: Vec<i64> = algo::hashes().iter().map(|s| string_word(s.as_bytes()) as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

// ---- Hash / Hmac instance methods ----

/// `hash.update(data)` — appends, returns `this` (chainable).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_UPDATE(this: u64, data: u64) -> u64 {
    append(this, &read_bytes(data));
    this
}

/// `hash.digest()` → Buffer (Uint8Array-shaped) of the raw digest bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_DIGEST(this: u64) -> u64 {
    byte_array(&digest_bytes(this))
}

/// `hash.digest(encoding)` → encoded string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_DIGEST_ENC(this: u64, ep: *const u8, el: i64) -> u64 {
    intern(&algo::encode(&digest_bytes(this), &read(ep, el)))
}

fn digest_bytes(this: u64) -> Vec<u8> {
    match finalize_state(this) {
        Some((a, is_hmac, input, key)) => {
            if is_hmac {
                algo::hmac_bytes(a, &key, &input)
            } else {
                algo::hash_bytes(a, &input)
            }
        }
        None => Vec::new(),
    }
}
