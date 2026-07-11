//! node:path — GC-string interning, `parse` object construction, and `format`
//! object-argument reading (via the engine value model, no JSON round-trip).

use rts_engine::heap::handles::{read_string_handle, with_entry, Entry};
use rts_engine::heap::poly::poly_handle_normalize;
use rts_engine::heap::shapes::{alloc_shaped_object, string_word};

use super::parse::Parsed;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Intern a Rust string as a GC string handle (an ABI `Handle` return).
pub fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Build the `{ root, dir, base, ext, name }` object from a [`Parsed`].
pub fn parsed_object(p: &Parsed) -> u64 {
    let keys: &[&str] = &["root", "dir", "base", "ext", "name"];
    let values: [i64; 5] = [
        string_word(p.root.as_bytes()) as i64,
        string_word(p.dir.as_bytes()) as i64,
        string_word(p.base.as_bytes()) as i64,
        string_word(p.ext.as_bytes()) as i64,
        string_word(p.name.as_bytes()) as i64,
    ];
    alloc_shaped_object(keys, &values)
}

/// Read a `FormatInputPathObject`'s five optional string fields from an object
/// handle: `(root, dir, base, name, ext)`. A missing/non-string field is `None`.
pub fn format_fields(handle: u64) -> (Option<String>, Option<String>, Option<String>, Option<String>, Option<String>) {
    let mut root = None;
    let mut dir = None;
    let mut base = None;
    let mut name = None;
    let mut ext = None;
    for (key, word) in object_entries(handle) {
        let val = word_string(word as u64);
        match key.as_str() {
            "root" => root = val,
            "dir" => dir = val,
            "base" => base = val,
            "name" => name = val,
            "ext" => ext = val,
            _ => {}
        }
    }
    (root, dir, base, name, ext)
}

/// The `(key, value_word)` pairs of a shaped object handle, in shape order.
fn object_entries(handle: u64) -> Vec<(String, i64)> {
    use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_PAYLOAD_MASK};
    use rts_engine::heap::shapes::global_shape_keys;
    with_entry(handle, |e| {
        let slots = match e {
            Some(Entry::Vec(v)) => v,
            _ => return Vec::new(),
        };
        if slots.is_empty() {
            return Vec::new();
        }
        let w0 = slots[0] as u64;
        if (w0 & POLY_BOX_BASE) != POLY_BOX_BASE {
            return Vec::new();
        }
        let shape_id = (w0 & POLY_PAYLOAD_MASK) as u32;
        let keys = match global_shape_keys(shape_id) {
            Some(k) if k.len() + 1 == slots.len() => k,
            _ => return Vec::new(),
        };
        keys.into_iter().zip(slots[1..].iter().copied()).collect()
    })
}

/// A value word that is a string handle → its text; otherwise `None` (a
/// non-string `format` field is ignored, matching Node's coercion of only
/// string-valued fields).
fn word_string(w: u64) -> Option<String> {
    let h = poly_handle_normalize(w)?;
    read_string_handle(h)
}
