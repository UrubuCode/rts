//! A `PolyValue` vector store behind an OBJECT handle — the direct refutation of
//! the old engine's `Entry::FloatPrim`.
//!
//! In the old engine, a `Vec<i64>` could not store a fractional `f64` without
//! ambiguity (the `i64` slot already meant int/handle/sentinel), so a
//! fractional float was *re-boxed* as `Entry::FloatPrim` and read back through a
//! `FLOAT_BOX`/`UNBOX`/`EQ`/`ARITH` helper quadruple. Here a `1.5` double, a `7`
//! int32, and a string handle are all just `PolyValue` (`u64`) words sitting in
//! the *same* `Vec<PolyValue>` with **zero** special-casing — heterogeneous
//! storage falls out of the value model for free.
//!
//! The extern "C" ABI is the PolyValue convention: every value crossing the JIT
//! boundary is a raw `u64` (a `PolyValue`); the vec handle is itself a `PolyValue`
//! OBJECT raw word, so even the container reference is one uniform 64-bit value.

use std::sync::{Mutex, OnceLock};

use crate::value::PolyValue;

/// The global store of vectors, indexed by OBJECT slot.
fn store() -> &'static Mutex<Vec<Vec<PolyValue>>> {
    static STORE: OnceLock<Mutex<Vec<Vec<PolyValue>>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Vec::new()))
}

/// Allocate a fresh empty vector, returning its handle as a raw OBJECT
/// `PolyValue` word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_vec_new() -> u64 {
    let mut guard = store().lock().expect("vec store poisoned");
    let slot = guard.len() as u64;
    guard.push(Vec::new());
    PolyValue::from_object_handle(slot).raw()
}

/// Push `val` (a raw `PolyValue`) onto the vector identified by the OBJECT
/// handle `vec` (a raw `PolyValue`). The value is stored verbatim — a double, an
/// int32, a string handle, anything — no per-type boxing.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_vec_push(vec: u64, val: u64) {
    let slot = PolyValue::from_raw(vec).as_handle();
    let mut guard = store().lock().expect("vec store poisoned");
    guard
        .get_mut(slot as usize)
        .expect("vec handle out of range")
        .push(PolyValue::from_raw(val));
}

/// Get element `idx` from the vector `vec`, as a raw `PolyValue`. Out-of-range
/// reads return `undefined` (JS array semantics).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_vec_get(vec: u64, idx: u64) -> u64 {
    let slot = PolyValue::from_raw(vec).as_handle();
    let guard = store().lock().expect("vec store poisoned");
    let v = guard.get(slot as usize).expect("vec handle out of range");
    match v.get(idx as usize) {
        Some(pv) => pv.raw(),
        None => PolyValue::undefined().raw(),
    }
}

/// The length of the vector `vec`, as a raw int32 `PolyValue`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_vec_len(vec: u64) -> u64 {
    let slot = PolyValue::from_raw(vec).as_handle();
    let guard = store().lock().expect("vec store poisoned");
    let len = guard.get(slot as usize).expect("vec handle out of range").len();
    PolyValue::from_i32(len as i32).raw()
}
