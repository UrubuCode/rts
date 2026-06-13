//! JS `ToString` on a [`PolyValue`] — the text the console prints for a value.
//!
//! This is the heap-aware sibling of the heap-free `to_string` inside
//! [`super::ops`]: it resolves string handles through the interner and applies
//! the JS `Number→String` algorithm (integer-valued doubles print with no
//! decimal point; `-0` prints `0`; the non-finite spellings are literal).
//!
//! Scope honesty: the f64 formatting uses Rust's `{}` Display, which is
//! shortest-round-trip and matches V8 for the simple magnitudes the harness
//! targets (`1.5`, `3`, `0.1`, …). KNOWN divergences from V8 for exotic
//! magnitudes are documented on [`js_number_to_string`].

use crate::runtime::strings;
use crate::value::PolyValue;

/// JS `ToString(v)` for any [`PolyValue`], resolving heap strings.
///
/// - number (int32 or double) → [`js_number_to_string`]
/// - string → the raw chars, NO surrounding quotes (a bare `console.log("a")`
///   prints `a`, not `"a"`)
/// - boolean → `"true"`/`"false"`; null → `"null"`; undefined → `"undefined"`
/// - object → `"[object Object]"`; function → `"function"`
pub fn js_to_string(v: PolyValue) -> String {
    if v.is_double() {
        return js_number_to_string(v.as_f64());
    }
    if v.is_int32() {
        // An int32 is always integer-valued; print it directly.
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

/// JS `Number→String` for an `f64`.
///
/// - `NaN` → `"NaN"`, `±Infinity` → `"Infinity"`/`"-Infinity"` (literal spellings)
/// - `-0.0` → `"0"` (JS prints negative zero without the sign)
/// - integer-valued finite doubles in `i64` range → printed with NO fractional
///   part (`3.0` → `"3"`), which is the headline JS divergence from Rust's `{}`
///   (`3.0` would print `"3"` in Rust too, but `3` here keeps very large
///   integers exact via the `i64` cast)
/// - everything else → Rust's `{}` (shortest round-trip)
///
/// ## KNOWN divergences from V8
///
/// - Exotic magnitudes outside the `i64` integer window (e.g. `1e21`) print via
///   Rust's `{}` as `1000000000000000000000` rather than V8's `"1e+21"`
///   exponential form. The cross-runtime harness here targets simple fixtures
///   (small integers, short decimals) where Rust shortest-round-trip and V8
///   agree; the exponential-notation edge is a documented later increment.
pub fn js_number_to_string(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    // -0.0 == 0.0 is true, so this also normalizes negative zero to "0".
    if f == 0.0 {
        return "0".to_string();
    }
    // Integer-valued and inside the lossless i64 window: print as a plain
    // integer (no ".0"). JS prints `3` for `3.0`.
    if f == f.trunc() && f.abs() < 9.007_199_254_740_992e15 {
        return format!("{}", f as i64);
    }
    // Fractional / very-large: Rust's shortest round-trip Display.
    format!("{f}")
}
