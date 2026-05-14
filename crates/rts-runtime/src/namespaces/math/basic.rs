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

/// `Math.f16round(x)` — arredonda para IEEE 754 binary16 (half) e volta para f64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_F16ROUND(x: f64) -> f64 {
    f16_from_f64(x)
}

fn f16_from_f64(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 {
        return x;
    }
    if !x.is_finite() {
        return x;
    }
    let f = x as f32;
    let bits = f.to_bits();
    let sign = ((bits >> 31) & 0x1) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let mant = bits & 0x7f_ffff;

    let half_bits: u16 = if exp == 0xff {
        let m16 = if mant != 0 { 0x200 | ((mant >> 13) as u16) } else { 0 };
        (sign << 15) | (0x1f << 10) | m16
    } else {
        let new_exp = exp - 127 + 15;
        if new_exp >= 0x1f {
            (sign << 15) | (0x1f << 10)
        } else if new_exp <= 0 {
            if new_exp < -10 {
                sign << 15
            } else {
                let m = mant | 0x80_0000;
                let shift = (14 - new_exp) as u32;
                let round_bit = 1u32 << (shift - 1);
                let sticky_mask = round_bit - 1;
                let sticky = (m & sticky_mask) != 0;
                let mut q = m >> shift;
                let r = (m >> (shift - 1)) & 1;
                if r == 1 && (sticky || (q & 1) == 1) {
                    q += 1;
                }
                (sign << 15) | (q as u16)
            }
        } else {
            let round_bit = 1u32 << 12;
            let sticky_mask = round_bit - 1;
            let sticky = (mant & sticky_mask) != 0;
            let mut q = mant >> 13;
            let r = (mant >> 12) & 1;
            if r == 1 && (sticky || (q & 1) == 1) {
                q += 1;
            }
            let mut e = new_exp as u32;
            if q == 0x400 {
                q = 0;
                e += 1;
            }
            if e >= 0x1f {
                (sign << 15) | (0x1f << 10)
            } else {
                (sign << 15) | ((e as u16) << 10) | (q as u16)
            }
        }
    };

    half_to_f64(half_bits)
}

fn half_to_f64(h: u16) -> f64 {
    let sign = ((h >> 15) & 0x1) as u32;
    let exp = ((h >> 10) & 0x1f) as u32;
    let mant = (h & 0x3ff) as u32;
    let bits32: u32 = if exp == 0 {
        if mant == 0 {
            sign << 31
        } else {
            let mut m = mant;
            let mut e: i32 = -14;
            while (m & 0x400) == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= 0x3ff;
            let new_exp = (e + 127) as u32;
            (sign << 31) | (new_exp << 23) | (m << 13)
        }
    } else if exp == 0x1f {
        (sign << 31) | (0xff << 23) | (mant << 13)
    } else {
        let new_exp = exp + (127 - 15);
        (sign << 31) | (new_exp << 23) | (mant << 13)
    };
    f32::from_bits(bits32) as f64
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

/// (#208) `Math.imul(a, b)` — multiplicacao C-style 32-bit signed.
/// Args sao truncados pra i32 antes de multiplicar (wrapping).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_IMUL(a: i64, b: i64) -> i64 {
    ((a as i32).wrapping_mul(b as i32)) as i64
}

/// (#208) `Math.clz32(x)` — count leading zeros em uint32.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_CLZ32(x: i64) -> i64 {
    (x as u32).leading_zeros() as i64
}

