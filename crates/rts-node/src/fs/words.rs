//! node:fs — value helpers: ABI string read, interning, byte-input reading,
//! Uint8Array-shaped byte output, and turning a `std::io::Error` into a
//! Node-style thrown error (`ENOENT: ...`).

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_PAYLOAD_MASK};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
    // Truthiness of a PolyValue word (returns raw 0/1) — used to read an
    // options object's boolean flags (`{ recursive: true }`).
    fn __rtsadp_to_boolean(v: u64) -> i64;
}

/// Read a boolean flag off an options OBJECT handle (`options.recursive`).
/// Handles both a shaped object literal (`Entry::Vec`: slot 0 = shape id, the
/// remaining slots the values, keyed by `global_shape_keys`) and an
/// `Entry::Map`. Missing field / non-object → `false`.
pub fn opt_bool(options: u64, key: &str) -> bool {
    use rts_engine::heap::shapes::global_shape_keys;
    let word = with_entry(options, |e| match e {
        Some(Entry::Vec(slots)) if !slots.is_empty() => {
            let w0 = slots[0] as u64;
            if (w0 & POLY_BOX_BASE) != POLY_BOX_BASE {
                return None;
            }
            let shape_id = (w0 & POLY_PAYLOAD_MASK) as u32;
            let keys = global_shape_keys(shape_id)?;
            if keys.len() + 1 != slots.len() {
                return None;
            }
            keys.iter().position(|k| k == key).map(|i| slots[i + 1])
        }
        Some(Entry::Map(m)) => m.get(key).copied(),
        _ => None,
    });
    match word {
        Some(w) => unsafe { __rtsadp_to_boolean(w as u64) != 0 },
        None => false,
    }
}

pub fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }.unwrap_or("").to_string()
}

pub fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// A byte slice → `Uint8Array`-shaped `Entry::Vec` (each byte an inline-f64
/// number word), so JS `.length`/indexing work.
pub fn byte_array(bytes: &[u8]) -> u64 {
    let words: Vec<i64> = bytes.iter().map(|&b| f64::from(b).to_bits() as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// A string list → `Entry::Vec` of string words (for `readdirSync`).
pub fn string_array(items: &[String]) -> u64 {
    use rts_engine::heap::shapes::string_word;
    let words: Vec<i64> = items.iter().map(|s| string_word(s.as_bytes()) as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// Read the bytes of a `write`/`append` data argument: `Entry::String` (UTF-8),
/// `Entry::Buffer`, or `Entry::Vec` (`Uint8Array`). Anything else → empty.
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

/// The Node error code for an `io::Error`.
pub(super) fn err_code(e: &std::io::Error) -> &'static str {
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => "ENOENT",
        PermissionDenied => "EACCES",
        AlreadyExists => "EEXIST",
        _ => match e.raw_os_error() {
            Some(13) => "EACCES",
            Some(2) => "ENOENT",
            Some(17) => "EEXIST",
            Some(20) => "ENOTDIR",
            Some(21) => "EISDIR",
            Some(39) | Some(41) | Some(145) => "ENOTEMPTY",
            _ => "EIO",
        },
    }
}

/// Throw a Node-style `Error` for a failed fs op (`<CODE>: <msg>, <op> '<path>'`).
pub fn throw_io(e: &std::io::Error, op: &str, path: &str) {
    let code = err_code(e);
    let msg = format!("{code}: {e}, {op} '{path}'");
    unsafe { __rtsadp_throw_js_error(code.as_ptr(), code.len() as i64, msg.as_ptr(), msg.len() as i64) };
}
