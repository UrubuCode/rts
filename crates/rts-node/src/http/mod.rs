//! `node:http` — the stream/server-independent surface: `METHODS()` (the HTTP
//! method list), `STATUS_CODES()` (the code→reason registry object),
//! `validateHeaderName(name)` / `validateHeaderValue(name, value)` (RFC 7230
//! validation, throwing on an invalid token / field-value). Real data + real
//! checks.
//!
//! Deferred (need the event-loop / socket / stream subsystems): `createServer` +
//! the `Server`/`IncomingMessage`/`ServerResponse` classes, `request`/`get` +
//! `ClientRequest`, `Agent`/`globalAgent`, the `OutgoingMessage` writable
//! surface — the whole networked request/response machinery.
//!
//! Layout: `data` (the tables + validators), `mod` (registration).

mod data;

use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::{alloc_shaped_object, string_word};
use rts_engine::AbiType::{self, Handle, StrPtr, Void};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }.unwrap_or("").to_string()
}

fn throw_type(message: &str) {
    unsafe { __rtsadp_throw_js_error(b"TypeError".as_ptr(), 9, message.as_ptr(), message.len() as i64) };
}

/// `http.METHODS`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_HTTP_METHODS() -> u64 {
    let words: Vec<i64> = data::METHODS.iter().map(|s| string_word(s.as_bytes()) as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// `http.STATUS_CODES` — `{ "200": "OK", … }`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_HTTP_STATUS_CODES() -> u64 {
    let keys: Vec<&str> = data::STATUS_CODES.iter().map(|(k, _)| *k).collect();
    let values: Vec<i64> = data::STATUS_CODES.iter().map(|(_, v)| string_word(v.as_bytes()) as i64).collect();
    alloc_shaped_object(&keys, &values)
}

/// `http.validateHeaderName(name)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_HTTP_VALIDATE_NAME(p: *const u8, l: i64) {
    let name = read(p, l);
    if !data::valid_header_name(&name) {
        throw_type(&format!("Header name must be a valid HTTP token [\"{name}\"]"));
    }
}

/// `http.validateHeaderValue(name, value)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_HTTP_VALIDATE_VALUE(np: *const u8, nl: i64, vp: *const u8, vl: i64) {
    let name = read(np, nl);
    let value = read(vp, vl);
    if !data::valid_header_value(&value) {
        throw_type(&format!("Invalid character in header content [\"{name}\"]"));
    }
}

fn func(name: &str, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8, throws: bool) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: if throws { MemberFlags::THROWS } else { MemberFlags::NONE },
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        intrinsic: None,
    }
}

/// Registers the `node:http` surface.
pub fn register(e: &mut Engine) {
    e.ns("node:http")
        .doc("HTTP (node:http): METHODS, STATUS_CODES, validateHeaderName, validateHeaderValue.")
        .member(func("METHODS", vec![], Handle, "__RTS_FN_NODE_HTTP_METHODS", "METHODS(): string[]", __RTS_FN_NODE_HTTP_METHODS as *const u8, false))
        .member(func("STATUS_CODES", vec![], Handle, "__RTS_FN_NODE_HTTP_STATUS_CODES", "STATUS_CODES(): object", __RTS_FN_NODE_HTTP_STATUS_CODES as *const u8, false))
        .member(func("validateHeaderName", vec![StrPtr], Void, "__RTS_FN_NODE_HTTP_VALIDATE_NAME", "validateHeaderName(name: string): void", __RTS_FN_NODE_HTTP_VALIDATE_NAME as *const u8, true))
        .member(func("validateHeaderValue", vec![StrPtr, StrPtr], Void, "__RTS_FN_NODE_HTTP_VALIDATE_VALUE", "validateHeaderValue(name: string, value: string): void", __RTS_FN_NODE_HTTP_VALIDATE_VALUE as *const u8, true))
        .done();
}
