//! Basic numeric intrinsics (non-trig, non-minmax).

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_FLOOR(x: f64) -> f64 {
    x.floor()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_CEIL(x: f64) -> f64 {
    x.ceil()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ROUND(x: f64) -> f64 {
    // JS ties-to-+inf, Rust ties-away-from-zero. Match JS.
    (x + 0.5).floor()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_TRUNC(x: f64) -> f64 {
    x.trunc()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SQRT(x: f64) -> f64 {
    x.sqrt()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_CBRT(x: f64) -> f64 {
    x.cbrt()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_POW(base: f64, exp: f64) -> f64 {
    base.powf(exp)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_EXP(x: f64) -> f64 {
    x.exp()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LN(x: f64) -> f64 {
    x.ln()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LOG2(x: f64) -> f64 {
    x.log2()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LOG10(x: f64) -> f64 {
    x.log10()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ABS_F64(x: f64) -> f64 {
    x.abs()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ABS_I64(x: i64) -> i64 {
    // wrapping_abs: i64::MIN maps to itself, matching Rust's overflow rules.
    x.wrapping_abs()
}
