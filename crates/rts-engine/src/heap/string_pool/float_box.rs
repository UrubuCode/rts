//! (narrow-storage) Boxed-primitive-float ABI — `Entry::FloatPrim` box/unbox
//! and tag-preserving arithmetic/equality over an operand that may be a
//! boxed float (e.g. a `Map.get` result).

use crate::heap::handles::{Entry, alloc_entry, with_entry};

/// Boxes a PRIMITIVE float into an `Entry::FloatPrim` and returns the handle.
/// Used when a float enters a heterogeneous container (a Map value) where the
/// inline i64 doesn't distinguish f64 bits from an int/handle. The read-back
/// (INSPECT/typeof/===/arith) unwraps it as a primitive number.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_FLOAT_BOX(value: f64) -> u64 {
    alloc_entry(Entry::FloatPrim(value))
}

/// If `handle` is `Entry::FloatPrim`, writes the f64 to `*out` and returns 1;
/// otherwise returns 0 (`*out` untouched). Used by ===/arithmetic to unwrap
/// boxed operands.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_FLOAT_UNBOX(handle: u64, out: *mut f64) -> i64 {
    match with_entry(handle, |e| match e {
        Some(Entry::FloatPrim(f)) => Some(*f),
        _ => None,
    }) {
        Some(f) => {
            unsafe { *out = f };
            1
        }
        None => 0,
    }
}

/// (narrow-storage) `===` between an AMBIGUOUS operand (a map.get/vec_get
/// result — may be a boxed FloatPrim, an inline int, or a string/object
/// handle) and a known `other: f64`. Unwraps FloatPrim and compares
/// numerically; an inline int compares as f64 (`2 === 2.0`); a non-numeric
/// handle = false (`===` is strict). Sentinels (undefined/null/bool) = false
/// vs a number.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_FLOAT_EQ_AMBIG(ambig: i64, other: f64) -> i64 {
    // Boxed FloatPrim -> unwrap.
    let mut unboxed = 0.0f64;
    if __RTS_FN_RT_FLOAT_UNBOX(ambig as u64, &mut unboxed) != 0 {
        return (unboxed == other) as i64;
    }
    // JS sentinels (undefined/null/bool/hole) are never === a number.
    if ambig == i64::MIN
        || ambig == i64::MIN + 1
        || ambig == i64::MIN + 2
        || ambig == i64::MIN + 3
        || ambig == i64::MIN + 4
    {
        return 0;
    }
    // A valid handle (string/array/map/obj/etc) — not a number -> false.
    let is_handle = ambig > 0 && with_entry(ambig as u64, |e| e.is_some());
    if is_handle {
        return 0;
    }
    // Otherwise it's a raw inline int -> compare as f64 (`2 === 2.0`).
    ((ambig as f64) == other) as i64
}

/// (narrow-storage) Tag-preserving arithmetic for an operand that may be a
/// boxed FloatPrim (a `Map.get` result). op: 0=Sub, 1=Mul, 2=Div.
/// FloatPrim -> unbox+f64+rebox; otherwise int (Sub/Mul) — Div is always f64
/// (JS `/`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_NUM_ARITH(a: i64, b: i64, op: i64) -> i64 {
    let mut fa = 0.0f64;
    let af = __RTS_FN_RT_FLOAT_UNBOX(a as u64, &mut fa) != 0;
    let mut fb = 0.0f64;
    let bf = __RTS_FN_RT_FLOAT_UNBOX(b as u64, &mut fb) != 0;
    if af || bf || op == 2 {
        let av = if af { fa } else { a as f64 };
        let bv = if bf { fb } else { b as f64 };
        let r = match op {
            0 => av - bv,
            1 => av * bv,
            2 => av / bv,
            _ => 0.0,
        };
        return __RTS_FN_RT_FLOAT_BOX(r) as i64;
    }
    match op {
        0 => a.wrapping_sub(b),
        1 => a.wrapping_mul(b),
        _ => 0,
    }
}
