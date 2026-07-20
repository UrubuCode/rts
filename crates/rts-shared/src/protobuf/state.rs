//! `protobuf` — instance storage for `ProtoWriter`/`ProtoReader`. Same
//! `Entry::Map`-tagged object-backed class model `node:crypto`'s `Hash`/
//! `Cipher` use. `ProtoWriter` owns an `Entry::Buffer` it appends to.
//! `ProtoReader` wraps an existing `Entry::Buffer` (read-only) plus a cursor
//! offset and the field number/wire type from the last `readTag()` call.

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry, with_entry_mut};
use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_PAYLOAD_MASK};

use super::wire::WireType;

/// Read the raw bytes backing a handle: `Entry::Buffer` directly, or
/// `Entry::Vec` (`Uint8Array`-shaped, boxed number words) decoded byte-wise.
pub fn read_bytes(handle: u64) -> Vec<u8> {
    with_entry(handle, |e| match e {
        Some(Entry::Buffer(b)) => b.clone(),
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

/// Build a `ProtoWriter` instance: `__rts_class` tag + an empty accumulating
/// `Entry::Buffer`.
pub fn build_writer() -> u64 {
    let mut m: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    m.insert("__rts_class".to_string(), alloc_entry(Entry::String(b"ProtoWriter".to_vec())) as i64);
    m.insert("__buf".to_string(), alloc_entry(Entry::Buffer(Vec::new())) as i64);
    alloc_entry(Entry::Map(Box::new(m)))
}

/// Build a `ProtoReader` instance over `data`: `__rts_class` tag, a copy of
/// the input bytes, a cursor `__pos`, and the last-read `__field`/`__wt`.
pub fn build_reader(data: &[u8]) -> u64 {
    let mut m: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    m.insert("__rts_class".to_string(), alloc_entry(Entry::String(b"ProtoReader".to_vec())) as i64);
    m.insert("__buf".to_string(), alloc_entry(Entry::Buffer(data.to_vec())) as i64);
    m.insert("__pos".to_string(), 0);
    m.insert("__field".to_string(), -1);
    m.insert("__wt".to_string(), -1);
    alloc_entry(Entry::Map(Box::new(m)))
}

fn field(handle: u64, key: &str) -> Option<i64> {
    with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied(),
        _ => None,
    })
}

fn set_field(handle: u64, key: &str, value: i64) {
    with_entry_mut(handle, |e| {
        if let Some(Entry::Map(m)) = e {
            m.insert(key.to_string(), value);
        }
    });
}

/// Append raw bytes to a `ProtoWriter`'s accumulated buffer.
pub fn writer_append(handle: u64, data: &[u8]) {
    if let Some(buf_h) = field(handle, "__buf") {
        with_entry_mut(buf_h as u64, |e| {
            if let Some(Entry::Buffer(b)) = e {
                b.extend_from_slice(data);
            }
        });
    }
}

/// The bytes accumulated so far in a `ProtoWriter`.
pub fn writer_bytes(handle: u64) -> Vec<u8> {
    field(handle, "__buf").map(|h| read_bytes(h as u64)).unwrap_or_default()
}

/// A `ProtoReader`'s `(buffer, cursor_pos)`.
pub fn reader_state(handle: u64) -> Option<(Vec<u8>, usize)> {
    let buf = field(handle, "__buf").map(|h| read_bytes(h as u64))?;
    let pos = field(handle, "__pos")? as usize;
    Some((buf, pos))
}

/// Advance a `ProtoReader`'s cursor by `n` bytes.
pub fn reader_advance(handle: u64, n: usize) {
    if let Some(pos) = field(handle, "__pos") {
        set_field(handle, "__pos", pos + n as i64);
    }
}

/// Record the field number + wire type from the last successful `readTag()`.
pub fn reader_set_last_tag(handle: u64, field_number: u32, wire_type: WireType) {
    set_field(handle, "__field", field_number as i64);
    set_field(handle, "__wt", wire_type as i64);
}

/// The wire type recorded by the last `readTag()` call (`None` if invalid/unset).
pub fn reader_last_wire_type(handle: u64) -> Option<WireType> {
    WireType::from_u64(field(handle, "__wt")? as u64)
}

/// The field number recorded by the last `readTag()` call.
pub fn reader_last_field_number(handle: u64) -> i64 {
    field(handle, "__field").unwrap_or(-1)
}
