//! node:buffer — the STATIC / global surface that avoids the instance-method
//! wall: `atob`/`btoa` (base64), and the `Buffer` static methods `alloc`,
//! `allocUnsafe`, `from`, `isBuffer`, `byteLength`, `concat`, `compare`. Buffers
//! are returned as `Uint8Array`-shaped arrays (`.length`/indexing work). Real
//! bytes, no fabrication.

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::poly::{poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }.unwrap_or("").to_string()
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// A byte slice → `Uint8Array`-shaped `Entry::Vec` (each byte an inline-f64 word).
fn byte_array(bytes: &[u8]) -> u64 {
    let words: Vec<i64> = bytes.iter().map(|&b| f64::from(b).to_bits() as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// Read the bytes of a Buffer/Uint8Array/string value. `word` may be a boxed
/// PolyValue word (normalized to a handle) OR a raw handle (a Handle-typed arg
/// arrives raw) — `unwrap_or(word)` passes a raw handle straight through.
fn word_bytes(word: u64) -> Vec<u8> {
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

fn base64_encode(data: &[u8]) -> String {
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

fn base64_decode(s: &str) -> Vec<u8> {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_BUFFER_BTOA(p: *const u8, l: i64) -> u64 {
    let s = read(p, l);
    // Each char is a byte (latin1); a multi-byte UTF-8 char takes its low byte.
    let bytes: Vec<u8> = s.chars().map(|c| c as u8).collect();
    intern(&base64_encode(&bytes))
}

/// `atob(data)` — decode base64 to a binary (latin1) string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_BUFFER_ATOB(p: *const u8, l: i64) -> u64 {
    let decoded = base64_decode(&read(p, l));
    let s: String = decoded.iter().map(|&b| b as char).collect();
    intern(&s)
}

/// `Buffer.alloc(size)` / `Buffer.allocUnsafe(size)` — a zeroed byte array.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_BUFFER_ALLOC(size: i64) -> u64 {
    byte_array(&vec![0u8; size.max(0) as usize])
}

/// `Buffer.from(string)` — UTF-8 bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_BUFFER_FROM(p: *const u8, l: i64) -> u64 {
    byte_array(read(p, l).as_bytes())
}

/// `Buffer.from(string, encoding)` — utf8 / hex / base64 / latin1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_BUFFER_FROM_ENC(p: *const u8, l: i64, ep: *const u8, el: i64) -> u64 {
    let s = read(p, l);
    let bytes = match read(ep, el).to_lowercase().as_str() {
        "hex" => (0..s.len() / 2)
            .filter_map(|i| u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok())
            .collect(),
        "base64" | "base64url" => base64_decode(&s),
        "latin1" | "binary" => s.chars().map(|c| c as u8).collect(),
        _ => s.into_bytes(),
    };
    byte_array(&bytes)
}

/// `Buffer.isBuffer(obj)` — a Uint8Array-shaped array or a Buffer.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_BUFFER_IS_BUFFER(word: u64) -> i64 {
    let h = poly_handle_normalize(word).unwrap_or(word);
    with_entry(h, |e| matches!(e, Some(Entry::Buffer(_)) | Some(Entry::Vec(_)))) as i64
}

/// `Buffer.byteLength(string)` — UTF-8 byte count.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_BUFFER_BYTE_LENGTH(p: *const u8, l: i64) -> i64 {
    read(p, l).len() as i64
}

/// `Buffer.compare(a, b)` — lexicographic (-1 / 0 / 1).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_BUFFER_COMPARE(a: u64, b: u64) -> i64 {
    match word_bytes(a).cmp(&word_bytes(b)) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// `Buffer.concat(list)` — concatenate an array of buffers.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_BUFFER_CONCAT(list: u64) -> u64 {
    let mut out = Vec::new();
    let h = poly_handle_normalize(list).unwrap_or(list);
    with_entry(h, |e| {
        if let Some(Entry::Vec(v)) = e {
            for &w in v.iter() {
                out.extend_from_slice(&word_bytes(w as u64));
            }
        }
    });
    byte_array(&out)
}
