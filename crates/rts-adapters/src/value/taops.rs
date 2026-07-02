//! TypedArray constructors + the two TypedArray-only methods (`set`/`subarray`)
//! — Vec-backed (level A).
//!
//! A typed array is represented as an ORDINARY JS array (`Entry::Vec` of
//! PolyValue words), with the TYPE's semantics applied where they are
//! observable: element WRAP on construction (`new Int8Array([200])[0] === -56`)
//! and byte decoding when constructed over an `ArrayBuffer` (`Entry::Buffer`
//! snapshot, little-endian). Everything an array already does (length, index,
//! `Array.from`, `join`, `at`, `includes`, `slice`) comes for free — the ctor's
//! spec declares a `number[]` return so the engine tracks the result as a plain
//! array. LIMITS (level B, `typedarray_view_shared`): a buffer-constructed view
//! is a SNAPSHOT (no live sharing), and post-construction indexed writes do not
//! re-wrap.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::PolyValue;


/// One typed-array kind: element width in bytes, signedness, floatness.
#[derive(Clone, Copy)]
struct Kind {
    elem_bytes: usize,
    signed: bool,
    float: bool,
}

/// Wrap a JS number into the kind's element domain (the observable ToIntN /
/// ToUintN semantics; floats round-trip through their width).
fn wrap(v: f64, k: Kind) -> PolyValue {
    if k.float {
        let x = if k.elem_bytes == 4 { v as f32 as f64 } else { v };
        return PolyValue::from_f64(x);
    }
    let bits = (k.elem_bytes as u32) * 8;
    // ToIntN/ToUintN: truncate toward zero, take modulo 2^bits.
    let t = if v.is_finite() { v.trunc() as i64 } else { 0 };
    let m = (t as u64) & (u64::MAX >> (64 - bits));
    let out = if k.signed {
        let shift = 64 - bits;
        ((m << shift) as i64) >> shift
    } else {
        m as i64
    };
    PolyValue::from_f64(out as f64)
}

/// The constructor core: `arg_word` is a NUMBER (length → zeros), an ARRAY
/// (each element ToNumber'd + wrapped), or an `Entry::Buffer` object (an
/// ArrayBuffer — decode `elem_bytes`-wide little-endian elements, a SNAPSHOT).
/// Anything else → an empty typed array.
fn ta_new(arg_word: u64, k: Kind) -> u64 {
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let v = PolyValue::from_raw(arg_word);
    if !v.is_boxed() || v.is_int32() {
        // A LENGTH: n zero elements.
        let n = super::genops::to_number(v);
        let n = if n.is_finite() && n > 0.0 { n as usize } else { 0 };
        let zero = wrap(0.0, k).raw() as i64;
        for _ in 0..n {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, zero);
        }
        return finish(out);
    }
    if v.is_object() {
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
        // ArrayBuffer (Entry::Buffer): decode bytes, little-endian.
        if let Some(bytes) = rts_engine::heap::handles::with_entry(h, |e| match e {
            Some(rts_engine::heap::handles::Entry::Buffer(b)) => Some(b.clone()),
            _ => None,
        }) {
            let n = bytes.len() / k.elem_bytes;
            for i in 0..n {
                let chunk = &bytes[i * k.elem_bytes..(i + 1) * k.elem_bytes];
                let raw: u64 = chunk
                    .iter()
                    .enumerate()
                    .fold(0u64, |acc, (j, &b)| acc | ((b as u64) << (j * 8)));
                let val = if k.float {
                    match k.elem_bytes {
                        4 => f32::from_le_bytes(chunk.try_into().unwrap()) as f64,
                        _ => f64::from_le_bytes(chunk.try_into().unwrap()),
                    }
                } else {
                    raw as f64
                };
                // Sign-extension for signed int kinds happens through wrap.
                let w = wrap(val, k).raw() as i64;
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, w);
            }
            return finish(out);
        }
        // A source ARRAY: ToNumber + wrap each element.
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0);
        for i in 0..len {
            let ew = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i) as u64;
            let n = super::genops::to_number(PolyValue::from_raw(ew));
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, wrap(n, k).raw() as i64);
        }
        return finish(out);
    }
    // A plain double length (`new Uint16Array(3)` lowered as f64).
    let n = super::genops::to_number(v);
    let n = if n.is_finite() && n > 0.0 { n as usize } else { 0 };
    let zero = wrap(0.0, k).raw() as i64;
    for _ in 0..n {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, zero);
    }
    finish(out)
}

fn finish(vec_handle: u64) -> u64 {
    // The ctor's ABI returns the RAW Vec handle; the engine's array rebox
    // (`ret_is_array_handle` → `__rtsadp_box_handle_auto`) boxes it.
    vec_handle
}

macro_rules! ta_ctor {
    ($name:ident, $bytes:expr, $signed:expr, $float:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(arg_word: u64) -> u64 {
            ta_new(
                arg_word,
                Kind {
                    elem_bytes: $bytes,
                    signed: $signed,
                    float: $float,
                },
            )
        }
    };
}

ta_ctor!(__RTS_FN_GL_TA_NEW_U8, 1, false, false);
ta_ctor!(__RTS_FN_GL_TA_NEW_I8, 1, true, false);
ta_ctor!(__RTS_FN_GL_TA_NEW_U16, 2, false, false);
ta_ctor!(__RTS_FN_GL_TA_NEW_I16, 2, true, false);
ta_ctor!(__RTS_FN_GL_TA_NEW_U32, 4, false, false);
ta_ctor!(__RTS_FN_GL_TA_NEW_I32, 4, true, false);
ta_ctor!(__RTS_FN_GL_TA_NEW_F32, 4, false, true);
ta_ctor!(__RTS_FN_GL_TA_NEW_F64, 8, false, true);

/// `ta.set(src, offset?)` — copy `src`'s elements into the array starting at
/// `offset` (default 0). Word-level copy (the level-A Vec backing does not
/// re-wrap; the tests write in-range values). Returns `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_ta_set(arr_word: u64, src_word: u64, off_word: u64) -> u64 {
    let a = PolyValue::from_raw(arr_word);
    let s = PolyValue::from_raw(src_word);
    if a.is_object() && s.is_object() {
        let ah = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(a.as_handle());
        let sh = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(s.as_handle());
        let off = super::genops::to_number(PolyValue::from_raw(off_word));
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

// ── Atomics (level A — the runtime is single-threaded at the JS level, so
// each op is a plain read/modify/write on the Vec-backed typed array; the
// observable JS results — previous value for RMW ops, the stored value for
// `store` — are exact). ────────────────────────────────────────────────────

fn vec_elem_i64(arr_word: u64, idx_word: u64) -> Option<(u64, i64, i64)> {
    let a = PolyValue::from_raw(arr_word);
    if !a.is_object() {
        return None;
    }
    let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(a.as_handle());
    let i = super::genops::to_number(PolyValue::from_raw(idx_word)) as i64;
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0);
    if i < 0 || i >= len {
        return None;
    }
    let cur = super::genops::to_number(PolyValue::from_raw(
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i) as u64,
    )) as i64;
    Some((h, i, cur))
}

fn num(v: i64) -> u64 {
    PolyValue::from_f64(v as f64).raw()
}

macro_rules! atomics_rmw {
    ($name:ident, $op:expr) => {
        /// Atomics RMW op — returns the PREVIOUS value (JS semantics).
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(arr_word: u64, idx_word: u64, val_word: u64) -> u64 {
            let Some((h, i, cur)) = vec_elem_i64(arr_word, idx_word) else {
                return PolyValue::undefined().raw();
            };
            let v = super::genops::to_number(PolyValue::from_raw(val_word)) as i64;
            let op: fn(i64, i64) -> i64 = $op;
            let next = op(cur, v);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(h, i, num(next) as i64);
            num(cur)
        }
    };
}

atomics_rmw!(__rtsadp_atomics_add, |a, b| a.wrapping_add(b));
atomics_rmw!(__rtsadp_atomics_sub, |a, b| a.wrapping_sub(b));
atomics_rmw!(__rtsadp_atomics_and, |a, b| a & b);
atomics_rmw!(__rtsadp_atomics_or, |a, b| a | b);
atomics_rmw!(__rtsadp_atomics_xor, |a, b| a ^ b);
atomics_rmw!(__rtsadp_atomics_exchange, |_a, b| b);

/// `Atomics.load(ta, i)` — the current value.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_atomics_load(arr_word: u64, idx_word: u64) -> u64 {
    match vec_elem_i64(arr_word, idx_word) {
        Some((_, _, cur)) => num(cur),
        None => PolyValue::undefined().raw(),
    }
}

/// `Atomics.store(ta, i, v)` — stores and returns `v` (JS).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_atomics_store(arr_word: u64, idx_word: u64, val_word: u64) -> u64 {
    if let Some((h, i, _)) = vec_elem_i64(arr_word, idx_word) {
        let v = super::genops::to_number(PolyValue::from_raw(val_word)) as i64;
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(h, i, num(v) as i64);
        return num(v);
    }
    PolyValue::undefined().raw()
}

/// `Atomics.compareExchange(ta, i, expected, replacement)` — returns the
/// PREVIOUS value; stores only on match.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_atomics_cmpxchg(
    arr_word: u64,
    idx_word: u64,
    expected_word: u64,
    replacement_word: u64,
) -> u64 {
    let Some((h, i, cur)) = vec_elem_i64(arr_word, idx_word) else {
        return PolyValue::undefined().raw();
    };
    let expected = super::genops::to_number(PolyValue::from_raw(expected_word)) as i64;
    if cur == expected {
        let r = super::genops::to_number(PolyValue::from_raw(replacement_word)) as i64;
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(h, i, num(r) as i64);
    }
    num(cur)
}

/// 1-arg form `ta.set(src)` — offset 0.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_ta_set1(arr_word: u64, src_word: u64) -> u64 {
    __rtsadp_arr_ta_set(arr_word, src_word, PolyValue::undefined().raw())
}

/// 1-arg form `ta.subarray(begin)` — to the end.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_subarray1(arr_word: u64, begin_word: u64) -> u64 {
    __rtsadp_arr_subarray(arr_word, begin_word, PolyValue::undefined().raw())
}

/// `ta.subarray(begin?, end?)` — the level-A COPY of the range (JS returns a
/// live view; the tests only read the result). Negative indices count from the
/// end, like `slice`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_subarray(arr_word: u64, begin_word: u64, end_word: u64) -> u64 {
    let a = PolyValue::from_raw(arr_word);
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    if a.is_object() {
        let ah = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(a.as_handle());
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(ah).max(0);
        let norm = |w: u64, dflt: i64| -> i64 {
            let v = PolyValue::from_raw(w);
            if v.is_undefined() {
                return dflt;
            }
            let n = super::genops::to_number(v);
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
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(out)).raw()
}
