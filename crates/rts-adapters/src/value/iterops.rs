//! Codegen-owned ITERATION-source trampolines (P5.10) — for-of / for-in.
//!
//! The loop lowering ([`crate::front::run::stmt`]) desugars every iterating loop
//! to ONE shared index walk over a real `Entry::Vec` of boxed PolyValue WORDS (the
//! engine's array representation): `for (i in 0..VEC_LEN) { x = VEC_GET(arr, i); …
//! }`. An ARRAY iterable already IS such a Vec, so it feeds the walk directly. The
//! two non-array iterables this increment supports — a STRING (for-of) and an
//! OBJECT (for-in) — are first MATERIALIZED into such a Vec by these trampolines,
//! so the walk is representation-identical for all three:
//!
//! - [`__rtsadp_str_chars`] — a string's code points as a fresh array of one-char
//!   string PolyValue words (JS `for (const c of str)` iterates code points).
//! - [`__rtsadp_obj_keys`] — a keyed object's OWN enumerable keys as a fresh array
//!   of string PolyValue words (JS `for (const k in obj)` iterates key strings),
//!   recovered from the object's slot-0 global shape-id (the SAME registry the
//!   inspect / dynamic-property paths read, so the key set never diverges).
//!
//! Convention (matching the rest of the `__rtsadp_*` surface): the source crosses
//! as a raw `u64` PolyValue word; the result is a fresh `Entry::Vec` boxed as a
//! `TAG_OBJECT` array PolyValue word.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use crate::shape::global_shape_keys;

use super::inspect::looks_like_object;
use super::{PolyValue, abi_adapter};

/// Box a fresh real Vec handle as a `TAG_OBJECT` array PolyValue word (the engine's
/// array representation), matching [`super::globalops`]'s array-producing helpers.
fn box_vec_as_array(vec_handle: u64) -> u64 {
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(vec_handle)).raw()
}

/// `__rtsadp_str_chars(str_word)` — the code points of a string as a fresh array
/// whose elements are one-char string PolyValue words (real pool). JS `for...of`
/// over a string yields code points (not UTF-16 units), so we iterate Rust `chars`
/// (Unicode scalar values). A non-string source yields an EMPTY array — the
/// lowering only routes a proven string here, so this is just the inner safety net.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_chars(str_word: u64) -> u64 {
    let v = PolyValue::from_raw(str_word);
    let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    if v.is_string() {
        let s = abi_adapter::resolve_poly(v);
        for ch in s.chars() {
            let word = abi_adapter::intern_poly(&ch.to_string()).raw() as i64;
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
        }
    }
    box_vec_as_array(vec)
}

/// `__rtsadp_to_iter_array(word)` — coerce an UNPROVEN for-of source to an array of
/// element words to walk: an ARRAY rides its own handle (returned verbatim); a
/// STRING is materialized to its code-point char array (`str_chars`); anything else
/// yields an EMPTY array (JS would throw "not iterable" — we have no throw channel
/// in for-of, and the lowering only routes here for values that are plausibly
/// iterable, i.e. NOT a known class instance, so an empty walk is the honest
/// no-throw fallback). This is what lets `for (const ch of s)` (string PARAM) and
/// `for (const x of row)` (nested-array for-of binding) iterate without a static
/// proof of the source's kind.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_to_iter_array(word: u64) -> u64 {
    let v = PolyValue::from_raw(word);
    if v.is_object() && !looks_like_object(v) {
        // Already an array (Vec-backed, NOT a shaped object): walk it directly.
        return word;
    }
    if v.is_string() {
        return __rtsadp_str_chars(word);
    }
    box_vec_as_array(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW())
}

/// `__rtsadp_obj_keys(obj_word)` — the OWN enumerable keys of a keyed object as a
/// fresh array of string PolyValue words, in insertion order (the shape's ordered
/// key list). Recovered from the object's slot-0 global shape-id via the SAME
/// process-global registry the inspect / dynamic-property trampolines read. A
/// non-object source (or one without a live shape header) yields an EMPTY array —
/// the lowering only routes a proven keyed object here.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_keys(obj_word: u64) -> u64 {
    let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let obj = PolyValue::from_raw(obj_word);
    if obj.is_object() && looks_like_object(obj) {
        let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
        let slot0 = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 0) as u64);
        if let Some(keys) = slot0
            .is_int32()
            .then(|| global_shape_keys(slot0.as_i32() as u32))
            .flatten()
        {
            for k in keys {
                let word = abi_adapter::intern_poly(&k).raw() as i64;
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
            }
        }
    }
    box_vec_as_array(vec)
}
