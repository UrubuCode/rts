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

/// (#208) `Math.sign(x)` — retorna -1, 0, ou 1. Para NaN retorna NaN.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SIGN(x: f64) -> f64 {
    if x.is_nan() {
        f64::NAN
    } else if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        // +0 ou -0 retorna o proprio (preserva sinal de zero).
        x
    }
}

/// (#208) `Math.hypot(a, b)` — sqrt(a² + b²) sem overflow intermediario.
/// Versao 2-arg apenas; var-args em PR separada.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_HYPOT(a: f64, b: f64) -> f64 {
    a.hypot(b)
}

/// (#208) `Math.expm1(x)` — exp(x) - 1, preciso para x perto de 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_EXPM1(x: f64) -> f64 {
    x.exp_m1()
}

/// (#208) `Math.log1p(x)` — ln(1 + x), preciso para x perto de 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LOG1P(x: f64) -> f64 {
    x.ln_1p()
}

/// (#208) `Math.fround(x)` — arredonda para o f32 mais proximo (depois f64).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_FROUND(x: f64) -> f64 {
    x as f32 as f64
}

/// (#208) `Math.sinh/cosh/tanh` — funcoes hiperbolicas.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SINH(x: f64) -> f64 { x.sinh() }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_COSH(x: f64) -> f64 { x.cosh() }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_TANH(x: f64) -> f64 { x.tanh() }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ASINH(x: f64) -> f64 { x.asinh() }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ACOSH(x: f64) -> f64 { x.acosh() }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ATANH(x: f64) -> f64 { x.atanh() }
