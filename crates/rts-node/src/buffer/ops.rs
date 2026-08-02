//! node:buffer — byte-level helpers (base64 + word/handle byte extraction)
//! shared by the `Buffer` class's `#[rtse::class]` statics and by `atob`/`btoa`.

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::poly::{poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK};

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

/// Throw a RangeError for an invalid Buffer size; returns the clamped usize.
pub(super) fn checked_size(size: i64) -> usize {
    if size < 0 {
        let msg = "The value of \"size\" is out of range. It must be >= 0";
        unsafe { __rtsadp_throw_js_error(b"RangeError".as_ptr(), 10, msg.as_ptr(), msg.len() as i64) };
        return 0;
    }
    size as usize
}

/// A byte slice → `Uint8Array`-shaped `Entry::Vec` (each byte an inline-f64 word).
pub(super) fn byte_array(bytes: &[u8]) -> u64 {
    let words: Vec<i64> = bytes.iter().map(|&b| f64::from(b).to_bits() as i64).collect();
    alloc_entry(Entry::vec(words))
}

/// Read the bytes of a Buffer/Uint8Array/string value. `word` may be a boxed
/// PolyValue word (normalized to a handle) OR a raw handle (a Handle-typed arg
/// arrives raw) — `unwrap_or(word)` passes a raw handle straight through.
pub(super) fn word_bytes(word: u64) -> Vec<u8> {
    let h = poly_handle_normalize(word).unwrap_or(word);
    with_entry(h, |e| match e {
        Some(Entry::Buffer(b)) => b.clone(),
        Some(Entry::String(s)) => s.clone(),
        Some(Entry::Vec(v)) => v
            .iter()
            .map(|&w| {
                let u = w as u64;
                if (u & POLY_BOX_BASE) != POLY_BOX_BASE {
                    f64::from_bits(u) as u8
                } else {
                    (u & POLY_PAYLOAD_MASK) as u32 as u8
                }
            })
            .collect(),
        _ => Vec::new(),
    })
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(super) fn base64_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(B64[(n >> 18 & 63) as usize] as char);
        out.push(B64[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[(n & 63) as usize] as char } else { '=' });
    }
    out
}

pub(super) fn base64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let clean: Vec<u8> = s.bytes().filter(|&c| val(c).is_some()).collect();
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c).unwrap_or(0) << (18 - 6 * i);
        }
        out.push((n >> 16 & 0xff) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8 & 0xff) as u8);
        }
        if chunk.len() > 3 {
            out.push((n & 0xff) as u8);
        }
    }
    out
}

/// `btoa(data)` — base64-encode a binary (latin1) string.
#[rtse::function(module = "node:buffer", value = "btoa")]
fn btoa(data: &str) -> String {
    // Each char is a byte (latin1); a multi-byte UTF-8 char takes its low byte.
    let bytes: Vec<u8> = data.chars().map(|c| c as u8).collect();
    base64_encode(&bytes)
}

/// `atob(data)` — decode base64 to a binary (latin1) string.
#[rtse::function(module = "node:buffer", value = "atob")]
fn atob(data: &str) -> String {
    let decoded = base64_decode(data);
    decoded.iter().map(|&b| b as char).collect()
}
