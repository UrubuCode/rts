//! `node:crypto` — shared byte-marshalling helpers between JS values and raw
//! `Vec<u8>`, used by the free functions (`symbols.rs`) and both instance
//! classes (`hash.rs`, `cipher_instance.rs`). The `Hash`/`Cipher` instances
//! themselves are `#[rtse::class]` structs now (`Entry::Rtse`, see those
//! modules) — this file used to also carry their state as a flattened
//! `Entry::Map`, which the class macro's real Rust fields replaced.

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_PAYLOAD_MASK};

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
