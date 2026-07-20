//! node:crypto — instance storage for the `Hash`/`Hmac` objects. Same
//! object-backed Registry-class model as `StringDecoder`: the instance is an
//! `Entry::Map` tagged `__rts_class = "Hash"`, carrying the algorithm index, an
//! HMAC flag, and the accumulated input (and, for HMAC, the key) as
//! `Entry::Buffer` handles that `update` extends and `digest` reads. Node
//! streams the input; accumulating it and hashing at `digest` yields identical
//! output.

use rts_engine::heap::handles::{alloc_entry, with_entry, with_entry_mut, Entry};
use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_PAYLOAD_MASK};

use super::algo::Algo;

/// Read the raw bytes of an `update`/`digest`/HMAC argument: `Entry::Buffer`
/// (ArrayBuffer view), `Entry::Vec` (`new Uint8Array([...])`, boxed number
/// words), or `Entry::String` (its UTF-8 bytes). Anything else → empty.
pub fn read_bytes(handle: u64) -> Vec<u8> {
    with_entry(handle, |e| match e {
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

/// A byte slice → a `Uint8Array`-shaped `Entry::Vec` (each byte an inline-f64
/// number word), so JS `.length`/indexing work on the result.
pub fn byte_array(bytes: &[u8]) -> u64 {
    let words: Vec<i64> = bytes.iter().map(|&b| f64::from(b).to_bits() as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// Build a `Hash` (or `Hmac`, when `key` is `Some`) instance object.
pub fn build_instance(algo: Algo, key: Option<&[u8]>) -> u64 {
    let mut m: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    m.insert("__rts_class".to_string(), alloc_entry(Entry::String(b"Hash".to_vec())) as i64);
    m.insert("__algo".to_string(), algo.index());
    m.insert("__hmac".to_string(), key.is_some() as i64);
    m.insert("__buf".to_string(), alloc_entry(Entry::Buffer(Vec::new())) as i64);
    if let Some(k) = key {
        m.insert("__key".to_string(), alloc_entry(Entry::Buffer(k.to_vec())) as i64);
    }
    alloc_entry(Entry::Map(Box::new(m)))
}

fn field(handle: u64, key: &str) -> Option<i64> {
    with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied(),
        _ => None,
    })
}

/// Append `data` to the instance's accumulated-input buffer.
pub fn append(handle: u64, data: &[u8]) {
    if let Some(buf_h) = field(handle, "__buf") {
        with_entry_mut(buf_h as u64, |e| {
            if let Some(Entry::Buffer(b)) = e {
                b.extend_from_slice(data);
            }
        });
    }
}

/// The `(algo, is_hmac, input_bytes, key_bytes)` needed to finalize a digest.
pub fn finalize_state(handle: u64) -> Option<(Algo, bool, Vec<u8>, Vec<u8>)> {
    let algo = Algo::from_index(field(handle, "__algo")?);
    let is_hmac = field(handle, "__hmac").unwrap_or(0) != 0;
    let input = field(handle, "__buf").map(|h| read_bytes(h as u64)).unwrap_or_default();
    let key = field(handle, "__key").map(|h| read_bytes(h as u64)).unwrap_or_default();
    Some((algo, is_hmac, input, key))
}

/// Build a `Cipher` (Cipheriv/Decipheriv) instance object: algorithm index,
/// key/iv bytes, an `is_decrypt` flag, the accumulated input, and (GCM only)
/// an AAD buffer + auth tag buffer set via `setAAD`/`setAuthTag`.
pub fn build_cipher_instance(algo_index: i64, key: &[u8], iv: &[u8], is_decrypt: bool) -> u64 {
    let mut m: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    m.insert("__rts_class".to_string(), alloc_entry(Entry::String(b"Cipher".to_vec())) as i64);
    m.insert("__algo".to_string(), algo_index);
    m.insert("__key".to_string(), alloc_entry(Entry::Buffer(key.to_vec())) as i64);
    m.insert("__iv".to_string(), alloc_entry(Entry::Buffer(iv.to_vec())) as i64);
    m.insert("__decrypt".to_string(), is_decrypt as i64);
    m.insert("__buf".to_string(), alloc_entry(Entry::Buffer(Vec::new())) as i64);
    m.insert("__aad".to_string(), alloc_entry(Entry::Buffer(Vec::new())) as i64);
    m.insert("__tag".to_string(), alloc_entry(Entry::Buffer(Vec::new())) as i64);
    alloc_entry(Entry::Map(Box::new(m)))
}

/// Overwrite a cipher instance's `__buf`-style field with fresh bytes
/// (`setAAD`/`setAuthTag` replace rather than accumulate).
pub fn set_field_bytes(handle: u64, key: &str, data: &[u8]) {
    if let Some(h) = field(handle, key) {
        with_entry_mut(h as u64, |e| {
            if let Some(Entry::Buffer(b)) = e {
                *b = data.to_vec();
            }
        });
    }
}

/// The full cipher state needed to finalize: `(algo_index, key, iv, is_decrypt,
/// input, aad, tag)`.
pub fn cipher_state(handle: u64) -> Option<(i64, Vec<u8>, Vec<u8>, bool, Vec<u8>, Vec<u8>, Vec<u8>)> {
    let algo_index = field(handle, "__algo")?;
    let key = field(handle, "__key").map(|h| read_bytes(h as u64)).unwrap_or_default();
    let iv = field(handle, "__iv").map(|h| read_bytes(h as u64)).unwrap_or_default();
    let is_decrypt = field(handle, "__decrypt").unwrap_or(0) != 0;
    let input = field(handle, "__buf").map(|h| read_bytes(h as u64)).unwrap_or_default();
    let aad = field(handle, "__aad").map(|h| read_bytes(h as u64)).unwrap_or_default();
    let tag = field(handle, "__tag").map(|h| read_bytes(h as u64)).unwrap_or_default();
    Some((algo_index, key, iv, is_decrypt, input, aad, tag))
}
