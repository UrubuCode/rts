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
use rts_runtime::namespaces::gc::string_pool as rt_str;

use super::{PolyValue, abi_adapter, funcops, genops};

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
/// `arr.join()` with no argument — `arr.join(",")` (the JS default separator).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_join0(vec_handle: u64) -> u64 {
    let comma = abi_adapter::intern_poly(",").as_handle_real();
    __rtsadp_arr_join(vec_handle, comma)
}

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

/// A fresh `Entry::Vec` that is a COPY of `vec_handle`'s slot words (the basis of
/// the non-mutating ES2023 methods `toReversed`/`toSorted`/`with`).
fn copy_vec(vec_handle: u64) -> u64 {
    let new_vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let len = vec_len(vec_handle);
    for i in 0..len {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(new_vec, vec_word(vec_handle, i) as i64);
    }
    new_vec
}

/// `arr.slice(start)` — the one-arg form (`slice(start, len)`): elements from
/// `start` (JS negative-index/clamp) to the end, as a NEW array word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_slice1(vec_handle: u64, start: i64) -> u64 {
    __rtsadp_arr_slice(vec_handle, start, vec_len(vec_handle))
}

/// `arr.slice()` (no args) — a shallow COPY of the whole array (`slice(0, len)`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_slice0(vec_handle: u64) -> u64 {
    __rtsadp_arr_slice(vec_handle, 0, vec_len(vec_handle))
}

/// `arr.toString()` — JS defines it as `arr.join(",")`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_to_string(vec_handle: u64) -> u64 {
    __rtsadp_arr_join0(vec_handle)
}

/// `arr.indexOf(needle, fromIndex)` — first index `>= fromIndex` whose element
/// `=== needle`, or `-1`. Negative `from` counts from the end (`len + from`,
/// clamped to `0`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_index_of_from(vec_handle: u64, needle_word: u64, from: i64) -> i64 {
    let len = vec_len(vec_handle);
    let start = if from < 0 { (len + from).max(0) } else { from };
    for i in start..len {
        if words_strict_eq(vec_word(vec_handle, i), needle_word) {
            return i;
        }
    }
    -1
}

/// `arr.includes(needle, fromIndex)` — `1` iff `indexOf(needle, fromIndex) >= 0`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_includes_from(vec_handle: u64, needle_word: u64, from: i64) -> i64 {
    (__rtsadp_arr_index_of_from(vec_handle, needle_word, from) >= 0) as i64
}

/// `arr.lastIndexOf(needle, fromIndex)` — last index `<= fromIndex` whose element
/// `=== needle`, or `-1`. Negative `from` counts from the end; the scan starts at
/// `min(from-normalized, len-1)` and walks down.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_last_index_of_from(
    vec_handle: u64,
    needle_word: u64,
    from: i64,
) -> i64 {
    let len = vec_len(vec_handle);
    if len == 0 {
        return -1;
    }
    let raw_start = if from < 0 { len + from } else { from };
    let start = raw_start.min(len - 1);
    for i in (0..=start.max(-1)).rev() {
        if i < 0 {
            break;
        }
        if words_strict_eq(vec_word(vec_handle, i), needle_word) {
            return i;
        }
    }
    -1
}

/// `arr.toReversed()` (ES2023) — a NEW reversed array; the receiver is UNCHANGED
/// (unlike `reverse()`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_to_reversed(vec_handle: u64) -> u64 {
    let copy = copy_vec(vec_handle);
    __rtsadp_arr_reverse(copy);
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(copy)).raw()
}

/// `arr.with(index, value)` (ES2023) — a NEW array equal to the receiver but with
/// slot `index` replaced by `value` (negative index counts from the end). An
/// out-of-range index would throw `RangeError` in JS; here it returns the copy
/// unchanged (throw is a later increment). The receiver is UNCHANGED.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_with(vec_handle: u64, index: i64, value_word: u64) -> u64 {
    let copy = copy_vec(vec_handle);
    let len = vec_len(copy);
    let idx = if index < 0 { len + index } else { index };
    if idx >= 0 && idx < len {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(copy, idx, value_word as i64);
    }
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(copy)).raw()
}

/// `arr.flat(depth)` — flatten nested arrays up to `depth` levels into a NEW array
/// (JS default depth is 1; `flat()`-arity-0 stays the dedicated single-level path).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_flat_depth(vec_handle: u64, depth: i64) -> u64 {
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    flat_into(vec_handle, depth, out);
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(out)).raw()
}

/// Push `vec_handle`'s elements into `out`, splicing array elements recursively
/// while `depth > 0`. A non-array element (or any element at `depth == 0`) is
/// pushed verbatim.
fn flat_into(vec_handle: u64, depth: i64, out: u64) {
    let len = vec_len(vec_handle);
    for i in 0..len {
        let w = vec_word(vec_handle, i);
        let ev = PolyValue::from_raw(w);
        if depth > 0 && ev.is_object() && !super::inspect::looks_like_object(ev) {
            let inner = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(ev.as_handle());
            flat_into(inner, depth - 1, out);
        } else {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, w as i64);
        }
    }
}

/// JS default array sort ORDER between two element words: compare their `ToString`
/// forms by UTF-8 code units (the spec's default is the string comparison; for the
/// numeric arrays in the suite this matches bun/node's default-sort output). Reuses
/// the genops `ToString` + the real pool `STRING_CMP`.
fn default_sort_cmp(a: u64, b: u64) -> std::cmp::Ordering {
    let sa = PolyValue::from_raw(genops::__rtsadp_to_string(a)).as_handle_real();
    let sb = PolyValue::from_raw(genops::__rtsadp_to_string(b)).as_handle_real();
    match rt_str::__RTS_FN_NS_GC_STRING_CMP(sa, sb) {
        n if n < 0 => std::cmp::Ordering::Less,
        0 => std::cmp::Ordering::Equal,
        _ => std::cmp::Ordering::Greater,
    }
}

/// `arr.sort()` — DEFAULT comparator (ToString ascending), IN PLACE; returns the
/// receiver word. The comparator-callback form (`sort(cmp)`) is a callback method
/// handled elsewhere (bails until implemented).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_sort(vec_handle: u64) -> u64 {
    let len = vec_len(vec_handle);
    let mut words: Vec<u64> = (0..len).map(|i| vec_word(vec_handle, i)).collect();
    words.sort_by(|&a, &b| default_sort_cmp(a, b));
    for (i, w) in words.into_iter().enumerate() {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(vec_handle, i as i64, w as i64);
    }
    box_self(vec_handle)
}

/// `arr.sort(cmp)` — sort IN PLACE using the JS comparator `cmp(a, b)`: a returned
/// number < 0 keeps `a` before `b`, > 0 puts `b` first, 0 (or NaN) treats them as
/// equal. The comparator is a `TAG_FUNCTION` word invoked through the uniform
/// indirect-call ABI. Returns the (mutated) receiver. Mirrors `__rtsadp_arr_sort`
/// but with the user comparator instead of `default_sort_cmp`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_sort_cmp(vec_handle: u64, cb: u64) -> u64 {
    let len = vec_len(vec_handle);
    let mut words: Vec<u64> = (0..len).map(|i| vec_word(vec_handle, i)).collect();
    let u = PolyValue::undefined().raw();
    words.sort_by(|&a, &b| {
        // `cmp(a, b)` → a PolyValue word; ToNumber it (JS coercion). A NaN/0 result
        // is `Equal` (a stable no-swap), matching the spec's "treat as equal".
        let r = genops::dyn_to_number(funcops::__rtsadp_fn_invoke(cb, a, b, u, u, u));
        if r < 0.0 {
            std::cmp::Ordering::Less
        } else if r > 0.0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    for (i, w) in words.into_iter().enumerate() {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(vec_handle, i as i64, w as i64);
    }
    box_self(vec_handle)
}

/// `arr.toSorted()` (ES2023) — a NEW array sorted by the default comparator; the
/// receiver is UNCHANGED.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_to_sorted(vec_handle: u64) -> u64 {
    let copy = copy_vec(vec_handle);
    __rtsadp_arr_sort(copy);
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(copy)).raw()
}

/// `arr.toSorted(cmp)` (ES2023) — a NEW array sorted by the user comparator; the
/// receiver is UNCHANGED. Copy then `__rtsadp_arr_sort_cmp` on the copy.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_to_sorted_cmp(vec_handle: u64, cb: u64) -> u64 {
    let copy = copy_vec(vec_handle);
    __rtsadp_arr_sort_cmp(copy, cb);
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(copy)).raw()
}

/// `arr.toSpliced(start, deleteCount)` (ES2023, no-insert 2-arg form) — a NEW array
/// with `deleteCount` elements removed at `start` (JS negative-index/clamp). The
/// receiver is UNCHANGED. The insert form (`toSpliced(s, d, ...items)`) is variadic
/// and a later increment (bails by arity).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_to_spliced(vec_handle: u64, start: i64, delete_count: i64) -> u64 {
    let len = vec_len(vec_handle);
    let s = clamp_index(start, len);
    let del = delete_count.clamp(0, len - s);
    let new_vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    for i in 0..s {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(new_vec, vec_word(vec_handle, i) as i64);
    }
    for i in (s + del)..len {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(new_vec, vec_word(vec_handle, i) as i64);
    }
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(new_vec)).raw()
}

/// `arr.splice(start, deleteCount?, ...items)` (MUTATING) — remove `deleteCount`
/// elements at `start` and insert `items`, returning a NEW array of the removed
/// elements. `args_word` is a TAG_OBJECT array word holding `[start, deleteCount?,
/// ...items]` (the lowering packs the variadic args). Delegates to the runtime
/// `VEC_SPLICE_AUTO` (both the receiver and the args ride real Vec handles; the
/// element words are raw `i64` PolyValue words on both sides).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_splice(vec_handle: u64, args_handle: u64) -> u64 {
    // Both `vec_handle` and `args_handle` are REAL Vec handles (the lowering
    // table-loads the receiver and the packed args array before the call).
    let removed = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_AUTO(vec_handle, args_handle);
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(removed)).raw()
}

/// `arr.toSpliced(start, deleteCount?, ...items)` (ES2023, NON-mutating, variadic)
/// — like `splice` but returns a NEW array with the splice applied and leaves the
/// receiver unchanged. Same packed-args convention; delegates to the runtime
/// `VEC_TO_SPLICED_AUTO`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_to_spliced_var(vec_handle: u64, args_handle: u64) -> u64 {
    let result = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED_AUTO(vec_handle, args_handle);
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(result)).raw()
}

/// `arr.copyWithin(target, start, end)` — copy the slice `[start, end)` to `target`,
/// IN PLACE, returning the receiver word (JS clamps all three; the copy is shift-
/// safe via a snapshot of the source range). Arity-2 (`copyWithin(target, start)`)
/// passes `end = len` from the lowering's default.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_copy_within(
    vec_handle: u64,
    target: i64,
    start: i64,
    end: i64,
) -> u64 {
    let len = vec_len(vec_handle);
    let t = clamp_index(target, len);
    let s = clamp_index(start, len);
    let e = clamp_index(end, len).max(s);
    // Snapshot the source range so an overlapping copy stays correct.
    let src: Vec<u64> = (s..e).map(|i| vec_word(vec_handle, i)).collect();
    for (k, w) in src.into_iter().enumerate() {
        let dst = t + k as i64;
        if dst >= len {
            break;
        }
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(vec_handle, dst, w as i64);
    }
    box_self(vec_handle)
}

/// `arr.copyWithin(target, start)` — the 2-arg form (`end = len`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_copy_within2(vec_handle: u64, target: i64, start: i64) -> u64 {
    __rtsadp_arr_copy_within(vec_handle, target, start, vec_len(vec_handle))
}

impl PolyValue {
    /// The REAL runtime string handle behind a string PolyValue (generation
    /// reconstructed from the live slot) — used to return a real handle from the
    /// trampolines whose result is a string the lowering re-boxes as a string.
    fn as_handle_real(self) -> u64 {
        abi_adapter::real_handle_of(self)
    }
}
