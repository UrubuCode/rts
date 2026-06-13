//! The generic JS operators on `PolyValue` — the ONE tag-dispatched path that
//! replaces the old engine's AST-shape guessing (`is_map_get_call` &c.).
//!
//! Each operator takes raw `u64` PolyValue words in and returns a raw `u64`
//! PolyValue word out (the tagged-in/tagged-out runtime convention, design §10.3).
//! `+` runs the real JS `+` algorithm on tags; `===` and `typeof` are single tag
//! inspections. There is no per-shape special case and no re-tag helper zoo: the
//! discriminant is the tag inside the value, period.

use crate::runtime::strings;
use crate::value::PolyValue;

/// JS `ToString` for the values P1 can stringify without heap traversal:
/// numbers (both kinds), booleans, `undefined`, `null`, and already-strings.
/// Objects/functions get a placeholder (`"[object Object]"` / a function tag) —
/// enough for the `+` proof, which never feeds an object to concatenation.
fn to_string(v: PolyValue) -> String {
    if v.is_double() {
        return fmt_f64(v.as_f64());
    }
    if v.is_int32() {
        return v.as_i32().to_string();
    }
    if v.is_string() {
        return strings::resolve_poly(v);
    }
    if v.is_undefined() {
        return "undefined".to_string();
    }
    if v.is_null() {
        return "null".to_string();
    }
    if v.is_bool() {
        return if v.as_bool() { "true" } else { "false" }.to_string();
    }
    if v.is_function() {
        return "function".to_string();
    }
    // objects (and any reserved/leaked singleton)
    "[object Object]".to_string()
}

/// Format an `f64` the way JS `String(x)` does for the cases this proof needs:
/// integer-valued finite doubles print with no decimal point (`1.0` → `"1"`),
/// `NaN`/`±Infinity` print their JS spellings, everything else uses Rust's
/// shortest round-trip (close enough for `1.5` → `"1.5"`).
fn fmt_f64(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if f == f.trunc() && f.abs() < 1e21 {
        // integer-valued: print without a fractional part (JS-style).
        return format!("{}", f as i64);
    }
    format!("{f}")
}

/// Is this PolyValue a JS number (either an inline double or a tagged int32)?
fn is_number(v: PolyValue) -> bool {
    v.is_double() || v.is_int32()
}

/// `__rtsn_add` — the JS binary `+` on two PolyValues.
///
/// - both numbers → numeric add; result is an int32 when both inputs were int32
///   and the sum fits in `i32`, otherwise a double.
/// - either operand a string → ToString both, concatenate, intern the result.
/// - (P1 scope: objects ToString to `"[object Object]"`; that path is not
///   exercised by the proof but is handled, not panicked.)
///
/// This single function is the refutation of the old `arr[0] + 5 → "05"` bug:
/// the decision is made on the runtime tags of the actual values, never on the
/// shape of the source expression.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_add(a: u64, b: u64) -> u64 {
    let av = PolyValue::from_raw(a);
    let bv = PolyValue::from_raw(b);

    // String concatenation path: if either side is a string, both ToString.
    if av.is_string() || bv.is_string() {
        let mut s = to_string(av);
        s.push_str(&to_string(bv));
        return strings::intern_poly(&s).raw();
    }

    // Numeric path: both numbers.
    if is_number(av) && is_number(bv) {
        if av.is_int32() && bv.is_int32() {
            let lhs = av.as_i32();
            let rhs = bv.as_i32();
            if let Some(sum) = lhs.checked_add(rhs) {
                return PolyValue::from_i32(sum).raw();
            }
            // overflow: fall through to double.
            return PolyValue::from_f64(lhs as f64 + rhs as f64).raw();
        }
        let sum = av.number_as_f64() + bv.number_as_f64();
        return PolyValue::from_f64(sum).raw();
    }

    // Mixed/other (e.g. number + bool, object + number): JS would ToPrimitive;
    // P1 stringifies both (matches the proof's expectations and never panics).
    let mut s = to_string(av);
    s.push_str(&to_string(bv));
    strings::intern_poly(&s).raw()
}

/// `__rtsn_strict_eq` — JS `===` on two PolyValues, returning a PolyValue bool.
///
/// Same-tag-and-payload is equal (bitwise) for every kind EXCEPT numbers, which
/// must be compared cross-representation (int32 `7` `===` double `7.0`) by value,
/// and NaN, which is never `===` itself.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_strict_eq(a: u64, b: u64) -> u64 {
    let av = PolyValue::from_raw(a);
    let bv = PolyValue::from_raw(b);

    let eq = if is_number(av) && is_number(bv) {
        let lf = av.number_as_f64();
        let rf = bv.number_as_f64();
        // NaN !== NaN, +0 === -0 — both fall out of IEEE `==` on f64.
        lf == rf
    } else {
        // Non-numbers: identical representation ⇒ ===. (Strings are interned so
        // equal text shares a slot; objects/functions compare by handle identity.)
        av.raw() == bv.raw()
    };

    PolyValue::bool(eq).raw()
}

/// `__rtsn_typeof` — JS `typeof`, returning a PolyValue **string** handle of the
/// `typeof_str`. A single tag inspection; no side-table.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_typeof(a: u64) -> u64 {
    let v = PolyValue::from_raw(a);
    strings::intern_poly(v.typeof_str()).raw()
}

/// `__rtsn_to_string` — JS `ToString`, returning a PolyValue string handle.
/// Used by the tests to read results back as text.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_to_string(a: u64) -> u64 {
    let s = to_string(PolyValue::from_raw(a));
    strings::intern_poly(&s).raw()
}
