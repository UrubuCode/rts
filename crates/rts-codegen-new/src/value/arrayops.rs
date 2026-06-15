//! Codegen-owned, PolyValue-aware Array instance-method trampolines (P4.5).
//!
//! The new engine OWNS its array element interpretation: an array is a REAL
//! `Entry::Vec` whose i64 slots each hold a raw [`PolyValue`] WORD (see P3 in
//! [`crate::front::run::obj`]). The runtime's own `__RTS_FN_NS_COLLECTIONS_VEC_*`
//! Array methods read those slots as plain i64 elements and would interpret the
//! NaN-boxed words wrong (a boxed int32 word `!=` a stored raw i64), so Array
//! method semantics CANNOT be delegated to them directly.
//!
//! Instead — exactly like the generic operators in [`super::genops`] — these are
//! codegen-owned `__rtsadp_*` trampolines (NOT `__RTS_FN_*`): each takes the
//! array's REAL Vec handle (a `u64`) plus PolyValue-word args, reads each element
//! through the REAL `VEC_GET`/`VEC_LEN` (so the storage stays the runtime's), and
//! interprets every slot as a [`PolyValue`] using the SAME semantics as the
//! generic operators ([`super::genops`] strict-eq / ToString). No fake store, no
//! divergent reimplementation: equality reuses the strict-eq path, ToString reuses
//! the genops path, strings/joins go through the REAL pool.
//!
//! Convention: PolyValue words cross as raw `u64`; indices/lengths cross as `i64`;
//! string args/results cross as real string handles (`u64`). A returned array is a
//! fresh `Entry::Vec` boxed as a `TAG_OBJECT` PolyValue word.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::{PolyValue, abi_adapter, genops};

/// Element count of the real Vec behind `vec_handle` (clamped to `>= 0`).
fn vec_len(vec_handle: u64) -> i64 {
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(vec_handle).max(0)
}

/// Read slot `i` of the real Vec as a raw PolyValue word. Caller guarantees
/// `0 <= i < len` (we never call it out of range).
fn vec_word(vec_handle: u64, i: i64) -> u64 {
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(vec_handle, i) as u64
}

/// JS strict-equality between two PolyValue words, reusing the EXACT genops
/// `__rtsadp_strict_eq` semantics (number cross-representation, string-by-content
/// via the real pool, identical-representation otherwise). Never a divergent
/// reimplementation — this calls the same function `===` lowers to.
fn words_strict_eq(a: u64, b: u64) -> bool {
    PolyValue::from_raw(genops::__rtsadp_strict_eq(a, b)).as_bool()
}

/// `arr.indexOf(needle)` — first index whose element `=== needle`, or `-1`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_index_of(vec_handle: u64, needle_word: u64) -> i64 {
    let len = vec_len(vec_handle);
    for i in 0..len {
        if words_strict_eq(vec_word(vec_handle, i), needle_word) {
            return i;
        }
    }
    -1
}

/// `arr.includes(needle)` — `1` if any element `=== needle`, else `0`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_includes(vec_handle: u64, needle_word: u64) -> i64 {
    (__rtsadp_arr_index_of(vec_handle, needle_word) >= 0) as i64
}

/// `arr.at(i)` — the element PolyValue word at `i` (negative `i` counts from the
/// end: `len + i`); out of range yields the raw `undefined` PolyValue word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_at(vec_handle: u64, i: i64) -> u64 {
    let len = vec_len(vec_handle);
    let idx = if i < 0 { len + i } else { i };
    if idx < 0 || idx >= len {
        return PolyValue::undefined().raw();
    }
    vec_word(vec_handle, idx)
}

/// `arr.join(sep)` — ToString every element (the SAME genops ToString used by
/// `console.log`/`__rtsadp_to_string`), join with `sep` (a real string handle),
/// and intern the result in the REAL string pool, returning its handle. An empty
/// array yields the interned empty string.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_join(vec_handle: u64, sep_str_handle: u64) -> u64 {
    let sep = abi_adapter::real_handle_to_string(sep_str_handle);
    let len = vec_len(vec_handle);
    let mut out = String::new();
    for i in 0..len {
        if i > 0 {
            out.push_str(&sep);
        }
        // ToString each element through the genops path (string already its own
        // ToString; numbers via the real STRING_FROM_F64 formatting; etc).
        let word = vec_word(vec_handle, i);
        let s_word = genops::__rtsadp_to_string(word);
        out.push_str(&abi_adapter::resolve_poly(PolyValue::from_raw(s_word)));
    }
    abi_adapter::intern_poly(&out).as_handle_real()
}

/// `arr.push(val)` — append the raw PolyValue word to the real Vec; return the
/// new length.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_push(vec_handle: u64, val_word: u64) -> i64 {
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec_handle, val_word as i64);
    vec_len(vec_handle)
}

/// `arr.pop()` — remove and return the last element PolyValue word; `undefined`
/// (raw word) when the array is empty.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_pop(vec_handle: u64) -> u64 {
    if vec_len(vec_handle) == 0 {
        return PolyValue::undefined().raw();
    }
    // The Vec stores raw PolyValue words; VEC_POP returns the slot i64 verbatim
    // (the empty-sentinel branch is excluded by the length check above).
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_POP(vec_handle) as u64
}

/// `arr.slice(start, end)` — a NEW array (fresh `Entry::Vec`) with the elements in
/// `[start, end)` under JS negative-index / clamp semantics; returned as a
/// `TAG_OBJECT` PolyValue word of the new Vec.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_slice(vec_handle: u64, start: i64, end: i64) -> u64 {
    let len = vec_len(vec_handle);
    let s = clamp_index(start, len);
    let e = clamp_index(end, len).max(s);
    let new_vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    for i in s..e {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(new_vec, vec_word(vec_handle, i) as i64);
    }
    // Box the new Vec handle as a TAG_OBJECT PolyValue (the array representation).
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(new_vec)).raw()
}

/// JS slice index clamp: negative counts from the end (`len + i`), then clamp into
/// `[0, len]`.
fn clamp_index(i: i64, len: i64) -> i64 {
    let idx = if i < 0 { len + i } else { i };
    idx.clamp(0, len)
}

/// `arr.lastIndexOf(needle)` — last index whose element `=== needle`, or `-1`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_last_index_of(vec_handle: u64, needle_word: u64) -> i64 {
    let len = vec_len(vec_handle);
    for i in (0..len).rev() {
        if words_strict_eq(vec_word(vec_handle, i), needle_word) {
            return i;
        }
    }
    -1
}

/// `arr.reverse()` — reverse the array IN PLACE (JS mutates the receiver) and
/// return its TAG_OBJECT word so chaining (`a.reverse().join(",")`) works. Swaps
/// the boxed slot words via the REAL `VEC_GET`/`VEC_SET`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_reverse(vec_handle: u64) -> u64 {
    let len = vec_len(vec_handle);
    let mut lo = 0i64;
    let mut hi = len - 1;
    while lo < hi {
        let a = vec_word(vec_handle, lo);
        let b = vec_word(vec_handle, hi);
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(vec_handle, lo, b as i64);
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(vec_handle, hi, a as i64);
        lo += 1;
        hi -= 1;
    }
    box_self(vec_handle)
}

/// `arr.fill(value)` — overwrite every slot with the raw PolyValue word `value`
/// (the whole-array form; the start/end range form is a later increment, bailed at
/// the lowering by arity). Mutates in place; returns the receiver's word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_fill(vec_handle: u64, value_word: u64) -> u64 {
    let len = vec_len(vec_handle);
    for i in 0..len {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(vec_handle, i, value_word as i64);
    }
    box_self(vec_handle)
}

/// `arr.concat(other)` — a NEW array (fresh `Entry::Vec`) with this array's
/// elements followed by `other`'s, as boxed PolyValue words. `other_word` is the
/// raw PolyValue word of the second array; if it is NOT an array word, it is
/// appended as a single element (JS `[1].concat(2)` → `[1, 2]`). Returns the new
/// array word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_concat(vec_handle: u64, other_word: u64) -> u64 {
    let new_vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let len = vec_len(vec_handle);
    for i in 0..len {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(new_vec, vec_word(vec_handle, i) as i64);
    }
    let other = PolyValue::from_raw(other_word);
    if other.is_object() && !super::inspect::looks_like_object(other) {
        let oh = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(other.as_handle());
        let olen = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(oh).max(0);
        for i in 0..olen {
            let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(oh, i);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(new_vec, w);
        }
    } else {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(new_vec, other_word as i64);
    }
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(new_vec)).raw()
}

/// `arr.flat()` — flatten ONE level (the JS default depth 1): a NEW array with each
/// element that is itself an array spliced in, non-array elements copied verbatim.
/// Deeper flattening (`flat(2)`) is a later increment (bailed by arity).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_flat(vec_handle: u64) -> u64 {
    let new_vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let len = vec_len(vec_handle);
    for i in 0..len {
        let w = vec_word(vec_handle, i);
        let ev = PolyValue::from_raw(w);
        if ev.is_object() && !super::inspect::looks_like_object(ev) {
            let inner = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(ev.as_handle());
            let ilen = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(inner).max(0);
            for j in 0..ilen {
                let iw = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(inner, j);
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(new_vec, iw);
            }
        } else {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(new_vec, w as i64);
        }
    }
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(new_vec)).raw()
}

/// `arr.shift()` — remove and return the FIRST element word (`undefined` when
/// empty), shifting the rest down one slot. Mutates in place.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_shift(vec_handle: u64) -> u64 {
    let len = vec_len(vec_handle);
    if len == 0 {
        return PolyValue::undefined().raw();
    }
    let first = vec_word(vec_handle, 0);
    for i in 1..len {
        let w = vec_word(vec_handle, i);
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(vec_handle, i - 1, w as i64);
    }
    // Drop the now-duplicated last slot via POP (its value moved down already).
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_POP(vec_handle);
    first
}

/// `arr.unshift(value)` — prepend the raw PolyValue word `value` (the single-arg
/// form), shifting existing elements up one slot; return the new length. Mutates
/// in place.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_unshift(vec_handle: u64, value_word: u64) -> i64 {
    let len = vec_len(vec_handle);
    // Grow by one (append a placeholder), then shift everything up.
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec_handle, PolyValue::undefined().raw() as i64);
    for i in (0..len).rev() {
        let w = vec_word(vec_handle, i);
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(vec_handle, i + 1, w as i64);
    }
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(vec_handle, 0, value_word as i64);
    len + 1
}

/// The receiver array's own TAG_OBJECT word (reconstructed from its Vec handle),
/// returned by the in-place mutating methods so chaining works.
fn box_self(vec_handle: u64) -> u64 {
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(vec_handle)).raw()
}

impl PolyValue {
    /// The REAL runtime string handle behind a string PolyValue (generation
    /// reconstructed from the live slot) — used to return a real handle from the
    /// trampolines whose result is a string the lowering re-boxes as a string.
    fn as_handle_real(self) -> u64 {
        abi_adapter::real_handle_of(self)
    }
}
