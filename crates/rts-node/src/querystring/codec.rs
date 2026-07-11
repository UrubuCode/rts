//! node:querystring — the percent-encoding codec (`escape`/`unescape`) and the
//! shared helpers `parse`/`stringify` build on.
//!
//! `escape` matches `encodeURIComponent` (RFC 3986 unreserved set kept, every
//! other byte percent-encoded from its UTF-8 bytes). `unescape` is lenient
//! (never throws, matching Node's `querystring.unescape` fallback): a malformed
//! `%XX` is left literal. `parse` decodes with `+`→space; `unescape` does not.

use super::words::intern;

/// `encodeURIComponent`-style: keep `A-Za-z0-9` and `-_.!~*'()`, percent-encode
/// every other byte of the UTF-8 encoding as uppercase `%XX`.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex_upper(b >> 4));
            out.push(hex_upper(b & 0x0f));
        }
    }
    out
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')')
}

fn hex_upper(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'A' + (nibble - 10)) as char,
    }
}

/// Percent-decode `s`. When `plus_to_space`, a literal `+` becomes a space
/// (`parse` semantics); `querystring.unescape` passes `false`. Lenient: an
/// incomplete/invalid `%XX` is kept literal (never throws).
pub fn unescape(s: &str, plus_to_space: bool) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                    (Some(hi), Some(lo)) => {
                        out.push((hi << 4) | lo);
                        i += 3;
                    }
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' if plus_to_space => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// `querystring.escape(str)` — percent-encode.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_ESCAPE(ptr: *const u8, len: i64) -> u64 {
    intern(&escape(read_str(ptr, len)))
}

/// `querystring.unescape(str)` — percent-decode (no `+`→space).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_UNESCAPE(ptr: *const u8, len: i64) -> u64 {
    intern(&unescape(read_str(ptr, len), false))
}

/// Decode an ABI `(ptr, len)` string arg to `&str` (empty on null/invalid).
pub fn read_str<'a>(ptr: *const u8, len: i64) -> &'a str {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }.unwrap_or("")
}
