//! `math.*` deterministic f64 primitives wrapping `f64::*` intrinsics.
//!
//! Each function is a thin `extern "C"` shim so Cranelift can emit a direct
//! call; the compiler is free to inline the underlying `f64::sqrt` etc.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SQRT(x: f64) -> f64 {
    x.sqrt()
}

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
    // JS round ties-to-positive-infinity; Rust ties-away-from-zero.
    // Match JS here so user expectations line up.
    (x + 0.5).floor()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ABS(x: f64) -> f64 {
    x.abs()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_POW(base: f64, exp: f64) -> f64 {
    base.powf(exp)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_MIN(a: f64, b: f64) -> f64 {
    a.min(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_MAX(a: f64, b: f64) -> f64 {
    a.max(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SIN(x: f64) -> f64 {
    x.sin()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_COS(x: f64) -> f64 {
    x.cos()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LOG(x: f64) -> f64 {
    x.ln()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_EXP(x: f64) -> f64 {
    x.exp()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_PI() -> f64 {
    std::f64::consts::PI
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_E() -> f64 {
    std::f64::consts::E
}
