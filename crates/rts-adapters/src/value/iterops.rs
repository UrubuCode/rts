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
/// JS own-property ENUMERATION order: ARRAY-INDEX keys (a canonical non-negative
/// integer string `< 2^32-1`, no leading zero) come FIRST in ascending NUMERIC
/// order, then every other key in insertion order. `{ "10": …, "3": …, "b": …,
/// "1": … }` enumerates `1, 3, 10, b`. Used for `Object.keys`/`getOwnPropertyNames`/
/// `for-in` — NOT for the storage-order `object_keys_vec` (slot manipulation).
pub(crate) fn reorder_enum_keys(keys: Vec<String>) -> Vec<String> {
    let as_index = |s: &str| -> Option<u32> {
        if s == "0" {
            return Some(0);
        }
        if s.is_empty() || s.as_bytes()[0] == b'0' || !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s.parse::<u32>().ok().filter(|&n| n != u32::MAX)
    };
    let mut idx: Vec<(u32, String)> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    for k in keys {
        match as_index(&k) {
            Some(n) => idx.push((n, k)),
            None => rest.push(k),
        }
    }
    idx.sort_by_key(|(n, _)| *n);
    idx.into_iter().map(|(_, k)| k).chain(rest).collect()
}

/// `Reflect.ownKeys(target)` — the trap's list VERBATIM (the ECMA `ownKeys`
/// reflector does NOT run the per-key enumerability filter `Object.keys` does).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_own_keys_raw(obj_word: u64) -> u64 {
    if let Some((target, handler)) = super::objops::proxy_parts(obj_word) {
        let trap_key = abi_adapter::intern_poly("ownKeys").raw();
        let trap = super::objops::__rtsadp_obj_get(handler, trap_key);
        if PolyValue::from_raw(trap).is_function() {
            let undef = PolyValue::undefined().raw();
            return super::funcops::__rtsadp_fn_invoke(trap, target, undef, undef, undef, 0);
        }
        return __rtsadp_own_keys_raw(target);
    }
    __rtsadp_obj_keys(obj_word)
}

#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_keys(obj_word: u64) -> u64 {
    // PROXY (#218): the `ownKeys` trap lists the keys; JS's `Object.keys` then
    // runs [[GetOwnProperty]] PER KEY (trap/forward) and keeps only ENUMERABLE
    // ones — a trap key absent from the target (and with no getOwnDesc trap)
    // yields `undefined` → filtered (Bun/Node return `[]` for that shape). No
    // trap → forward to the target.
    if let Some((target, handler)) = super::objops::proxy_parts(obj_word) {
        let trap_key = abi_adapter::intern_poly("ownKeys").raw();
        let trap = super::objops::__rtsadp_obj_get(handler, trap_key);
        if PolyValue::from_raw(trap).is_function() {
            let undef = PolyValue::undefined().raw();
            let keys_word =
                super::funcops::__rtsadp_fn_invoke(trap, target, undef, undef, undef, 0);
            let keys = PolyValue::from_raw(keys_word);
            if !keys.is_object() {
                return keys_word;
            }
            let kh = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(keys.as_handle());
            let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(kh).max(0);
            let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
            let enum_key = abi_adapter::intern_poly("enumerable").raw();
            for i in 0..len {
                let k = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(kh, i) as u64;
                let desc = super::objops::__rtsadp_obj_get_own_property_descriptor(obj_word, k);
                let dv = PolyValue::from_raw(desc);
                let keep = dv.is_object()
                    && PolyValue::from_raw(super::objops::__rtsadp_obj_get(desc, enum_key))
                        .is_truthy();
                if keep {
                    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, k as i64);
                }
            }
            return PolyValue::from_object_handle(
                rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(out),
            )
            .raw();
        }
        return __rtsadp_obj_keys(target);
    }
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
            for k in reorder_enum_keys(keys) {
                let word = abi_adapter::intern_poly(&k).raw() as i64;
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
            }
        }
    } else if obj.is_object() {
        // ARRAY receiver: `Object.keys([a, b, c])` is the STRING indices
        // `["0", "1", "2"]` (JS treats an array as an object whose own enumerable
        // keys are its indices). The element count is the backing Vec's length.
        let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(handle).max(0);
        for i in 0..len {
            let word = abi_adapter::intern_poly(&i.to_string()).raw() as i64;
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
        }
    }
    box_vec_as_array(vec)
}

/// `Object.getOwnPropertyNames(x)` — like [`__rtsadp_obj_keys`] but includes the
/// NON-enumerable own properties JS exposes here: for an ARRAY, the trailing
/// `"length"` (`getOwnPropertyNames([a,b,c])` is `["0","1","2","length"]`). For a
/// keyed object it equals `Object.keys` (the corpus has no non-enumerable own
/// props on plain objects). Reuses `__rtsadp_obj_keys` then appends `"length"`
/// when the receiver is an array (object, not a keyed shape).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_own_names(obj_word: u64) -> u64 {
    let keys_arr = __rtsadp_obj_keys(obj_word);
    let obj = PolyValue::from_raw(obj_word);
    if obj.is_object() && !looks_like_object(obj) {
        // ARRAY: append the own non-enumerable "length".
        let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(
            PolyValue::from_raw(keys_arr).as_handle(),
        );
        let word = abi_adapter::intern_poly("length").raw() as i64;
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle, word);
    }
    keys_arr
}
