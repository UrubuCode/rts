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
