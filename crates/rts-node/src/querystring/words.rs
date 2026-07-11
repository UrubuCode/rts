//! node:querystring — value-word builders (for `parse`'s object result) and
//! native readers (for `stringify`'s object argument).
//!
//! Reuses the engine value model directly: objects are shaped `Entry::Vec`
//! (slot 0 = shape id, decoded via `global_shape_keys`); arrays are plain
//! `Entry::Vec`; scalar words decode through the NaN-box tags. No duplicated
//! representation, no JSON round-trip.

use rts_engine::heap::handles::{alloc_entry, read_string_handle, with_entry, Entry};
use rts_engine::heap::poly::{
    poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK, POLY_TAG_MASK, POLY_TAG_OBJECT,
    POLY_TAG_SHIFT, POLY_TAG_SINGLETON, POLY_TAG_STR,
};
use rts_engine::heap::shapes::{
    alloc_shaped_object_owned, global_shape_keys, handle_word_auto, string_word,
};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Intern a Rust string as a GC string handle (the ABI `Handle` return).
pub fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// A `string` element word.
pub fn str_word(s: &str) -> i64 {
    string_word(s.as_bytes()) as i64
}

/// A JS array of strings, as an OBJECT element word (for an object slot value).
pub fn str_array_word(items: &[String]) -> i64 {
    let words: Vec<i64> = items.iter().map(|s| str_word(s)).collect();
    handle_word_auto(alloc_entry(Entry::Vec(Box::new(words)))) as i64
}

/// Build a shaped object `{ keys[i]: values[i] }` (owned keys) → raw handle.
pub fn object(keys: Vec<String>, values: &[i64]) -> u64 {
    alloc_shaped_object_owned(keys, values)
}

// --- native readers (stringify's object argument) ---------------------------

/// The `(key, value_word)` pairs of a shaped object handle, in shape order.
/// `None` if the handle is not a shaped object (missing shape / not a Vec).
pub fn object_entries(handle: u64) -> Option<Vec<(String, i64)>> {
    with_entry(handle, |e| {
        let slots = match e {
            Some(Entry::Vec(v)) => v,
            _ => return None,
        };
        if slots.is_empty() {
            return None;
        }
        let shape_id = shape_id_of(slots[0] as u64)?;
        let keys = global_shape_keys(shape_id)?;
        if keys.len() + 1 != slots.len() {
            return None;
        }
        Some(keys.into_iter().zip(slots[1..].iter().copied()).collect())
    })
}

/// The string elements of a plain (non-shaped) array handle. Each element is
/// coerced to its scalar string form.
pub fn array_strings(handle: u64) -> Vec<String> {
    with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => v.iter().map(|&w| scalar_string(w as u64)).collect(),
        _ => Vec::new(),
    })
}

/// `Some(shape_id)` if `w` is a boxed-INT32 word carrying a REGISTERED global
/// shape id (i.e. slot 0 of a shaped object); `None` otherwise (an array's
/// first element, a scalar).
fn shape_id_of(w: u64) -> Option<u32> {
    if (w & POLY_BOX_BASE) != POLY_BOX_BASE {
        return None;
    }
    // shape ids are boxed INT32 (tag 1); the payload low 32 bits are the id.
    let id = (w & POLY_PAYLOAD_MASK) as u32;
    global_shape_keys(id).map(|_| id)
}

/// Whether a value word is a nested array/object (OBJECT tag), used by
/// `stringify` to decide between the scalar and array/object paths.
pub fn is_object_word(w: u64) -> bool {
    (w & POLY_BOX_BASE) == POLY_BOX_BASE
        && ((w >> POLY_TAG_SHIFT) & POLY_TAG_MASK) == POLY_TAG_OBJECT
}

/// The runtime handle behind a STR/OBJECT/FUNCTION word, or `None`.
pub fn word_handle(w: u64) -> Option<u64> {
    poly_handle_normalize(w)
}

/// Decode a value word to its querystring scalar string form: a string handle →
/// its text; a number → its decimal form; a boolean → `"true"`/`"false"`;
/// `null`/`undefined` → `""` (Node coerces these to empty in `stringify`).
pub fn scalar_string(w: u64) -> String {
    // Inline double (not boxed) → a real number.
    if (w & POLY_BOX_BASE) != POLY_BOX_BASE {
        return format_number(f64::from_bits(w));
    }
    let tag = (w >> POLY_TAG_SHIFT) & POLY_TAG_MASK;
    if tag == POLY_TAG_STR {
        if let Some(h) = poly_handle_normalize(w) {
            return read_string_handle(h).unwrap_or_default();
        }
        return String::new();
    }
    if tag == POLY_TAG_SINGLETON {
        // true/false carry distinct payloads; null/undefined → "".
        return singleton_string(w);
    }
    // Boxed INT32 (tag 1): the low 32 bits are the i32 value.
    format_number((w & POLY_PAYLOAD_MASK) as u32 as i32 as f64)
}

/// Format a JS number the way `String(n)` would for the integer/finite cases
/// querystring encounters (no exponent gymnastics needed for query values).
fn format_number(n: f64) -> String {
    if n.is_nan() || n.is_infinite() {
        // Node's stringify coerces non-finite numbers to "".
        return String::new();
    }
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    format!("{n}")
}

const SINGLETON_FALSE: u64 = 2;
const SINGLETON_TRUE: u64 = 3;

fn singleton_string(w: u64) -> String {
    match w & POLY_PAYLOAD_MASK {
        SINGLETON_TRUE => "true".to_string(),
        SINGLETON_FALSE => "false".to_string(),
        _ => String::new(),
    }
}
