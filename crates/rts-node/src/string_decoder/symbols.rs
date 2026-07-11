//! node:string_decoder — the class `extern "C"` entry points: constructor
//! (default `utf8` / explicit encoding, throwing `TypeError` on an unknown
//! encoding), the incremental `write`/`end` methods, and the `encoding` getter.

use rts_engine::heap::handles::{with_entry, Entry};

use super::decoder::Enc;
use super::state::{build_instance, load, read_bytes, store};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }
        .unwrap_or("")
        .to_string()
}

/// `new StringDecoder()` — defaults to `utf8`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_STRING_DECODER_NEW() -> u64 {
    build_instance(Enc::Utf8)
}

/// `new StringDecoder(encoding)` — throws `TypeError` (`ERR_UNKNOWN_ENCODING`)
/// when `encoding` is not recognized.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_STRING_DECODER_NEW_ENC(ptr: *const u8, len: i64) -> u64 {
    let name = read(ptr, len);
    match Enc::parse(&name) {
        Some(enc) => build_instance(enc),
        None => {
            let msg = format!("Unknown encoding: {name}");
            unsafe {
                __rtsadp_throw_js_error(
                    b"TypeError".as_ptr(),
                    9,
                    msg.as_ptr(),
                    msg.len() as i64,
                );
            }
            build_instance(Enc::Utf8)
        }
    }
}

/// `decoder.write(buffer)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_STRING_DECODER_WRITE(this: u64, input: u64) -> u64 {
    let mut d = match load(this) {
        Some(d) => d,
        None => return intern(""),
    };
    let bytes = read_bytes(input);
    let out = d.write(&bytes);
    store(this, &d);
    intern(&out)
}

/// `decoder.end()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_STRING_DECODER_END(this: u64) -> u64 {
    let mut d = match load(this) {
        Some(d) => d,
        None => return intern(""),
    };
    let out = d.end(None);
    store(this, &d);
    intern(&out)
}

/// `decoder.end(buffer)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_STRING_DECODER_END_BUF(this: u64, input: u64) -> u64 {
    let mut d = match load(this) {
        Some(d) => d,
        None => return intern(""),
    };
    let bytes = read_bytes(input);
    let out = d.end(Some(&bytes));
    store(this, &d);
    intern(&out)
}

/// `decoder.encoding` (getter) — the stored canonical encoding-name handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_STRING_DECODER_ENCODING(this: u64) -> u64 {
    with_entry(this, |e| match e {
        Some(Entry::Map(m)) => m.get("encoding").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}
