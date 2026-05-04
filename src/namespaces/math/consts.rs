//! Numeric constants exposed as zero-arg `extern "C"` fns.
//!
//! Real `MemberKind::Constant` support (codegen resolving the symbol as a
//! global data load) is still pending. Until then we model constants as
//! thin accessor functions so callers can still write `math.pi()`.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_PI() -> f64 {
    std::f64::consts::PI
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_E() -> f64 {
    std::f64::consts::E
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_INFINITY() -> f64 {
    f64::INFINITY
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_NAN() -> f64 {
    f64::NAN
}

// (#208) Math constants extras (JS spec).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SQRT2() -> f64 {
    std::f64::consts::SQRT_2
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SQRT1_2() -> f64 {
    std::f64::consts::FRAC_1_SQRT_2
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LN2() -> f64 {
    std::f64::consts::LN_2
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LN10() -> f64 {
    std::f64::consts::LN_10
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LOG2E() -> f64 {
    std::f64::consts::LOG2_E
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LOG10E() -> f64 {
    std::f64::consts::LOG10_E
}
