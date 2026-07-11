//! node:string_decoder — instance storage. A `StringDecoder` instance is an
//! `Entry::Map` tagged `__rts_class = "StringDecoder"` (the same object-backed
//! class model `DOMException` uses), carrying the encoding + the incremental
//! decoder's pending-byte state as plain integer/string fields. The `Decoder`
//! is loaded from the map, run, and stored back via `with_entry_mut`.

use rts_engine::heap::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

use super::decoder::{Decoder, Enc};

fn enc_index(enc: Enc) -> i64 {
    match enc {
        Enc::Utf8 => 0,
        Enc::Utf16le => 1,
        Enc::Base64 => 2,
        Enc::Base64url => 3,
        Enc::Latin1 => 4,
        Enc::Ascii => 5,
        Enc::Hex => 6,
    }
}

fn enc_from_index(i: i64) -> Enc {
    match i {
        1 => Enc::Utf16le,
        2 => Enc::Base64,
        3 => Enc::Base64url,
        4 => Enc::Latin1,
        5 => Enc::Ascii,
        6 => Enc::Hex,
        _ => Enc::Utf8,
    }
}

fn pack(bytes: &[u8; 4]) -> i64 {
    (bytes[0] as i64) | ((bytes[1] as i64) << 8) | ((bytes[2] as i64) << 16) | ((bytes[3] as i64) << 24)
}

fn unpack(v: i64) -> [u8; 4] {
    [v as u8, (v >> 8) as u8, (v >> 16) as u8, (v >> 24) as u8]
}

/// Build a fresh `StringDecoder` instance object for `enc`.
pub fn build_instance(enc: Enc) -> u64 {
    let mut m: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    m.insert(
        "__rts_class".to_string(),
        alloc_entry(Entry::String(b"StringDecoder".to_vec())) as i64,
    );
    m.insert(
        "encoding".to_string(),
        alloc_entry(Entry::String(enc.name().as_bytes().to_vec())) as i64,
    );
    m.insert("__enc".to_string(), enc_index(enc));
    m.insert("__lc".to_string(), 0);
    m.insert("__need".to_string(), 0);
    m.insert("__total".to_string(), 0);
    alloc_entry(Entry::Map(Box::new(m)))
}

/// Load the decoder state from an instance handle.
pub fn load(handle: u64) -> Option<Decoder> {
    with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => {
            let enc = enc_from_index(m.get("__enc").copied().unwrap_or(0));
            Some(Decoder {
                enc,
                last_char: unpack(m.get("__lc").copied().unwrap_or(0)),
                last_need: m.get("__need").copied().unwrap_or(0).max(0) as usize,
                last_total: m.get("__total").copied().unwrap_or(0).max(0) as usize,
            })
        }
        _ => None,
    })
}

/// Store the (mutated) pending state back into the instance handle.
pub fn store(handle: u64, d: &Decoder) {
    with_entry_mut(handle, |e| {
        if let Some(Entry::Map(m)) = e {
            m.insert("__lc".to_string(), pack(&d.last_char));
            m.insert("__need".to_string(), d.last_need as i64);
            m.insert("__total".to_string(), d.last_total as i64);
        }
    });
}

/// Read the raw bytes of a `write`/`end` argument:
/// - `Entry::Buffer` (an ArrayBuffer-view typed array) — verbatim bytes;
/// - `Entry::Vec` (the common `new Uint8Array([...])` form — a JS array of
///   boxed number words) — each element's low byte;
/// - `Entry::String` (a JS string) — its UTF-8 bytes (Node's implicit
///   `Buffer.from(string)`).
/// Anything else → empty.
pub fn read_bytes(handle: u64) -> Vec<u8> {
    use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_PAYLOAD_MASK};
    with_entry(handle, |e| match e {
        Some(Entry::Buffer(b)) => b.clone(),
        Some(Entry::String(s)) => s.clone(),
        Some(Entry::Vec(v)) => v
            .iter()
            .map(|&w| {
                let u = w as u64;
                if (u & POLY_BOX_BASE) != POLY_BOX_BASE {
                    // inline f64
                    f64::from_bits(u) as u8
                } else {
                    // boxed INT32 (or other) — low payload bits
                    (u & POLY_PAYLOAD_MASK) as u32 as u8
                }
            })
            .collect(),
        _ => Vec::new(),
    })
}
