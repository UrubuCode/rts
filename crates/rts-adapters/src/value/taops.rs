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

/// LEVEL-B VIEW: a typed array constructed OVER an ArrayBuffer is a keyed
/// object `{ __ta_buf, __ta_bytes, __ta_signed, __ta_float }` — reads/writes
/// go through `TA_GET_ELEM`/`TA_SET_ELEM` on the SHARED `Entry::Buffer`, so
/// two views over one buffer see each other's writes (JS semantics). The
/// engine-owned `__ta_*` slot names are the shape detector (like `#items`).
fn ta_view_new(buf_word: u64, k: Kind) -> u64 {
    let keys = [
        "__ta_buf".to_string(),
        "__ta_bytes".to_string(),
        "__ta_signed".to_string(),
        "__ta_float".to_string(),
    ];
    let shape = crate::shape::intern_global_shape(&keys);
    let h = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, PolyValue::from_i32(shape as i32).raw() as i64);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, buf_word as i64);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(
        h,
        PolyValue::from_i32(k.elem_bytes as i32).raw() as i64,
    );
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(
        h,
        PolyValue::from_i32(k.signed as i32).raw() as i64,
    );
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, PolyValue::from_i32(k.float as i32).raw() as i64);
    // The ctor ABI returns the RAW Vec handle (like `finish`) — the engine's
    // array rebox boxes it; a keyed shape header makes it an OBJECT word there.
    h
}

/// The interned shape id of a level-B view, computed once.
///
/// `intern_global_shape` is idempotent for an identical key sequence, so this is
/// a memoized constant, not a snapshot that can go stale.
fn view_shape_id() -> u32 {
    static ID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ID.get_or_init(|| {
        crate::shape::intern_global_shape(&[
            "__ta_buf".to_string(),
            "__ta_bytes".to_string(),
            "__ta_signed".to_string(),
            "__ta_float".to_string(),
        ])
    })
}

/// Decompose a level-B VIEW word: `(buffer_handle, elem_bytes, signed, float)`.
/// `None` for anything that is not a `__ta_buf`-shaped keyed object.
///
/// ## Reads SLOTS, not properties — the reason this function existed to be fixed
///
/// [`ta_view_new`] lays a view out positionally: `[shape_id, buf, elem_bytes,
/// signed, float]`. The parts sit at fixed indices, identified by the shape id in
/// slot 0. That is the hidden-class contract the whole engine is built on —
/// compare the shape id, then load a fixed offset.
///
/// The previous implementation asked for the four `__ta_*` fields BY NAME through
/// full `__rtsadp_obj_get`. Since `view_parts` runs on EVERY `view[i]` read and
/// write, that meant, per element access: four `intern_poly` calls (each a
/// NON-deduped `STRING_NEW` heap allocation — abi_adapter.rs / string_pool.rs),
/// and four `obj_get` walks that each relocked the shard and re-cloned the
/// shape's key vector. Roughly fifty shard-mutex locks and a dozen string/Vec
/// allocations to answer a question whose answer is four integers already sitting
/// in fixed slots. Worse, the string flood pushed the handle table past its GC
/// floor, so the collector then ran every 256 allocations — the cost was paid
/// twice. A read-only loop could allocate its way to millions of live handles.
///
/// This reads the five slots under ONE `with_entry`, validates slot 0 against the
/// interned view shape, and allocates nothing.
pub(crate) fn view_parts(word: u64) -> Option<(u64, i64, i64, i64)> {
    use rts_engine::heap::handles::{Entry, with_entry};

    let v = PolyValue::from_raw(word);
    if !v.is_object() {
        return None;
    }
    let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
    let slots: Option<[u64; 5]> = with_entry(h, |e| match e {
        Some(Entry::Vec(vec)) if vec.len() >= 5 => {
            Some([vec[0] as u64, vec[1] as u64, vec[2] as u64, vec[3] as u64, vec[4] as u64])
        }
        _ => None,
    });
    let slots = slots?;

    // Slot 0 must be THIS shape — any other keyed object (or an array whose first
    // element happens to be an int) is not a view.
    let shape = PolyValue::from_raw(slots[0]);
    if !shape.is_int32() || shape.as_i32() as u32 != view_shape_id() {
        return None;
    }

    let bv = PolyValue::from_raw(slots[1]);
    if !bv.is_object() {
        return None;
    }
    let num = |w: u64| super::genops::to_number(PolyValue::from_raw(w)) as i64;
    let bytes = num(slots[2]);
    if bytes <= 0 {
        return None;
    }
    let bh = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(bv.as_handle());
    Some((bh, bytes, num(slots[3]), num(slots[4])))
}

/// The element COUNT of a level-B view (`buffer byteLength / elem_bytes`).
pub(crate) fn view_len(bh: u64, bytes: i64) -> i64 {
    let blen = rts_engine::heap::handles::with_entry(bh, |e| match e {
        Some(rts_engine::heap::handles::Entry::Buffer(b)) => b.len() as i64,
        _ => 0,
    });
    if bytes > 0 { blen / bytes } else { 0 }
}

/// Read `view[i]` as a JS number word via the shared buffer.
pub(crate) fn view_get(bh: u64, bytes: i64, signed: i64, float: i64, i: i64) -> u64 {
    use rts_runtime::namespaces::buffer as rt_buf;
    if i < 0 || i >= view_len(bh, bytes) {
        return PolyValue::undefined().raw();
    }
    let raw = rt_buf::__RTS_FN_GL_TA_GET_ELEM(bh, i, bytes, signed, float);
    if float != 0 {
        PolyValue::from_f64(f64::from_bits(raw as u64)).raw()
    } else {
        PolyValue::from_f64(raw as f64).raw()
    }
}

/// Write `view[i] = v` through the shared buffer (wraps via the byte store).
pub(crate) fn view_set(bh: u64, bytes: i64, float: i64, i: i64, val_word: u64) {
    use rts_runtime::namespaces::buffer as rt_buf;
    let n = super::genops::to_number(PolyValue::from_raw(val_word));
    let raw = if float != 0 {
        n.to_bits() as i64
    } else {
        n.trunc() as i64
    };
    rt_buf::__RTS_FN_GL_TA_SET_ELEM(bh, i, bytes, float, raw);
}

/// The constructor core: `arg_word` is a NUMBER (length → zeros), an ARRAY
/// (each element ToNumber'd + wrapped), an `Entry::Buffer` object (an
/// ArrayBuffer — a level-B live VIEW sharing the bytes), or anything else →
/// an empty typed array.
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
        // ArrayBuffer (Entry::Buffer): a level-B live VIEW over the shared bytes.
        let is_buffer = rts_engine::heap::handles::with_entry(h, |e| {
            matches!(e, Some(rts_engine::heap::handles::Entry::Buffer(_)))
        });
        if is_buffer {
            return ta_view_new(arg_word, k);
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
    // Level-B VIEW receiver: write each source element through the shared
    // buffer (`TA_SET_ELEM`), so sibling views observe it.
    if let Some((bh, bytes, _sg, float)) = view_parts(arr_word) {
        if s.is_object() {
            let off = super::genops::to_number(PolyValue::from_raw(off_word));
            let off = if off.is_finite() && off > 0.0 { off as i64 } else { 0 };
            let sh = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(s.as_handle());
            let slen = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(sh).max(0);
            for i in 0..slen {
                let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(sh, i) as u64;
                view_set(bh, bytes, float, off + i, w);
            }
        }
        return PolyValue::undefined().raw();
    }
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

/// The Atomics element accessor: a Vec-backed typed array reads/writes the Vec
/// slot; a level-B VIEW goes through the shared buffer. `Loc` abstracts the two.
enum Loc {
    Vec(u64, i64),
    View { bh: u64, bytes: i64, float: i64, i: i64 },
}

fn atomics_loc(arr_word: u64, idx_word: u64) -> Option<(Loc, i64)> {
    let i = super::genops::to_number(PolyValue::from_raw(idx_word)) as i64;
    if let Some((bh, bytes, signed, float)) = view_parts(arr_word) {
        if i < 0 || i >= view_len(bh, bytes) {
            return None;
        }
        let cur = super::genops::to_number(PolyValue::from_raw(view_get(
            bh, bytes, signed, float, i,
        ))) as i64;
        return Some((Loc::View { bh, bytes, float, i }, cur));
    }
    let a = PolyValue::from_raw(arr_word);
    if !a.is_object() {
        return None;
    }
    let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(a.as_handle());
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0);
    if i < 0 || i >= len {
        return None;
    }
    let cur = super::genops::to_number(PolyValue::from_raw(
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i) as u64,
    )) as i64;
    Some((Loc::Vec(h, i), cur))
}

fn atomics_store_loc(loc: &Loc, v: i64) {
    match *loc {
        Loc::Vec(h, i) => {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(h, i, num(v) as i64);
        }
        Loc::View { bh, bytes, float, i } => view_set(bh, bytes, float, i, num(v)),
    }
}

fn num(v: i64) -> u64 {
    PolyValue::from_f64(v as f64).raw()
}

macro_rules! atomics_rmw {
    ($name:ident, $op:expr) => {
        /// Atomics RMW op — returns the PREVIOUS value (JS semantics).
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(arr_word: u64, idx_word: u64, val_word: u64) -> u64 {
            let Some((loc, cur)) = atomics_loc(arr_word, idx_word) else {
                return PolyValue::undefined().raw();
            };
            let v = super::genops::to_number(PolyValue::from_raw(val_word)) as i64;
            let op: fn(i64, i64) -> i64 = $op;
            let next = op(cur, v);
            atomics_store_loc(&loc, next);
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
    match atomics_loc(arr_word, idx_word) {
        Some((_, cur)) => num(cur),
        None => PolyValue::undefined().raw(),
    }
}

/// `Atomics.store(ta, i, v)` — stores and returns `v` (JS).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_atomics_store(arr_word: u64, idx_word: u64, val_word: u64) -> u64 {
    if let Some((loc, _)) = atomics_loc(arr_word, idx_word) {
        let v = super::genops::to_number(PolyValue::from_raw(val_word)) as i64;
        atomics_store_loc(&loc, v);
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
    let Some((loc, cur)) = atomics_loc(arr_word, idx_word) else {
        return PolyValue::undefined().raw();
    };
    let expected = super::genops::to_number(PolyValue::from_raw(expected_word)) as i64;
    if cur == expected {
        let r = super::genops::to_number(PolyValue::from_raw(replacement_word)) as i64;
        atomics_store_loc(&loc, r);
    }
    num(cur)
}

/// `BigInt.asIntN(bits, v)` — wrap `v` into an N-bit SIGNED integer (the i64
/// interim BigInt model, #219). `asUintN` is the unsigned form.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_bigint_as_intn(bits_word: u64, val_word: u64) -> u64 {
    let bits = super::genops::to_number(PolyValue::from_raw(bits_word)) as u32;
    let v = super::genops::to_number(PolyValue::from_raw(val_word)).trunc() as i64;
    if bits == 0 || bits >= 64 {
        return PolyValue::from_f64(v as f64).raw();
    }
    let m = (v as u64) & (u64::MAX >> (64 - bits));
    let shift = 64 - bits;
    let out = ((m << shift) as i64) >> shift;
    PolyValue::from_f64(out as f64).raw()
}

/// `BigInt.asUintN(bits, v)` — unsigned N-bit wrap.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_bigint_as_uintn(bits_word: u64, val_word: u64) -> u64 {
    let bits = super::genops::to_number(PolyValue::from_raw(bits_word)) as u32;
    let v = super::genops::to_number(PolyValue::from_raw(val_word)).trunc() as i64;
    if bits == 0 || bits >= 64 {
        return PolyValue::from_f64(v as f64).raw();
    }
    let m = (v as u64) & (u64::MAX >> (64 - bits));
    PolyValue::from_f64(m as f64).raw()
}

/// Pack a `...rest` param's value from the uniform-ABI slots: the positional
/// slots `a[start_idx..4]` (TRAILING `undefined`s trimmed — the ABI carries no
/// argc, so an explicit trailing `undefined` argument is indistinguishable
/// from an absent one; documented divergence) followed by every element of the
/// overflow `rest_word` array. Returns a fresh array word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_pack_rest(
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    rest_word: u64,
    start_idx: i64,
) -> u64 {
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let undef = PolyValue::undefined().raw();
    let slots = [a0, a1, a2, a3];
    let start = start_idx.clamp(0, 4) as usize;
    // Trim trailing undefineds from the positional window.
    let mut end = 4usize;
    while end > start && slots[end - 1] == undef {
        end -= 1;
    }
    for &w in &slots[start..end] {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, w as i64);
    }
    let rv = PolyValue::from_raw(rest_word);
    if rv.is_object() {
        let rh = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(rv.as_handle());
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(rh).max(0);
        for i in 0..len {
            let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(rh, i);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, w);
        }
    }
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(out)).raw()
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
