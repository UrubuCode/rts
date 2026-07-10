//! node:querystring — base extern "C" symbol implementations (the sync surface).
//!
//! ABI mirrors the pure-namespace shape used across RTS: `Str` args arrive as
//! `(ptr, len)` and are rebuilt via `from_abi` (returns `None` on null / invalid
//! UTF-8); string results are interned to GC string handles. `parse`'s `object`
//! return and `stringify`'s `object` arg go through the engine's REAL shaped-
//! object representation (`rts_engine::heap::shapes`/`heap::poly`) — no
//! separate "querystring object" model, so a parsed result is a genuine JS
//! object any other property access can read, and `stringify` reads back any
//! genuine JS object a caller passes in (not just one `parse` built). Symbols
//! follow the rts-node convention `__RTS_FN_NODE_QUERYSTRING_*`.

use std::collections::HashMap;

use rts_engine::abi::str_abi::from_abi;
use rts_engine::heap::handles::{Entry, alloc_entry, read_string_handle, with_entry};
use rts_engine::heap::poly::{
    POLY_BOX_BASE, POLY_PAYLOAD_MASK, POLY_TAG_MASK, POLY_TAG_OBJECT, POLY_TAG_SHIFT,
    POLY_TAG_SINGLETON, POLY_TAG_STR, poly_handle_normalize,
};
use rts_engine::heap::shapes::{alloc_shaped_object_owned, global_shape_keys, handle_word_auto, string_word};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Interns a Rust string as a GC string handle (the ABI `Handle` return).
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Bytes `querystring.escape` leaves unescaped — the `encodeURIComponent`
/// unreserved set: ALPHA / DIGIT / `-` `_` `.` `!` `~` `*` `'` `(` `)`.
fn is_unescaped(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
        )
}

fn hex_upper(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        _ => b'A' + (nibble - 10),
    }
}

/// Percent-encodes `s` per `encodeURIComponent` semantics (uppercase hex).
/// Shared by `escape` and `stringify` (both real percent-encoders, not two
/// diverging implementations).
fn percent_encode(s: &str) -> String {
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    for &b in s.as_bytes() {
        if is_unescaped(b) {
            out.push(b);
        } else {
            out.push(b'%');
            out.push(hex_upper(b >> 4));
            out.push(hex_upper(b & 0x0F));
        }
    }
    // ASCII by construction, so this is always valid UTF-8.
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-decodes `s`, tolerant of malformed `%` sequences (an incomplete/
/// invalid `%XX` is left literal, matching Node's `unescape` fallback). When
/// `convert_plus` is set, a literal `+` decodes to a space FIRST — the extra
/// `application/x-www-form-urlencoded` rule `parse` needs that plain
/// `unescape`/`escape` do not apply.
fn percent_decode(s: &str, convert_plus: bool) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if convert_plus && bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push(((hi << 4) | lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `querystring.escape(str)` — percent-encodes per `encodeURIComponent`
/// semantics (uppercase hex), used by `querystring.stringify`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_ESCAPE(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&percent_encode(s))
}

/// `querystring.unescape(str)` — percent-decodes, tolerant of malformed `%`
/// sequences. Does NOT convert `+` to space (that lives in `parse`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_UNESCAPE(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&percent_decode(s, false))
}

/// `querystring.parse(str)` — legacy form-decode into a REAL plain object
/// (the engine's shaped-object model: `alloc_shaped_object_owned`). Splits on
/// `&`, each pair on the FIRST `=` (no `=` -> empty-string value, e.g. `"a"`
/// -> `{ a: "" }`, matching Node); both key and value are percent-decoded AND
/// have `+` converted to a space (querystring/form semantics, unlike
/// `escape`/`unescape`). A REPEATED key collects into a `string[]` value (a
/// real nested array object, `parse("a=1&a=2")` -> `{ a: ["1", "2"] }`,
/// matching Node) — a single occurrence stays a plain string. Key order is
/// first-seen (matches JS string-key enumeration order).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_PARSE(ptr: *const u8, len: i64) -> u64 {
    let s = unsafe { from_abi(ptr, len) }.unwrap_or("");
    let mut order: Vec<String> = Vec::new();
    let mut values: HashMap<String, Vec<String>> = HashMap::new();
    for pair in s.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (raw_key, raw_val) = match pair.find('=') {
            Some(idx) => (&pair[..idx], &pair[idx + 1..]),
            None => (pair, ""),
        };
        let key = percent_decode(raw_key, true);
        let val = percent_decode(raw_val, true);
        if !values.contains_key(&key) {
            order.push(key.clone());
        }
        values.entry(key).or_default().push(val);
    }
    let mut out_keys: Vec<String> = Vec::with_capacity(order.len());
    let mut out_vals: Vec<i64> = Vec::with_capacity(order.len());
    for key in order {
        // Safe: `key` was just pushed to `order` alongside its `values` entry.
        let vs = &values[&key];
        let word = if vs.len() == 1 {
            string_word(vs[0].as_bytes()) as i64
        } else {
            let words: Vec<i64> = vs.iter().map(|v| string_word(v.as_bytes()) as i64).collect();
            let arr_handle = alloc_entry(Entry::Vec(Box::new(words)));
            handle_word_auto(arr_handle) as i64
        };
        out_keys.push(key);
        out_vals.push(word);
    }
    alloc_shaped_object_owned(out_keys, &out_vals)
}

/// True iff `word` is in the NaN-box's boxed (non-double) space.
fn is_boxed(word: u64) -> bool {
    (word & POLY_BOX_BASE) == POLY_BOX_BASE
}

/// The 3-bit tag of a boxed word (meaningless unless [`is_boxed`]).
fn tag_of(word: u64) -> u64 {
    (word >> POLY_TAG_SHIFT) & POLY_TAG_MASK
}

/// Tag for a boxed small-integer (`int32`) PolyValue. NOT re-exported by
/// `rts_engine::heap::poly` (only the STR/OBJECT/FUNCTION/SINGLETON handle-
/// carrying tags are); mirrors the same frozen NaN-box ABI
/// `rts_engine::heap::shapes` privately duplicates its own copy of (see its
/// `POLY_TAG_INT32`) — this is the established per-file pattern for this
/// value-model constant, not a local invention.
const POLY_TAG_INT32: u64 = 1;
/// Singleton payload selectors (mirrors `rts-adapters::value::layout` /
/// `rts_engine::heap::shapes`' own private copy — frozen ABI).
const SINGLETON_FALSE: u64 = 2;
const SINGLETON_TRUE: u64 = 3;

/// Whether `word` is a NaN-boxed OBJECT that is a genuine JS ARRAY — i.e. a
/// header-less `Entry::Vec` (no shape-id at slot 0) — as opposed to a KEYED
/// object (shape-id header whose key count matches the Vec length). Mirrors
/// the discriminator `rts-adapters::value::inspect::looks_like_object` uses,
/// replicated here directly over `rts_engine::heap` primitives (this crate
/// does not depend on `rts-adapters`).
fn is_array_word(word: u64) -> bool {
    if !is_boxed(word) || tag_of(word) != POLY_TAG_OBJECT {
        return false;
    }
    let Some(handle) = poly_handle_normalize(word) else {
        return false;
    };
    !looks_like_keyed_object(handle)
}

/// Whether the REAL handle `handle` (already unboxed from a `TAG_OBJECT`
/// word) is a keyed object: an `Entry::Vec` whose slot 0 is a boxed `int32`
/// shape id, and whose registered key count is exactly `len - 1`.
fn looks_like_keyed_object(handle: u64) -> bool {
    with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => {
            let Some(&slot0) = v.first() else {
                return false;
            };
            let slot0 = slot0 as u64;
            if !is_boxed(slot0) || tag_of(slot0) != POLY_TAG_INT32 {
                return false;
            }
            let id = (slot0 & POLY_PAYLOAD_MASK) as u32;
            match global_shape_keys(id) {
                Some(keys) => keys.len() + 1 == v.len(),
                None => false,
            }
        }
        _ => false,
    })
}

/// Read the OWN key/value-word pairs of a keyed-object REAL handle, or `None`
/// when `handle` is not a keyed object (an array, a non-object value, an
/// invalid/freed handle, ...). Mirrors the same shape-header read
/// `looks_like_keyed_object` validates, just also returning the payload.
fn shaped_object_entries(handle: u64) -> Option<Vec<(String, u64)>> {
    with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => {
            let &slot0 = v.first()?;
            let slot0 = slot0 as u64;
            if !is_boxed(slot0) || tag_of(slot0) != POLY_TAG_INT32 {
                return None;
            }
            let id = (slot0 & POLY_PAYLOAD_MASK) as u32;
            let keys = global_shape_keys(id)?;
            if keys.len() + 1 != v.len() {
                return None;
            }
            Some(keys.into_iter().zip(v[1..].iter().map(|w| *w as u64)).collect())
        }
        _ => None,
    })
}

/// Best-effort JS `Number`-to-string: whole numbers within a safe `i64` range
/// print without a decimal point (`1` not `1.0`); anything else uses Rust's
/// default `f64` `Display`. This is NOT a byte-exact port of the ECMA-262
/// `Number::toString` algorithm (exponential notation thresholds, shortest-
/// round-trip digit count, etc.) — it is exact for integers and a close
/// approximation for the rest, which covers the overwhelmingly common
/// querystring case (small integers) exactly and the rest reasonably.
fn number_to_string(f: f64) -> String {
    if f == f.trunc() && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

/// Node's `querystring` internal `stringifyPrimitive`: a string passes
/// through as-is; a finite number renders its JS decimal form; a boolean
/// renders `"true"`/`"false"`; everything else (null, undefined, NaN,
/// +/-Infinity, a nested object/array element, a function) renders the empty
/// string — matching Node's real `stringify` behavior of emitting `key=`
/// (an EMPTY value, not a literal `"null"`/`"undefined"`/`"[object
/// Object]"`) for a non-primitive/non-finite value.
fn stringify_primitive(word: u64) -> String {
    if !is_boxed(word) {
        let f = f64::from_bits(word);
        return if f.is_finite() { number_to_string(f) } else { String::new() };
    }
    match tag_of(word) {
        t if t == POLY_TAG_STR => poly_handle_normalize(word)
            .and_then(read_string_handle)
            .unwrap_or_default(),
        t if t == POLY_TAG_INT32 => ((word as u32) as i32).to_string(),
        t if t == POLY_TAG_SINGLETON => match word & POLY_PAYLOAD_MASK {
            SINGLETON_TRUE => "true".to_string(),
            SINGLETON_FALSE => "false".to_string(),
            // null / undefined / hole / empty.
            _ => String::new(),
        },
        // OBJECT / FUNCTION.
        _ => String::new(),
    }
}

/// `querystring.stringify(obj)` — the inverse of `parse`: reads the REAL
/// runtime object behind `obj_handle` (any genuine JS object, not just one
/// `parse` built — the engine's shaped-object representation: `Entry::Vec`
/// with slot 0 = a boxed shape id, then one PolyValue-word slot per key; see
/// `rts_engine::heap::shapes`) and joins `key=value` pairs with `&`,
/// percent-encoding both. A key whose value is a genuine JS ARRAY is
/// flattened back to one `key=value` pair PER ELEMENT (Node's real
/// array-value serialization) — an empty array contributes zero pairs for
/// that key. Any other value renders through [`stringify_primitive`]. A
/// handle that is not a keyed object (invalid, freed, an array itself, a
/// primitive) yields the empty string — fail-soft, matching the rest of this
/// module's error convention.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_STRINGIFY(obj_handle: u64) -> u64 {
    let Some(entries) = shaped_object_entries(obj_handle) else {
        return intern("");
    };
    let mut parts: Vec<String> = Vec::new();
    for (key, word) in entries {
        let enc_key = percent_encode(&key);
        if is_array_word(word) {
            // `is_array_word` already validated `word` normalizes to a live
            // `Entry::Vec` handle, so this `with_entry` always sees `Some`.
            if let Some(handle) = poly_handle_normalize(word) {
                let elems: Vec<i64> = with_entry(handle, |e| match e {
                    Some(Entry::Vec(v)) => v.as_ref().clone(),
                    _ => Vec::new(),
                });
                for elem in elems {
                    let s = stringify_primitive(elem as u64);
                    parts.push(format!("{enc_key}={}", percent_encode(&s)));
                }
            }
        } else {
            let s = stringify_primitive(word);
            parts.push(format!("{enc_key}={}", percent_encode(&s)));
        }
    }
    intern(&parts.join("&"))
}
