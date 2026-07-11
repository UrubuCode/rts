//! node:punycode — the `extern "C"` entry points. `encode`/`decode`/`toASCII`/
//! `toUnicode` use the `StrPtr` ABI and throw `RangeError` on malformed input;
//! `version` is a constant getter; `ucs2` is a constant returning a real object
//! whose `decode`/`encode` are callable `Entry::Function` values backed by the
//! word-ABI natives below.

use rts_engine::heap::shapes::alloc_shaped_object;

use super::core;
use super::words::{
    array_word, fn_value, intern, num_word, str_word, throw, word_number_array, word_string,
};

/// Node's bundled Punycode.js version.
const VERSION: &str = "2.3.1";

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }
        .unwrap_or("")
        .to_string()
}

/// `punycode.encode(string)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PUNYCODE_ENCODE(p: *const u8, l: i64) -> u64 {
    match core::encode_label(&read(p, l)) {
        Ok(s) => intern(&s),
        Err(e) => {
            throw("RangeError", e.0);
            intern("")
        }
    }
}

/// `punycode.decode(string)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PUNYCODE_DECODE(p: *const u8, l: i64) -> u64 {
    match core::decode_label(&read(p, l)) {
        Ok(s) => intern(&s),
        Err(e) => {
            throw("RangeError", e.0);
            intern("")
        }
    }
}

/// `punycode.toASCII(domain)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PUNYCODE_TOASCII(p: *const u8, l: i64) -> u64 {
    match core::to_ascii(&read(p, l)) {
        Ok(s) => intern(&s),
        Err(e) => {
            throw("RangeError", e.0);
            intern("")
        }
    }
}

/// `punycode.toUnicode(domain)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PUNYCODE_TOUNICODE(p: *const u8, l: i64) -> u64 {
    match core::to_unicode(&read(p, l)) {
        Ok(s) => intern(&s),
        Err(e) => {
            throw("RangeError", e.0);
            intern("")
        }
    }
}

/// `punycode.version` (constant getter).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PUNYCODE_VERSION() -> u64 {
    intern(VERSION)
}

/// `punycode.ucs2.decode(string)` — UNIFORM-thunk word ABI
/// (`(env, a0, a1, a2, a3, rest) -> word`): the string value word `a0` → array
/// of code-point numbers (Rust `chars()` already yields combined code points).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PUNYCODE_UCS2_DECODE(
    _env: u64,
    a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
    _rest: u64,
) -> u64 {
    let s = word_string(a0).unwrap_or_default();
    let words: Vec<i64> = s.chars().map(|c| num_word(c as u32 as f64)).collect();
    array_word(words) as u64
}

/// `punycode.ucs2.encode(codePoints)` — UNIFORM-thunk word ABI: number array
/// word `a0` → string (astral code points become surrogate pairs). Throws
/// `RangeError` on an out-of-range / non-integer code point.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PUNYCODE_UCS2_ENCODE(
    _env: u64,
    a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
    _rest: u64,
) -> u64 {
    let nums = match word_number_array(a0) {
        Some(v) => v,
        None => return str_word("") as u64,
    };
    let mut out = String::new();
    for n in nums {
        if n.fract() != 0.0 || n < 0.0 || n > 0x10FFFF as f64 {
            throw("RangeError", &format!("Invalid code point: {}", n as i64));
            return str_word("") as u64;
        }
        match char::from_u32(n as u32) {
            Some(c) => out.push(c),
            None => {
                throw("RangeError", &format!("Invalid code point: {}", n as i64));
                return str_word("") as u64;
            }
        }
    }
    str_word(&out) as u64
}

/// `punycode.ucs2` (constant getter) — a real object of two callable functions.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PUNYCODE_UCS2() -> u64 {
    let keys: &[&str] = &["decode", "encode"];
    let values: [i64; 2] = [
        fn_value(__RTS_FN_NODE_PUNYCODE_UCS2_DECODE as *const u8, "decode", 1),
        fn_value(__RTS_FN_NODE_PUNYCODE_UCS2_ENCODE as *const u8, "encode", 1),
    ];
    alloc_shaped_object(keys, &values)
}
