//! Codegen-owned, PolyValue-aware Array CALLBACK trampolines (P4.7).
//!
//! Builds on P4.5 ([`super::arrayops`]) and P4.6 ([`super::funcops`]): the array
//! methods that take a callback function VALUE (`map`/`filter`/`forEach`/`find`/
//! `findIndex`/`some`/`every`/`reduce`) loop over the REAL `Entry::Vec` behind
//! the array's handle and invoke the callback per element through the FIXED
//! uniform indirect-call ABI ([`super::funcops::__rtsadp_fn_invoke`]).
//!
//! The JS callback signature is `(element, index, array)` for the predicate/map
//! family and `(accumulator, element, index, array)` for `reduce`. We marshal
//! them onto the uniform 5-slot ABI `fn(a0,a1,a2,a3, rest)`:
//!
//! - map/filter/forEach/find/findIndex/some/every:
//!   `a0 = element word`, `a1 = PolyValue::from_i32(index)`, `a2 = the array
//!   TAG_OBJECT word`, `a3 = undefined`, `rest = undefined`.
//! - reduce: `a0 = accumulator word`, `a1 = element word`, `a2 =
//!   PolyValue::from_i32(index)`, `a3 = the array TAG_OBJECT word`, `rest =
//!   undefined`.
//!
//! Truthiness reuses [`super::genops::__rtsadp_to_boolean`] (the ONE JS
//! `ToBoolean`, resolving the empty-string case on the heap). Element words are
//! read through the REAL `VEC_GET`/`VEC_LEN` (storage stays the runtime's), and
//! produced arrays are fresh `Entry::Vec`s boxed as `TAG_OBJECT` words — exactly
//! like [`super::arrayops`]. Nothing here reimplements equality/ToString/storage.
//!
//! ## Reentrancy (no shard lock held across an invoke)
//!
//! Each `VEC_GET`/`VEC_LEN`/`VEC_PUSH` is its OWN extern call that acquires and
//! releases the HandleTable shard lock internally before returning. We read the
//! element word out (the `VEC_GET` returns, releasing the lock), THEN call the
//! callback — so no shard lock is ever held across `__rtsadp_fn_invoke` (which
//! reenters JIT'd code that may itself touch the HandleTable). The output array
//! of `map`/`filter` is a DISTINCT Vec handle, so its `VEC_PUSH` also takes a
//! different shard's lock — no self-deadlock.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::{PolyValue, funcops, genops};

/// Element count of the real Vec behind `vec_handle` (clamped to `>= 0`).
fn vec_len(vec_handle: u64) -> i64 {
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(vec_handle).max(0)
}

/// Read slot `i` of the real Vec as a raw PolyValue word. The `VEC_GET` releases
/// the shard lock before returning, so the word is safe to feed to a callback.
fn vec_word(vec_handle: u64, i: i64) -> u64 {
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(vec_handle, i) as u64
}

/// Append a raw PolyValue word to the real Vec behind `vec_handle`.
fn vec_push(vec_handle: u64, word: u64) {
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec_handle, word as i64);
}

/// The array's own value word (a `TAG_OBJECT` PolyValue over its Vec) — the third
/// arg JS passes to the callback (`(element, index, array)`). Reconstructed from
/// the Vec handle via the bare-payload bridge (`POLY_FROM_HANDLE`) + the OBJECT
/// header, matching how the lowering boxed it in the first place.
fn array_word(vec_handle: u64) -> u64 {
    let payload = rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(vec_handle);
    PolyValue::from_object_handle(payload & super::PAYLOAD_MASK).raw()
}

/// The `undefined` PolyValue word (the unused `a3`/`rest` slots).
#[inline]
fn undef() -> u64 {
    PolyValue::undefined().raw()
}

/// The index passed to a callback, boxed as a PolyValue. A numeric callback
/// param (`i: number`) is unboxed by the thunk as a DOUBLE (`bitcast i64→f64`),
/// so the index must be a boxed *double*, not a tagged int32 — otherwise the
/// thunk reinterprets the int32-boxed NaN-word as a garbage float. (A `Tagged`
/// param keeps the word verbatim, for which either boxing reads back correctly.)
#[inline]
fn index_word(index: i64) -> u64 {
    PolyValue::from_f64(index as f64).raw()
}

/// Invoke the `(element, index, array)` callback for one element: `a0 = element`,
/// `a1 = index`, `a2 = array`, `a3 = undefined`, `rest = undefined`. No shard
/// lock is held here (the element word was already read out).
fn invoke_eia(cb: u64, element: u64, index: i64, arr_word: u64) -> u64 {
    funcops::__rtsadp_fn_invoke(cb, element, index_word(index), arr_word, undef(), undef())
}

/// JS truthiness of a PolyValue word, reusing the genops `ToBoolean` authority.
fn truthy(word: u64) -> bool {
    genops::__rtsadp_to_boolean(word) != 0
}

/// `arr.map(cb)` — a NEW array of `cb(element, index, array)` for each element,
/// returned as a `TAG_OBJECT` PolyValue word of the fresh Vec.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_map(vec_handle: u64, cb: u64) -> u64 {
    let len = vec_len(vec_handle);
    let arr_word = array_word(vec_handle);
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    for i in 0..len {
        let element = vec_word(vec_handle, i);
        let mapped = invoke_eia(cb, element, i, arr_word);
        vec_push(out, mapped);
    }
    array_word(out)
}

/// `arr.filter(cb)` — a NEW array of the elements for which `cb(element, index,
/// array)` is truthy. Returned as a `TAG_OBJECT` PolyValue word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_filter(vec_handle: u64, cb: u64) -> u64 {
    let len = vec_len(vec_handle);
    let arr_word = array_word(vec_handle);
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    for i in 0..len {
        let element = vec_word(vec_handle, i);
        if truthy(invoke_eia(cb, element, i, arr_word)) {
            vec_push(out, element);
        }
    }
    array_word(out)
}

/// `arr.forEach(cb)` — invoke `cb(element, index, array)` for each element,
/// discarding the result; returns `undefined` (forEach's JS result).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_for_each(vec_handle: u64, cb: u64) -> u64 {
    let len = vec_len(vec_handle);
    let arr_word = array_word(vec_handle);
    for i in 0..len {
        let element = vec_word(vec_handle, i);
        let _ = invoke_eia(cb, element, i, arr_word);
    }
    undef()
}

/// `arr.find(cb)` — the FIRST element for which `cb(element, index, array)` is
/// truthy, else `undefined` (raw PolyValue word).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_find(vec_handle: u64, cb: u64) -> u64 {
    let len = vec_len(vec_handle);
    let arr_word = array_word(vec_handle);
    for i in 0..len {
        let element = vec_word(vec_handle, i);
        if truthy(invoke_eia(cb, element, i, arr_word)) {
            return element;
        }
    }
    undef()
}

/// `arr.findIndex(cb)` — the FIRST index whose element satisfies the predicate,
/// else `-1`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_find_index(vec_handle: u64, cb: u64) -> i64 {
    let len = vec_len(vec_handle);
    let arr_word = array_word(vec_handle);
    for i in 0..len {
        let element = vec_word(vec_handle, i);
        if truthy(invoke_eia(cb, element, i, arr_word)) {
            return i;
        }
    }
    -1
}

/// `arr.some(cb)` — `1` if the predicate is truthy for ANY element, else `0`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_some(vec_handle: u64, cb: u64) -> i64 {
    let len = vec_len(vec_handle);
    let arr_word = array_word(vec_handle);
    for i in 0..len {
        let element = vec_word(vec_handle, i);
        if truthy(invoke_eia(cb, element, i, arr_word)) {
            return 1;
        }
    }
    0
}

/// `arr.every(cb)` — `1` if the predicate is truthy for EVERY element (vacuously
/// `1` for an empty array), else `0`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_every(vec_handle: u64, cb: u64) -> i64 {
    let len = vec_len(vec_handle);
    let arr_word = array_word(vec_handle);
    for i in 0..len {
        let element = vec_word(vec_handle, i);
        if !truthy(invoke_eia(cb, element, i, arr_word)) {
            return 0;
        }
    }
    1
}

/// `arr.reduce(cb, init?, hasInit)` — fold with the reducer `cb(accumulator,
/// element, index, array)`. `a0 = acc`, `a1 = element`, `a2 = index int`, `a3 =
/// array`, `rest = undefined`.
///
/// - `has_init != 0`: start `acc = init_word`, iterate from index 0.
/// - `has_init == 0`: start `acc = element[0]`, iterate from index 1. An EMPTY
///   array with no initial value is a JS `TypeError`; the new engine has no
///   exception channel here yet, so we return `undefined` (documented bail
///   value, never a wrong fold) — the only honest no-throw answer.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_reduce(
    vec_handle: u64,
    cb: u64,
    init_word: u64,
    has_init: i64,
) -> u64 {
    let len = vec_len(vec_handle);
    let arr_word = array_word(vec_handle);

    let (mut acc, start) = if has_init != 0 {
        (init_word, 0)
    } else {
        if len == 0 {
            // Reduce of empty array with no initial value → TypeError in JS; we
            // have no throw channel here, so return undefined (documented).
            return undef();
        }
        (vec_word(vec_handle, 0), 1)
    };

    for i in start..len {
        let element = vec_word(vec_handle, i);
        acc = funcops::__rtsadp_fn_invoke(cb, acc, element, index_word(i), arr_word, undef());
    }
    acc
}

/// `arr.findLast(cb)` — the LAST element (scanning from the end) whose predicate is
/// truthy, else `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_find_last(vec_handle: u64, cb: u64) -> u64 {
    let len = vec_len(vec_handle);
    let arr_word = array_word(vec_handle);
    for i in (0..len).rev() {
        let element = vec_word(vec_handle, i);
        if truthy(invoke_eia(cb, element, i, arr_word)) {
            return element;
        }
    }
    undef()
}

/// `arr.findLastIndex(cb)` — the LAST index whose element satisfies the predicate,
/// else `-1`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_find_last_index(vec_handle: u64, cb: u64) -> i64 {
    let len = vec_len(vec_handle);
    let arr_word = array_word(vec_handle);
    for i in (0..len).rev() {
        let element = vec_word(vec_handle, i);
        if truthy(invoke_eia(cb, element, i, arr_word)) {
            return i;
        }
    }
    -1
}

/// `arr.reduceRight(cb, init?, hasInit)` — fold from the END. Same ABI/empty-array
/// semantics as [`__rtsadp_arr_reduce`], iterating indices high→low.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_reduce_right(
    vec_handle: u64,
    cb: u64,
    init_word: u64,
    has_init: i64,
) -> u64 {
    let len = vec_len(vec_handle);
    let arr_word = array_word(vec_handle);

    let (mut acc, start) = if has_init != 0 {
        (init_word, len - 1)
    } else {
        if len == 0 {
            return undef();
        }
        (vec_word(vec_handle, len - 1), len - 2)
    };

    let mut i = start;
    while i >= 0 {
        let element = vec_word(vec_handle, i);
        acc = funcops::__rtsadp_fn_invoke(cb, acc, element, index_word(i), arr_word, undef());
        i -= 1;
    }
    acc
}

/// `arr.flatMap(cb)` — `cb(element, index, array)` per element, then flatten ONE
/// level: an array result is spliced in, a non-array result pushed verbatim. A NEW
/// array word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_flat_map(vec_handle: u64, cb: u64) -> u64 {
    let len = vec_len(vec_handle);
    let arr_word = array_word(vec_handle);
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    for i in 0..len {
        let element = vec_word(vec_handle, i);
        let r = invoke_eia(cb, element, i, arr_word);
        let rv = PolyValue::from_raw(r);
        if rv.is_object() && !super::inspect::looks_like_object(rv) {
            let inner = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(rv.as_handle());
            let ilen = vec_len(inner);
            for j in 0..ilen {
                vec_push(out, vec_word(inner, j));
            }
        } else {
            vec_push(out, r);
        }
    }
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(out) & super::PAYLOAD_MASK).raw()
}
