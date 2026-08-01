//! TypedArrays — the constructors plus the TypedArray-only surface.
//!
//! ## The two representations
//!
//! **Level A (Vec-backed)** — `new T(n)` / `new T(array)` / `new T(typedArray)`
//! build an ORDINARY JS array (`Entry::Vec` of PolyValue words) with the TYPE's
//! semantics applied where they are observable: element WRAP on construction
//! (`new Int8Array([200])[0] === -56`). Everything an array already does
//! (length, index, `Array.from`, `join`, `at`, `includes`, `slice`) comes for
//! free — the ctor's spec declares a `number[]` return, so the engine tracks the
//! result as a plain array. LIMIT: a post-construction indexed write does not
//! re-wrap.
//!
//! **Level B (buffer VIEW)** — `new T(buffer[, byteOffset[, length]])` builds a
//! keyed object over the SHARED `Entry::Buffer`, so two views of one buffer
//! observe each other's writes (real JS semantics) and the window
//! (`byteOffset`/`length`) is honoured on every access. See [`view`].
//!
//! ## Layout of this module
//!
//! | file | contents |
//! |---|---|
//! | `mod.rs` (this) | the element `Kind`, the wrap rule, and the ONE ctor body every element kind shares |
//! | [`view`] | the level-B [`View`]: slot layout, decode, element access, the native base-pointer entry point |
//! | [`elems`] | the TypedArray-only methods — `set`/`subarray`/`values`/`keys`/`entries` and the view accessors |
//! | [`atomics`] | `Atomics.*` over either representation |
//! | [`misc`] | `BigInt.asIntN`/`asUintN` + the `...rest` packer |
//!
//! Split out of a single 750-line `taops.rs` when the buffer-window ctor landed
//! (the ≤500-line ceiling for a non-codegen file).

pub mod atomics;
pub mod elems;
pub mod misc;
pub mod view;

pub(crate) use view::view_parts;
use view::ta_view_new;

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::PolyValue;
use super::genops;


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

/// The constructor core, covering the whole JS overload set with ONE body —
/// `arg_word` is:
///
/// - a NUMBER (a LENGTH) → `n` zero elements, Vec-backed;
/// - an `Entry::Buffer` object (an ArrayBuffer) → a level-B live VIEW sharing
///   the bytes, windowed by `off_word`/`len_word` (the 2nd/3rd ctor args, both
///   `undefined` when omitted — see [`ta_view_new`]);
/// - another TYPED ARRAY, in either representation → an element-wise COPY,
///   re-wrapped into this kind (`new Uint8Array(new Int8Array([-1]))[0] === 255`);
/// - a plain ARRAY → each element ToNumber'd + wrapped;
/// - anything else → an empty typed array.
///
/// `off_word`/`len_word` are meaningful only for the ArrayBuffer overload, which
/// is exactly JS: `new Uint8Array([1,2], 1)` ignores the extra arguments.
fn ta_new(arg_word: u64, off_word: u64, len_word: u64, k: Kind) -> u64 {
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let v = PolyValue::from_raw(arg_word);
    let zeros = |n: f64| {
        let n = if n.is_finite() && n > 0.0 { n as usize } else { 0 };
        let zero = wrap(0.0, k).raw() as i64;
        for _ in 0..n {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, zero);
        }
        finish(out)
    };
    if !v.is_boxed() || v.is_int32() {
        // A LENGTH: n zero elements.
        return zeros(genops::to_number(v));
    }
    if v.is_object() {
        let h = rt_handles::__rtsn_poly_to_handle(v.as_handle());
        // ArrayBuffer (Entry::Buffer): a level-B live VIEW over the shared bytes.
        let is_buffer = rts_engine::heap::handles::with_entry(h, |e| {
            matches!(e, Some(rts_engine::heap::handles::Entry::Buffer(_)))
        });
        if is_buffer {
            return ta_view_new(arg_word, k, off_word, len_word);
        }
        // A level-B VIEW source: read through the window, not the raw slots (the
        // view's backing Vec holds the six header words, not elements).
        if let Some(src) = view_parts(arg_word) {
            for i in 0..src.count {
                let n = genops::to_number(PolyValue::from_raw(src.get(i)));
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, wrap(n, k).raw() as i64);
            }
            return finish(out);
        }
        // A source ARRAY (or a Vec-backed typed array): ToNumber + wrap each.
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0);
        for i in 0..len {
            let ew = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i) as u64;
            let n = genops::to_number(PolyValue::from_raw(ew));
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, wrap(n, k).raw() as i64);
        }
        return finish(out);
    }
    // A plain double length (`new Uint16Array(3)` lowered as f64).
    zeros(genops::to_number(v))
}

fn finish(vec_handle: u64) -> u64 {
    // The ctor's ABI returns the RAW Vec handle; the engine's array rebox
    // (`ret_is_array_handle` → `__rtsadp_box_handle_auto`) boxes it.
    vec_handle
}

/// One TypedArray constructor per element kind.
///
/// `#[rtse::abi(native)]` with no `value=` derives the symbol from the Rust fn
/// name (`ta_new_u8` → `__rtsn_ta_new_u8`), so the name is spelled ONCE — at
/// the invocation below — instead of being duplicated as an ident and a
/// string. `registry_build.rs` registers these by that same derived symbol.
///
/// Was `abi` (`__rtsa_`) until 2026-07-31, when that scope was deleted for
/// meaning the same thing as `native` (RTS_ORGANIZATION.md N4). These eight were
/// its ONLY users, and they are invisible to `rts-symbol-baker` because a source
/// scanner cannot see through `macro_rules!` — which is why they are also the
/// last hand-written rows in `registry_build.rs`.
///
/// Every ctor takes THREE words — `(src, byteOffset, length)`. The 2nd and 3rd
/// are the ArrayBuffer-view overload's window and arrive as `undefined` when the
/// call site omits them (the registered `Sig` gives them `DefaultArg::Undefined`),
/// so one symbol per kind serves all of `new T(n)`, `new T(array)`,
/// `new T(typedArray)`, `new T(buf)`, `new T(buf, off)`, `new T(buf, off, len)`.
macro_rules! ta_ctor {
    ($name:ident, $bytes:expr, $signed:expr, $float:expr) => {
        #[rtse::abi(native)]
        pub fn $name(arg_word: u64, off_word: u64, len_word: u64) -> u64 {
            ta_new(
                arg_word,
                off_word,
                len_word,
                Kind {
                    elem_bytes: $bytes,
                    signed: $signed,
                    float: $float,
                },
            )
        }
    };
}

ta_ctor!(ta_new_u8, 1, false, false);
ta_ctor!(ta_new_i8, 1, true, false);
ta_ctor!(ta_new_u16, 2, false, false);
ta_ctor!(ta_new_i16, 2, true, false);
ta_ctor!(ta_new_u32, 4, false, false);
ta_ctor!(ta_new_i32, 4, true, false);
ta_ctor!(ta_new_f32, 4, false, true);
ta_ctor!(ta_new_f64, 8, false, true);
