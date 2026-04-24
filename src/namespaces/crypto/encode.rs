//! Encoders — base64 e hex. Impl pura, sem deps.

use super::super::gc::handles::{table, Entry};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

// ── Hex ──────────────────────────────────────────────────────────────

const HEX_ALPHA: &[u8; 16] = b"0123456789abcdef";

/// Hex encode de bytes em ptr+len. Retorna string handle lowercase.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_HEX_ENCODE(ptr: i64, len: i64) -> u64 {
    if ptr == 0 || len < 0 {
        return 0;
    }
    // SAFETY: caller contract.
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX_ALPHA[((b >> 4) & 0xF) as usize] as char);
        out.push(HEX_ALPHA[(b & 0xF) as usize] as char);
    }
    intern(&out)
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Hex decode. Retorna handle de buffer, 0 em erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_HEX_DECODE(ptr: *const u8, len: i64) -> u64 {
    if ptr.is_null() || len < 0 || len % 2 != 0 {
        return 0;
    }
    // SAFETY: caller contract.
    let s = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < s.len() {
        let (Some(h), Some(l)) = (hex_digit(s[i]), hex_digit(s[i + 1])) else {
            return 0;
        };
        out.push((h << 4) | l);
        i += 2;
    }
    table().lock().unwrap().alloc(Entry::Buffer(out))
}

// ── Base64 (RFC 4648, padded) ────────────────────────────────────────

const B64_ALPHA: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_BASE64_ENCODE(ptr: i64, len: i64) -> u64 {
    if ptr == 0 || len < 0 {
        return 0;
    }
    // SAFETY: caller contract.
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes[i + 1];
        let b2 = bytes[i + 2];
        out.push(B64_ALPHA[(b0 >> 2) as usize] as char);
        out.push(B64_ALPHA[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize] as char);
        out.push(B64_ALPHA[(((b1 & 0b1111) << 2) | (b2 >> 6)) as usize] as char);
        out.push(B64_ALPHA[(b2 & 0b11_1111) as usize] as char);
        i += 3;
    }
    let remaining = bytes.len() - i;
    if remaining == 1 {
        let b0 = bytes[i];
        out.push(B64_ALPHA[(b0 >> 2) as usize] as char);
        out.push(B64_ALPHA[((b0 & 0b11) << 4) as usize] as char);
        out.push('=');
        out.push('=');
    } else if remaining == 2 {
        let b0 = bytes[i];
        let b1 = bytes[i + 1];
        out.push(B64_ALPHA[(b0 >> 2) as usize] as char);
        out.push(B64_ALPHA[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize] as char);
        out.push(B64_ALPHA[((b1 & 0b1111) << 2) as usize] as char);
        out.push('=');
    }
    intern(&out)
}

fn b64_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_BASE64_DECODE(ptr: *const u8, len: i64) -> u64 {
    if ptr.is_null() || len < 0 || len % 4 != 0 {
        return 0;
    }
    // SAFETY: caller contract.
    let s = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut i = 0;
    while i < s.len() {
        let c0 = s[i];
        let c1 = s[i + 1];
        let c2 = s[i + 2];
        let c3 = s[i + 3];
        let (Some(v0), Some(v1)) = (b64_val(c0), b64_val(c1)) else {
            return 0;
        };
        out.push((v0 << 2) | (v1 >> 4));
        if c2 != b'=' {
            let Some(v2) = b64_val(c2) else { return 0 };
            out.push(((v1 & 0b1111) << 4) | (v2 >> 2));
            if c3 != b'=' {
                let Some(v3) = b64_val(c3) else { return 0 };
                out.push(((v2 & 0b11) << 6) | v3);
            }
        }
        i += 4;
    }
    table().lock().unwrap().alloc(Entry::Buffer(out))
}
