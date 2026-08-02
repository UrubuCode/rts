//! PolyValue WORD constructors and shaped-object allocation.
//!
//! Split out of `shapes/mod.rs` (which owns the shape REGISTRY) when that file
//! passed the 700-line engine ceiling. The two groups are genuinely separate:
//! everything here builds or reads a 64-bit PolyValue word and touches the
//! registry only to intern a key list, while the registry side never constructs
//! a word beyond the shape-id header.
//!
//! The tag/payload layout mirrors `rts-runtime::adapters::value::layout` and is
//! a FROZEN ABI — the JIT bakes these bit patterns as immediates.

use super::super::handles::{Entry, alloc_entry};
use super::super::poly::{POLY_BOX_BASE, POLY_TAG_SHIFT};
use super::{GlobalShapeId, intern_global_shape};

/// INT32 tag of the PolyValue NaN-box (shape ids ride slot 0 as boxed INT32).
const POLY_TAG_INT32: u64 = 1;
/// Singleton tag + payloads (mirrors `rts-runtime::adapters::value::layout` — frozen ABI).
const POLY_TAG_SINGLETON: u64 = 2;
const SINGLETON_NULL: u64 = 1;
const SINGLETON_FALSE: u64 = 2;
const SINGLETON_TRUE: u64 = 3;
/// String / object / function tags (frozen ABI, mirrors the value model).
const POLY_TAG_STR: u64 = 3;
const POLY_TAG_OBJECT: u64 = 4;
const POLY_TAG_FUNCTION: u64 = 5;

/// The `true`/`false` singleton PolyValue words.
pub fn bool_word(b: bool) -> u64 {
    let sel = if b { SINGLETON_TRUE } else { SINGLETON_FALSE };
    POLY_BOX_BASE | (POLY_TAG_SINGLETON << POLY_TAG_SHIFT) | sel
}

/// The `null` singleton PolyValue word.
pub fn null_word() -> u64 {
    POLY_BOX_BASE | (POLY_TAG_SINGLETON << POLY_TAG_SHIFT) | SINGLETON_NULL
}

/// Allocate `bytes` as a fresh string entry and return its STR PolyValue word.
pub fn string_word(bytes: &[u8]) -> u64 {
    let h = alloc_entry(Entry::String(bytes.to_vec()));
    let slot = crate::heap::handles::__rtsn_poly_from_handle(h);
    POLY_BOX_BASE | (POLY_TAG_STR << POLY_TAG_SHIFT) | slot
}

/// Box the raw handle `h` as the PolyValue word matching its LIVE entry kind
/// (String → STR word, Function → FUNCTION word, anything else → OBJECT word).
/// `0` → null word. The runtime-layer mirror of the engine's
/// `__rtsadp_box_handle_auto` top-level (no element normalization).
pub fn handle_word_auto(h: u64) -> u64 {
    use crate::heap::handles::{Entry as E, with_entry};
    if h == 0 {
        return null_word();
    }
    let tag = with_entry(h, |e| match e {
        Some(E::String(_)) => POLY_TAG_STR,
        Some(E::Function(_)) => POLY_TAG_FUNCTION,
        Some(_) => POLY_TAG_OBJECT,
        None => 0,
    });
    if tag == 0 {
        return null_word();
    }
    let slot = crate::heap::handles::__rtsn_poly_from_handle(h);
    POLY_BOX_BASE | (tag << POLY_TAG_SHIFT) | slot
}

/// Legacy raw-i64 sentinels (the era-i64 surface: TPL_COERCE/codegen convention).
const RAW_BOOL_FALSE: i64 = i64::MIN;
const RAW_BOOL_TRUE: i64 = i64::MIN + 1;
const RAW_UNDEFINED: i64 = i64::MIN + 2;
const RAW_NULL: i64 = i64::MIN + 3;

/// Convert a LEGACY raw-i64 surface value into a PolyValue word: an
/// already-boxed word passes through; the era-i64 sentinels map to their
/// singletons; a live raw handle (≥ 2^48) boxes by entry kind; an exact-i32
/// int boxes as INT32; anything else passes through verbatim (inline-double
/// bits are already a valid word).
pub fn legacy_i64_to_word(raw: i64) -> u64 {
    let w = raw as u64;
    if (w & POLY_BOX_BASE) == POLY_BOX_BASE {
        return w;
    }
    match raw {
        RAW_BOOL_FALSE => return bool_word(false),
        RAW_BOOL_TRUE => return bool_word(true),
        RAW_UNDEFINED => return crate::heap::poly::POLY_UNDEFINED,
        RAW_NULL => return null_word(),
        _ => {}
    }
    if w >= (1 << 48) {
        use crate::heap::handles::with_entry;
        let live = with_entry(w, |e| e.is_some());
        if live {
            return handle_word_auto(w);
        }
    }
    if let Ok(i) = i32::try_from(raw) {
        return POLY_BOX_BASE | (POLY_TAG_INT32 << POLY_TAG_SHIFT) | (i as u32 as u64);
    }
    w
}

/// The boxed-INT32 PolyValue word carrying `id` (what object slot 0 stores).
pub fn shape_id_word(id: GlobalShapeId) -> u64 {
    POLY_BOX_BASE | (POLY_TAG_INT32 << POLY_TAG_SHIFT) | id as u64
}

/// Allocate a runtime SHAPED object `{ keys[i]: values[i] }` — the engine's
/// object representation (`Entry::Vec` with slot 0 = boxed shape id, then one
/// PolyValue-word slot per key). `values` are PolyValue WORDS (or raw i64s a
/// consumer normalizes — prefer words). Returns the raw handle.
pub fn alloc_shaped_object(keys: &[&str], values: &[i64]) -> u64 {
    let owned: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
    alloc_shaped_object_owned(owned, values)
}

/// [`alloc_shaped_object`] with OWNED keys (runtime-computed key lists — e.g.
/// the numbered keys of a regex match row).
pub fn alloc_shaped_object_owned(keys: Vec<String>, values: &[i64]) -> u64 {
    debug_assert_eq!(keys.len(), values.len());
    let id = intern_global_shape(&keys);
    alloc_shaped_object_with_id(id, values)
}

/// [`alloc_shaped_object`] for an ALREADY-INTERNED shape id — the hot-path form.
/// Used by `#[rtse::type]`-generated return code, which interns its shape ONCE
/// (cached in a `OnceLock` at the call site) instead of re-hashing the key list
/// on every call.
pub fn alloc_shaped_object_with_id(id: GlobalShapeId, values: &[i64]) -> u64 {
    // Payload buffer from the per-thread recycler (Tier 4.2, `heap::bump`); with
    // `RTS_BUMP` unset this is the `Vec::with_capacity` it replaced.
    let mut slots = crate::heap::bump::acquire(values.len() + 1);
    slots.push(shape_id_word(id) as i64);
    slots.extend_from_slice(values);
    alloc_entry(Entry::Vec(slots))
}

/// The PolyValue word of a plain `f64` field/value: an inline double is already
/// its own word EXCEPT a NaN must canonicalize to the positive qNaN
/// (`f64::NAN`, sign bit 0) so it can never collide with the boxed-word space
/// (`POLY_BOX_BASE` requires the sign bit set) — mirrors
/// `rts-runtime::adapters::value::PolyValue::from_f64`.
pub fn f64_word(x: f64) -> u64 {
    if x.is_nan() { f64::NAN.to_bits() } else { x.to_bits() }
}

/// The PolyValue word of an `i64` field/value: boxed INT32 when it fits (the
/// same representation an integer literal gets), else a plain JS number
/// (`f64`, like any integer outside the safe/i32 range already behaves in JS).
pub fn int_word(n: i64) -> u64 {
    match i32::try_from(n) {
        Ok(i) => POLY_BOX_BASE | (POLY_TAG_INT32 << POLY_TAG_SHIFT) | (i as u32 as u64),
        Err(_) => f64_word(n as f64),
    }
}
