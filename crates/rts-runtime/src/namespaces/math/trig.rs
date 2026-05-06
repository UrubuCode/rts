//! Trigonometric intrinsics. All angles in radians.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SIN(x: f64) -> f64 {
    x.sin()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_COS(x: f64) -> f64 {
    x.cos()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_TAN(x: f64) -> f64 {
    x.tan()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ASIN(x: f64) -> f64 {
    x.asin()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ACOS(x: f64) -> f64 {
    x.acos()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ATAN(x: f64) -> f64 {
    x.atan()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ATAN2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}
