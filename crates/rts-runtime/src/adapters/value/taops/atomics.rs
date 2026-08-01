//! `Atomics.*` over a typed array, in either representation.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::super::PolyValue;
use super::super::genops;
use super::view::{View, view_parts};

// ── Atomics (level A — the runtime is single-threaded at the JS level, so
// each op is a plain read/modify/write on the Vec-backed typed array; the
// observable JS results — previous value for RMW ops, the stored value for
// `store` — are exact). ────────────────────────────────────────────────────

/// The Atomics element accessor: a Vec-backed typed array reads/writes the Vec
/// slot; a level-B VIEW goes through the shared buffer. `Loc` abstracts the two.
enum Loc {
    Vec(u64, i64),
    View(View, i64),
}

fn atomics_loc(arr_word: u64, idx_word: u64) -> Option<(Loc, i64)> {
    let i = genops::to_number(PolyValue::from_raw(idx_word)) as i64;
    if let Some(view) = view_parts(arr_word) {
        if i < 0 || i >= view.count {
            return None;
        }
        let cur = genops::to_number(PolyValue::from_raw(view.get(i))) as i64;
        return Some((Loc::View(view, i), cur));
    }
    let a = PolyValue::from_raw(arr_word);
    if !a.is_object() {
        return None;
    }
    let h = rt_handles::__rtsn_poly_to_handle(a.as_handle());
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0);
    if i < 0 || i >= len {
        return None;
    }
    let cur = genops::to_number(PolyValue::from_raw(
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i) as u64,
    )) as i64;
    Some((Loc::Vec(h, i), cur))
}

fn atomics_store_loc(loc: &Loc, v: i64) {
    match *loc {
        Loc::Vec(h, i) => {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(h, i, num(v) as i64);
        }
        Loc::View(view, i) => view.set(i, num(v)),
    }
}

fn num(v: i64) -> u64 {
    PolyValue::from_f64(v as f64).raw()
}

/// The RMW half of `Atomics.*`. Bare `#[rtse::abi]` prefixes the Rust fn name
/// with `__`, which is exactly how the sibling `rtsadp_atomics_load`/`_store`/
/// `_cmpxchg` in this file are declared — so the symbols keep their
/// `__rtsadp_atomics_*` spelling while the name is written once.
macro_rules! atomics_rmw {
    ($name:ident, $op:expr) => {
        /// Atomics RMW op — returns the PREVIOUS value (JS semantics).
        #[rtse::abi]
        pub fn $name(arr_word: u64, idx_word: u64, val_word: u64) -> u64 {
            let Some((loc, cur)) = atomics_loc(arr_word, idx_word) else {
                return PolyValue::undefined().raw();
            };
            let v = genops::to_number(PolyValue::from_raw(val_word)) as i64;
            let op: fn(i64, i64) -> i64 = $op;
            let next = op(cur, v);
            atomics_store_loc(&loc, next);
            num(cur)
        }
    };
}

atomics_rmw!(rtsadp_atomics_add, |a, b| a.wrapping_add(b));
atomics_rmw!(rtsadp_atomics_sub, |a, b| a.wrapping_sub(b));
atomics_rmw!(rtsadp_atomics_and, |a, b| a & b);
atomics_rmw!(rtsadp_atomics_or, |a, b| a | b);
atomics_rmw!(rtsadp_atomics_xor, |a, b| a ^ b);
atomics_rmw!(rtsadp_atomics_exchange, |_a, b| b);

/// `Atomics.load(ta, i)` — the current value.
#[rtse::abi]
pub fn rtsadp_atomics_load(arr_word: u64, idx_word: u64) -> u64 {
    match atomics_loc(arr_word, idx_word) {
        Some((_, cur)) => num(cur),
        None => PolyValue::undefined().raw(),
    }
}

/// `Atomics.store(ta, i, v)` — stores and returns `v` (JS).
#[rtse::abi]
pub fn rtsadp_atomics_store(arr_word: u64, idx_word: u64, val_word: u64) -> u64 {
    if let Some((loc, _)) = atomics_loc(arr_word, idx_word) {
        let v = genops::to_number(PolyValue::from_raw(val_word)) as i64;
        atomics_store_loc(&loc, v);
        return num(v);
    }
    PolyValue::undefined().raw()
}

/// `Atomics.compareExchange(ta, i, expected, replacement)` — returns the
/// PREVIOUS value; stores only on match.
#[rtse::abi]
pub fn rtsadp_atomics_cmpxchg(
    arr_word: u64,
    idx_word: u64,
    expected_word: u64,
    replacement_word: u64,
) -> u64 {
    let Some((loc, cur)) = atomics_loc(arr_word, idx_word) else {
        return PolyValue::undefined().raw();
    };
    let expected = genops::to_number(PolyValue::from_raw(expected_word)) as i64;
    if cur == expected {
        let r = genops::to_number(PolyValue::from_raw(replacement_word)) as i64;
        atomics_store_loc(&loc, r);
    }
    num(cur)
}
