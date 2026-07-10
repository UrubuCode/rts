//! node:querystring — base extern "C" symbol implementations (the sync surface).
//!
//! ABI mirrors the pure-namespace shape used across RTS: `Str` args arrive as
//! `(ptr, len)` and are rebuilt via `from_abi` (returns `None` on null / invalid
//! UTF-8); string results are interned to GC string handles. Symbols follow the
//! rts-node convention `__RTS_FN_NODE_QUERYSTRING_*`.

use rts_engine::abi::str_abi::from_abi;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Interns a Rust string as a GC string handle (the ABI `Handle` return).
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Bytes `querystring.escape` leaves unescaped — the `encodeURIComponent`
/// unreserved set: ALPHA / DIGIT / `-` `_` `.` `!` `~` `*` `'` `(` `)`.
fn is_unescaped(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
        )
}

fn hex_upper(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        _ => b'A' + (nibble - 10),
    }
}

/// `querystring.escape(str)` — percent-encodes per `encodeURIComponent`
/// semantics (uppercase hex), used by `querystring.stringify`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_ESCAPE(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    for &b in s.as_bytes() {
        if is_unescaped(b) {
            out.push(b);
        } else {
            out.push(b'%');
            out.push(hex_upper(b >> 4));
            out.push(hex_upper(b & 0x0F));
        }
    }
    // `out` is ASCII by construction, so this is always valid UTF-8.
    intern(&String::from_utf8_lossy(&out))
}

/// `querystring.unescape(str)` — percent-decodes, tolerant of malformed `%`
/// sequences (an incomplete/invalid `%XX` is left literal, matching Node's
/// `unescape` fallback). Does NOT convert `+` to space (that lives in `parse`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_UNESCAPE(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push(((hi << 4) | lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    intern(&String::from_utf8_lossy(&out))
}
