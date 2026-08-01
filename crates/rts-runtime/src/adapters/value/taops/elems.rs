//! The TypedArray-only instance surface: `set`, `subarray`, the iteration
//! methods (`values`/`keys`/`entries`) and the view accessors
//! (`buffer`/`byteOffset`/`byteLength`).
//!
//! Each entry point handles BOTH representations — a level-B [`View`] receiver
//! reads and writes through its window, anything else falls back to the
//! Vec-backed path.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::super::PolyValue;
use super::super::genops;
use super::view::view_parts;

/// `offset` (default 0). Word-level copy (the level-A Vec backing does not
/// re-wrap; the tests write in-range values). Returns `undefined`.
#[rtse::abi]
pub fn rtsadp_arr_ta_set(arr_word: u64, src_word: u64, off_word: u64) -> u64 {
    let a = PolyValue::from_raw(arr_word);
    let s = PolyValue::from_raw(src_word);
    // Level-B VIEW receiver: write each source element through the shared
    // buffer (`TA_SET_ELEM`), so sibling views observe it.
    if let Some(dst) = view_parts(arr_word) {
        if s.is_object() {
            let off = genops::to_number(PolyValue::from_raw(off_word));
            let off = if off.is_finite() && off > 0.0 { off as i64 } else { 0 };
            // The SOURCE may be a view too (`view.set(otherView)`); read it
            // through its own window, not its header slots.
            if let Some(src) = view_parts(src_word) {
                for i in 0..src.count {
                    dst.set(off + i, src.get(i));
                }
            } else {
                let sh = rt_handles::__rtsn_poly_to_handle(s.as_handle());
                let slen = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(sh).max(0);
                for i in 0..slen {
                    let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(sh, i) as u64;
                    dst.set(off + i, w);
                }
            }
        }
        return PolyValue::undefined().raw();
    }
    if a.is_object() && s.is_object() {
        let ah = rt_handles::__rtsn_poly_to_handle(a.as_handle());
        let sh = rt_handles::__rtsn_poly_to_handle(s.as_handle());
        let off = genops::to_number(PolyValue::from_raw(off_word));
        let off = if off.is_finite() && off > 0.0 { off as i64 } else { 0 };
        let slen = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(sh).max(0);
        let alen = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(ah).max(0);
        for i in 0..slen {
            if off + i >= alen {
                break;
            }
            let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(sh, i);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(ah, off + i, w);
        }
    }
    PolyValue::undefined().raw()
}


/// 1-arg form `ta.set(src)` — offset 0.
#[rtse::abi]
pub fn rtsadp_arr_ta_set1(arr_word: u64, src_word: u64) -> u64 {
    __rtsadp_arr_ta_set(arr_word, src_word, PolyValue::undefined().raw())
}

/// 1-arg form `ta.subarray(begin)` — to the end.
#[rtse::abi]
pub fn rtsadp_arr_subarray1(arr_word: u64, begin_word: u64) -> u64 {
    __rtsadp_arr_subarray(arr_word, begin_word, PolyValue::undefined().raw())
}

/// `ta.subarray(begin?, end?)` — the range of the receiver, as the level-A COPY
/// of that range. Negative indices count from the end, like `slice`.
///
/// A level-B VIEW receiver reads through its WINDOW here. Without this arm the
/// code below walked the view's backing Vec, whose slots are the view HEADER,
/// not elements — `new Uint8Array(new ArrayBuffer(8)).subarray(2)` reported
/// `length === 3` (the header word count minus 2) instead of 6, and every
/// element read was a header word.
///
/// DIVERGENCE (deliberate): JS returns a live sub-VIEW that shares the buffer,
/// so `w.subarray(2)[0] = 77` is visible as `w[2]`. Returning one here is easy
/// (the window is just a different `byteOffset`) but NOT sound yet: the front
/// types this call's result as an ARRAY — the TypedArray ctor declares an array
/// return for every overload, so `is_array_valued` sees `number[]` — and a local
/// bound to a sub-view would then take the raw-array element path and read and
/// WRITE the view's header slots. Making the result honest needs the static kind
/// to follow the receiver's REPRESENTATION, which the front only knows for a
/// `const x = new T(buf)` local (`HeapShape::TypedArrayView`), not for a param or
/// a chained expression. Until it does, a copy with the right length and the
/// right elements beats a view the caller reads as garbage. This is the same
/// level-A limitation already documented for the Vec-backed representation.
#[rtse::abi]
pub fn rtsadp_arr_subarray(arr_word: u64, begin_word: u64, end_word: u64) -> u64 {
    let a = PolyValue::from_raw(arr_word);
    if let Some(v) = view_parts(arr_word) {
        let norm = |w: u64, dflt: i64| -> i64 {
            let pv = PolyValue::from_raw(w);
            if pv.is_undefined() {
                return dflt;
            }
            let n = genops::to_number(pv);
            let n = if n.is_finite() { n as i64 } else { dflt };
            let n = if n < 0 { v.count + n } else { n };
            n.clamp(0, v.count)
        };
        let b = norm(begin_word, 0);
        let e = norm(end_word, v.count).max(b);
        let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for i in b..e {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, v.get(i) as i64);
        }
        return PolyValue::from_object_handle(rt_handles::__rtsn_poly_from_handle(out)).raw();
    }
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    if a.is_object() {
        let ah = rt_handles::__rtsn_poly_to_handle(a.as_handle());
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(ah).max(0);
        let norm = |w: u64, dflt: i64| -> i64 {
            let v = PolyValue::from_raw(w);
            if v.is_undefined() {
                return dflt;
            }
            let n = genops::to_number(v);
            let n = if n.is_finite() { n as i64 } else { dflt };
            let n = if n < 0 { len + n } else { n };
            n.clamp(0, len)
        };
        let b = norm(begin_word, 0);
        let e = norm(end_word, len);
        for i in b..e {
            let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(ah, i);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, w);
        }
    }
    PolyValue::from_object_handle(rt_handles::__rtsn_poly_from_handle(out)).raw()
}

// ── The iteration methods + the view accessors ──────────────────────────────
//
// A Vec-BACKED typed array is tracked by the engine as a plain array, so
// `values()`/`keys()`/`entries()` on one already resolve through the Array
// surface and never reach here. A level-B VIEW is an OBJECT word, so it
// dispatches on the `Uint8Array`/... runtime class instead — and before these
// existed the class had no such row, which is the
// "`Uint8Array.values(0 args)` — no such method on runtime class" bail a real
// page hit. Each entry point below therefore covers BOTH representations:
// materialize the view's window into a plain Vec, then delegate to the very
// same `__rtsadp_arr_*` trampoline the array path uses, so the two
// representations cannot drift in semantics.

/// The receiver's elements as a fresh Vec handle: the WINDOW for a level-B
/// view, the array itself for anything else (no copy — the array trampolines
/// this feeds either copy or read).
fn elems_vec_handle(word: u64) -> u64 {
    if let Some(v) = view_parts(word) {
        let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for i in 0..v.count {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, v.get(i) as i64);
        }
        return out;
    }
    let pv = PolyValue::from_raw(word);
    if pv.is_object() {
        return rt_handles::__rtsn_poly_to_handle(pv.as_handle());
    }
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW()
}

/// `ta.values()` — the elements as a materialized array that is ALSO an open
/// iterator (exactly what `arr.values()` returns; see `__rtsadp_arr_entries`).
#[rtse::abi]
pub fn rtsadp_ta_values(arr_word: u64) -> u64 {
    super::super::arrayops::__rtsadp_arr_values(elems_vec_handle(arr_word))
}

/// `ta.keys()` — the indices, same protocol as [`__rtsadp_ta_values`].
#[rtse::abi]
pub fn rtsadp_ta_keys(arr_word: u64) -> u64 {
    super::super::arrayops::__rtsadp_arr_keys(elems_vec_handle(arr_word))
}

/// `ta.entries()` — the `[index, value]` pairs, same protocol as
/// [`__rtsadp_ta_values`].
#[rtse::abi]
pub fn rtsadp_ta_entries(arr_word: u64) -> u64 {
    super::super::arrayops::__rtsadp_arr_entries(elems_vec_handle(arr_word))
}

/// `ta.byteOffset` — the view's window start. A Vec-backed typed array has no
/// buffer behind it, so its offset is `0` (which is also what JS reports for a
/// typed array built from a length or an array).
#[rtse::abi]
pub fn rtsadp_ta_byte_offset(arr_word: u64) -> u64 {
    let off = view_parts(arr_word).map(|v| v.off).unwrap_or(0);
    PolyValue::from_f64(off as f64).raw()
}

/// `ta.byteLength` — `length * BYTES_PER_ELEMENT`. For a Vec-backed typed
/// array the element width is not carried at runtime (the type is erased into a
/// plain array), so this reports the ELEMENT count, which is right only for the
/// 1-byte kinds; a view reports the exact byte length. Documented divergence,
/// not a silent guess — the buffer-backed representation is the one where
/// `byteLength` is a meaningful question.
#[rtse::abi]
pub fn rtsadp_ta_byte_length(arr_word: u64) -> u64 {
    if let Some(v) = view_parts(arr_word) {
        return PolyValue::from_f64(v.byte_len() as f64).raw();
    }
    let pv = PolyValue::from_raw(arr_word);
    let n = if pv.is_object() {
        let h = rt_handles::__rtsn_poly_to_handle(pv.as_handle());
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0)
    } else {
        0
    };
    PolyValue::from_f64(n as f64).raw()
}

/// `ta.buffer` — the SHARED `ArrayBuffer` a view was built over (`undefined`
/// for a Vec-backed typed array, which has no buffer). Returning the buffer
/// WORD, not a copy, is what makes `new Uint8Array(a.buffer, 4)` chain.
#[rtse::abi]
pub fn rtsadp_ta_buffer(arr_word: u64) -> u64 {
    match view_parts(arr_word) {
        Some(v) => {
            PolyValue::from_object_handle(rt_handles::__rtsn_poly_from_handle(v.bh)).raw()
        }
        None => PolyValue::undefined().raw(),
    }
}
