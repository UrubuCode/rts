//! `math` namespace — f64/i64 primitives, trig, min/max/clamp, constants and a
//! seeded xorshift64 PRNG.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`). Exercises three macro features: `intrinsic`
//! tags (sqrt/abs/min/max inline in codegen), `#[rts_alias(of = ...)]` (JS-style
//! aliases like log→ln that reuse the canonical symbol), and `#[rts_const]`
//! (PI/E/… as zero-arg fn-backed constants).

use std::cell::Cell;

use rts_engine::abi::ty::{F64, I64, U64};
use rts_macro::rts_namespace;

// ── PRNG state (thread-local xorshift64) ──────────────────────────────────────
thread_local! {
    static RNG_STATE: Cell<u64> = const { Cell::new(0x853c_49e6_748f_ea9b) };
}

#[inline(always)]
fn next_u64() -> u64 {
    RNG_STATE.with(|c| {
        let mut x = c.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        c.set(x);
        x
    })
}

// ── f16 round helpers ─────────────────────────────────────────────────────────
fn f16_from_f64(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 || !x.is_finite() {
        return x;
    }
    let f = x as f32;
    let bits = f.to_bits();
    let sign = ((bits >> 31) & 0x1) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let mant = bits & 0x7f_ffff;

    let half_bits: u16 = if exp == 0xff {
        let m16 = if mant != 0 {
            0x200 | ((mant >> 13) as u16)
        } else {
            0
        };
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

/// Floating-point / integer intrinsics and a seeded xorshift PRNG.
#[rts_namespace(math)]
impl MathNs {
    /// Largest integer <= x.
    #[rts_fn(pure)]
    pub fn floor(x: F64) -> F64 {
        x.floor()
    }

    /// Smallest integer >= x.
    #[rts_fn(pure)]
    pub fn ceil(x: F64) -> F64 {
        x.ceil()
    }

    /// Rounds to nearest; ties go to +Infinity to match JS semantics.
    #[rts_fn(pure)]
    pub fn round(x: F64) -> F64 {
        (x + 0.5).floor()
    }

    /// Truncates fractional part (rounds toward zero).
    #[rts_fn(pure)]
    pub fn trunc(x: F64) -> F64 {
        x.trunc()
    }

    /// Square root.
    #[rts_fn(pure, intrinsic = Sqrt)]
    pub fn sqrt(x: F64) -> F64 {
        x.sqrt()
    }

    /// Cube root.
    #[rts_fn(pure)]
    pub fn cbrt(x: F64) -> F64 {
        x.cbrt()
    }

    /// base raised to exp.
    #[rts_fn(pure)]
    pub fn pow(base: F64, exp: F64) -> F64 {
        base.powf(exp)
    }

    /// e^x.
    #[rts_fn(pure)]
    pub fn exp(x: F64) -> F64 {
        x.exp()
    }

    /// Natural logarithm (base e).
    #[rts_fn(pure)]
    pub fn ln(x: F64) -> F64 {
        x.ln()
    }

    /// Math.log(x) — natural log (alias de ln).
    #[rts_alias(of = ln)]
    pub fn log(x: F64) -> F64 {
        unreachable!()
    }

    /// Base-2 logarithm.
    #[rts_fn(pure)]
    pub fn log2(x: F64) -> F64 {
        x.log2()
    }

    /// Base-10 logarithm.
    #[rts_fn(pure)]
    pub fn log10(x: F64) -> F64 {
        x.log10()
    }

    /// Sign of x: -1, 0, or 1; NaN for NaN.
    #[rts_fn(pure)]
    pub fn sign(x: F64) -> F64 {
        if x.is_nan() {
            f64::NAN
        } else if x > 0.0 {
            1.0
        } else if x < 0.0 {
            -1.0
        } else {
            x
        }
    }

    /// sqrt(a² + b²) sem overflow intermediario. 2-arg em v0.
    #[rts_fn(pure)]
    pub fn hypot(a: F64, b: F64) -> F64 {
        a.hypot(b)
    }

    /// exp(x) - 1, preciso para x perto de 0.
    #[rts_fn(pure)]
    pub fn expm1(x: F64) -> F64 {
        x.exp_m1()
    }

    /// ln(1 + x), preciso para x perto de 0.
    #[rts_fn(pure)]
    pub fn log1p(x: F64) -> F64 {
        x.ln_1p()
    }

    /// Arredonda para o f32 mais proximo (volta para f64).
    #[rts_fn(pure)]
    pub fn fround(x: F64) -> F64 {
        x as f32 as f64
    }

    /// Arredonda para IEEE 754 binary16 (half) e volta para f64.
    #[rts_fn(pure)]
    pub fn f16round(x: F64) -> F64 {
        f16_from_f64(x)
    }

    /// Seno hiperbolico.
    #[rts_fn(pure)]
    pub fn sinh(x: F64) -> F64 {
        x.sinh()
    }

    /// Cosseno hiperbolico.
    #[rts_fn(pure)]
    pub fn cosh(x: F64) -> F64 {
        x.cosh()
    }

    /// Tangente hiperbolica.
    #[rts_fn(pure)]
    pub fn tanh(x: F64) -> F64 {
        x.tanh()
    }

    /// Arc seno hiperbolico.
    #[rts_fn(pure)]
    pub fn asinh(x: F64) -> F64 {
        x.asinh()
    }

    /// Arc cosseno hiperbolico.
    #[rts_fn(pure)]
    pub fn acosh(x: F64) -> F64 {
        x.acosh()
    }

    /// Arc tangente hiperbolica.
    #[rts_fn(pure)]
    pub fn atanh(x: F64) -> F64 {
        x.atanh()
    }

    /// C-style 32-bit signed multiplication (wrapping).
    #[rts_fn(pure)]
    pub fn imul(a: I64, b: I64) -> I64 {
        ((a as i32).wrapping_mul(b as i32)) as i64
    }

    /// Count leading zeros em uint32.
    #[rts_fn(pure)]
    pub fn clz32(x: I64) -> I64 {
        (x as u32).leading_zeros() as i64
    }

    /// Absolute value (f64).
    #[rts_fn(pure, intrinsic = AbsF64)]
    pub fn abs_f64(x: F64) -> F64 {
        x.abs()
    }

    /// Absolute value (i64); i64::MIN maps to itself (wrapping).
    #[rts_fn(pure, intrinsic = AbsI64)]
    pub fn abs_i64(x: I64) -> I64 {
        x.wrapping_abs()
    }

    /// Math.abs(x) — alias de abs_f64.
    #[rts_alias(of = abs_f64, intrinsic = AbsF64)]
    pub fn abs(x: F64) -> F64 {
        unreachable!()
    }

    /// Sine (radians).
    #[rts_fn(pure)]
    pub fn sin(x: F64) -> F64 {
        x.sin()
    }

    /// Cosine (radians).
    #[rts_fn(pure)]
    pub fn cos(x: F64) -> F64 {
        x.cos()
    }

    /// Tangent (radians).
    #[rts_fn(pure)]
    pub fn tan(x: F64) -> F64 {
        x.tan()
    }

    /// Arc sine (returns radians).
    #[rts_fn(pure)]
    pub fn asin(x: F64) -> F64 {
        x.asin()
    }

    /// Arc cosine (returns radians).
    #[rts_fn(pure)]
    pub fn acos(x: F64) -> F64 {
        x.acos()
    }

    /// Arc tangent (returns radians).
    #[rts_fn(pure)]
    pub fn atan(x: F64) -> F64 {
        x.atan()
    }

    /// atan2(y, x) — angle (radians) of the 2D vector (x, y).
    #[rts_fn(pure)]
    pub fn atan2(y: F64, x: F64) -> F64 {
        y.atan2(x)
    }

    /// Minimum of two f64 values (NaN-aware).
    #[rts_fn(pure, intrinsic = MinF64)]
    pub fn min_f64(a: F64, b: F64) -> F64 {
        a.min(b)
    }

    /// Maximum of two f64 values (NaN-aware).
    #[rts_fn(pure, intrinsic = MaxF64)]
    pub fn max_f64(a: F64, b: F64) -> F64 {
        a.max(b)
    }

    /// Minimum of two i64 values.
    #[rts_fn(pure, intrinsic = MinI64)]
    pub fn min_i64(a: I64, b: I64) -> I64 {
        a.min(b)
    }

    /// Math.min(a, b) — alias de min_f64.
    #[rts_alias(of = min_f64, intrinsic = MinF64)]
    pub fn min(a: F64, b: F64) -> F64 {
        unreachable!()
    }

    /// Math.max(a, b) — alias de max_f64.
    #[rts_alias(of = max_f64, intrinsic = MaxF64)]
    pub fn max(a: F64, b: F64) -> F64 {
        unreachable!()
    }

    /// Maximum of two i64 values.
    #[rts_fn(pure, intrinsic = MaxI64)]
    pub fn max_i64(a: I64, b: I64) -> I64 {
        a.max(b)
    }

    /// Clamps x into [lo, hi]. NaN propagates.
    #[rts_fn(pure)]
    pub fn clamp_f64(x: F64, lo: F64, hi: F64) -> F64 {
        if x.is_nan() {
            return x;
        }
        x.max(lo).min(hi)
    }

    /// Clamps x into [lo, hi].
    #[rts_fn(pure)]
    pub fn clamp_i64(x: I64, lo: I64, hi: I64) -> I64 {
        x.clamp(lo, hi)
    }

    /// Uniform f64 in [0, 1) from a thread-local xorshift64 PRNG.
    #[rts_fn]
    pub fn random_f64() -> F64 {
        let bits = next_u64() >> 11;
        bits as f64 / ((1u64 << 53) as f64)
    }

    /// Math.random() — alias de random_f64. Uniform [0, 1).
    #[rts_alias(of = random_f64)]
    pub fn random() -> F64 {
        unreachable!()
    }

    /// Uniform i64 in [lo, hi). Returns lo when lo >= hi.
    #[rts_fn]
    pub fn random_i64_range(lo: I64, hi: I64) -> I64 {
        if lo >= hi {
            return lo;
        }
        let span = (hi as i128) - (lo as i128);
        let r = next_u64() as i128;
        let offset = r.rem_euclid(span);
        (lo as i128 + offset) as i64
    }

    /// Seeds the PRNG. Zero is replaced by the default seed.
    #[rts_fn]
    pub fn seed(s: U64) {
        let s = if s == 0 { 0x853c_49e6_748f_ea9b } else { s };
        RNG_STATE.with(|c| c.set(s));
    }

    /// Archimedes' constant.
    #[rts_const(pure)]
    pub fn PI() -> F64 {
        std::f64::consts::PI
    }

    /// Euler's number.
    #[rts_const(pure)]
    pub fn E() -> F64 {
        std::f64::consts::E
    }

    /// Positive infinity.
    #[rts_const(pure)]
    pub fn INFINITY() -> F64 {
        f64::INFINITY
    }

    /// Quiet NaN.
    #[rts_const(pure)]
    pub fn NAN() -> F64 {
        f64::NAN
    }

    /// Square root of 2 (~1.4142).
    #[rts_const(pure)]
    pub fn SQRT2() -> F64 {
        std::f64::consts::SQRT_2
    }

    /// 1/sqrt(2) (~0.7071).
    #[rts_const(pure)]
    pub fn SQRT1_2() -> F64 {
        std::f64::consts::FRAC_1_SQRT_2
    }

    /// Natural log of 2 (~0.6931).
    #[rts_const(pure)]
    pub fn LN2() -> F64 {
        std::f64::consts::LN_2
    }

    /// Natural log of 10 (~2.302).
    #[rts_const(pure)]
    pub fn LN10() -> F64 {
        std::f64::consts::LN_10
    }

    /// Log base 2 of e (~1.4427).
    #[rts_const(pure)]
    pub fn LOG2E() -> F64 {
        std::f64::consts::LOG2_E
    }

    /// Log base 10 of e (~0.4342).
    #[rts_const(pure)]
    pub fn LOG10E() -> F64 {
        std::f64::consts::LOG10_E
    }
}
