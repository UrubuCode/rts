//! Min/max/clamp, typed for f64 and i64.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_MIN_F64(a: f64, b: f64) -> f64 {
    a.min(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_MAX_F64(a: f64, b: f64) -> f64 {
    a.max(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_MIN_I64(a: i64, b: i64) -> i64 {
    a.min(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_MAX_I64(a: i64, b: i64) -> i64 {
    a.max(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_CLAMP_F64(x: f64, lo: f64, hi: f64) -> f64 {
    // lo.max(x).min(hi) handles NaN-safe clamp when lo <= hi.
    if x.is_nan() { return x; }
    x.max(lo).min(hi)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_CLAMP_I64(x: i64, lo: i64, hi: i64) -> i64 {
    x.clamp(lo, hi)
}
