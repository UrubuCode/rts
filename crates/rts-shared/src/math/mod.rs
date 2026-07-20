//! `math` namespace — f64/i64 primitives, trig, min/max/clamp, constants and a
//! seeded xorshift64 PRNG.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).
//! Exercita três features: `intrinsic` tags (sqrt/abs/min/max inline no codegen),
//! aliases JS-style (`log`→`ln`, `abs`→`abs_f64`, `min`/`max`/`random`) que
//! reusam o símbolo canônico com `fn_ptr` nulo, e constantes
//! (`MemberKind::Constant`, PI/E/… como getters zero-arg).

use std::cell::Cell;

use rts_engine::abi::ty::{F64, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Intrinsic, Member, MemberFlags, MemberKind, Sig};

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

// ── extern "C" symbols ────────────────────────────────────────────────────────

/// Largest integer <= x.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_FLOOR(x: F64) -> F64 {
    x.floor()
}

/// Smallest integer >= x.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_CEIL(x: F64) -> F64 {
    x.ceil()
}

/// Rounds to nearest; ties go to +Infinity to match JS semantics.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ROUND(x: F64) -> F64 {
    let r = (x + 0.5).floor();
    // JS: a negative input rounding to zero yields NEGATIVE zero
    // (`Math.round(-0.4)` -> -0, `Object.is(Math.round(-0.5), -0)` -> true).
    if r == 0.0 && (x < 0.0 || (x == 0.0 && x.is_sign_negative())) {
        return -0.0;
    }
    r
}

/// Truncates fractional part (rounds toward zero).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_TRUNC(x: F64) -> F64 {
    x.trunc()
}

/// Square root.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SQRT(x: F64) -> F64 {
    x.sqrt()
}

/// Cube root.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_CBRT(x: F64) -> F64 {
    x.cbrt()
}

/// base raised to exp.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_POW(base: F64, exp: F64) -> F64 {
    base.powf(exp)
}

/// e^x.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_EXP(x: F64) -> F64 {
    x.exp()
}

/// Natural logarithm (base e).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LN(x: F64) -> F64 {
    x.ln()
}

/// Base-2 logarithm.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LOG2(x: F64) -> F64 {
    x.log2()
}

/// Base-10 logarithm.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LOG10(x: F64) -> F64 {
    x.log10()
}

/// Sign of x: -1, 0, or 1; NaN for NaN.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SIGN(x: F64) -> F64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_HYPOT(a: F64, b: F64) -> F64 {
    a.hypot(b)
}

/// exp(x) - 1, preciso para x perto de 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_EXPM1(x: F64) -> F64 {
    x.exp_m1()
}

/// ln(1 + x), preciso para x perto de 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LOG1P(x: F64) -> F64 {
    x.ln_1p()
}

/// Arredonda para o f32 mais proximo (volta para f64).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_FROUND(x: F64) -> F64 {
    x as f32 as f64
}

/// Arredonda para IEEE 754 binary16 (half) e volta para f64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_F16ROUND(x: F64) -> F64 {
    f16_from_f64(x)
}

/// Seno hiperbolico.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SINH(x: F64) -> F64 {
    x.sinh()
}

/// Cosseno hiperbolico.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_COSH(x: F64) -> F64 {
    x.cosh()
}

/// Tangente hiperbolica.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_TANH(x: F64) -> F64 {
    x.tanh()
}

/// Arc seno hiperbolico.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ASINH(x: F64) -> F64 {
    x.asinh()
}

/// Arc cosseno hiperbolico.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ACOSH(x: F64) -> F64 {
    x.acosh()
}

/// Arc tangente hiperbolica.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ATANH(x: F64) -> F64 {
    x.atanh()
}

/// C-style 32-bit signed multiplication (wrapping).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_IMUL(a: I64, b: I64) -> I64 {
    ((a as i32).wrapping_mul(b as i32)) as i64
}

/// Count leading zeros em uint32.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_CLZ32(x: I64) -> I64 {
    (x as u32).leading_zeros() as i64
}

/// Absolute value (f64).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ABS_F64(x: F64) -> F64 {
    x.abs()
}

/// Absolute value (i64); i64::MIN maps to itself (wrapping).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ABS_I64(x: I64) -> I64 {
    x.wrapping_abs()
}

/// Sine (radians).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SIN(x: F64) -> F64 {
    x.sin()
}

/// Cosine (radians).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_COS(x: F64) -> F64 {
    x.cos()
}

/// Tangent (radians).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_TAN(x: F64) -> F64 {
    x.tan()
}

/// Arc sine (returns radians).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ASIN(x: F64) -> F64 {
    x.asin()
}

/// Arc cosine (returns radians).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ACOS(x: F64) -> F64 {
    x.acos()
}

/// Arc tangent (returns radians).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ATAN(x: F64) -> F64 {
    x.atan()
}

/// atan2(y, x) — angle (radians) of the 2D vector (x, y).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_ATAN2(y: F64, x: F64) -> F64 {
    y.atan2(x)
}

/// Minimum of two f64 values (NaN-aware).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_MIN_F64(a: F64, b: F64) -> F64 {
    a.min(b)
}

/// Maximum of two f64 values (NaN-aware).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_MAX_F64(a: F64, b: F64) -> F64 {
    a.max(b)
}

/// Minimum of two i64 values.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_MIN_I64(a: I64, b: I64) -> I64 {
    a.min(b)
}

/// Maximum of two i64 values.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_MAX_I64(a: I64, b: I64) -> I64 {
    a.max(b)
}

/// Clamps x into [lo, hi]. NaN propagates.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_CLAMP_F64(x: F64, lo: F64, hi: F64) -> F64 {
    if x.is_nan() {
        return x;
    }
    x.max(lo).min(hi)
}

/// Clamps x into [lo, hi].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_CLAMP_I64(x: I64, lo: I64, hi: I64) -> I64 {
    x.clamp(lo, hi)
}

/// Uniform f64 in [0, 1) from a thread-local xorshift64 PRNG.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_RANDOM_F64() -> F64 {
    let bits = next_u64() >> 11;
    bits as f64 / ((1u64 << 53) as f64)
}

/// Uniform i64 in [lo, hi). Returns lo when lo >= hi.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_RANDOM_I64_RANGE(lo: I64, hi: I64) -> I64 {
    if lo >= hi {
        return lo;
    }
    let span = (hi as i128) - (lo as i128);
    let r = next_u64() as i128;
    let offset = r.rem_euclid(span);
    (lo as i128 + offset) as i64
}

/// Seeds the PRNG. Zero is replaced by the default seed.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SEED(s: U64) {
    let s = if s == 0 { 0x853c_49e6_748f_ea9b } else { s };
    RNG_STATE.with(|c| c.set(s));
}

/// Archimedes' constant.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_PI() -> F64 {
    std::f64::consts::PI
}

/// Euler's number.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_E() -> F64 {
    std::f64::consts::E
}

/// Positive infinity.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_INFINITY() -> F64 {
    f64::INFINITY
}

/// Quiet NaN.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_NAN() -> F64 {
    f64::NAN
}

/// Square root of 2 (~1.4142).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SQRT2() -> F64 {
    std::f64::consts::SQRT_2
}

/// 1/sqrt(2) (~0.7071).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SQRT1_2() -> F64 {
    std::f64::consts::FRAC_1_SQRT_2
}

/// Natural log of 2 (~0.6931).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LN2() -> F64 {
    std::f64::consts::LN_2
}

/// Natural log of 10 (~2.302).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LN10() -> F64 {
    std::f64::consts::LN_10
}

/// Log base 2 of e (~1.4427).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LOG2E() -> F64 {
    std::f64::consts::LOG2_E
}

/// Log base 10 of e (~0.4342).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_LOG10E() -> F64 {
    std::f64::consts::LOG10_E
}

// ── member helpers ────────────────────────────────────────────────────────────

/// Função `math.f(args)` — com `pure` e `intrinsic` opcionais.
#[allow(clippy::too_many_arguments)]
fn func(
    name: &str,
    symbol: &str,
    sig: Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
    intrinsic: Option<Intrinsic>,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        intrinsic,
    }
}

/// Constante `math.K` (zero-arg, sem parênteses) — getter pure F64.
fn cst(name: &str, symbol: &str, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Constant,
        sig: Sig::new(Vec::new(), AbiType::F64),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
    }
}

/// Alias JS-style — sem extern próprio: aponta `symbol` ao alvo canônico e usa
/// `fn_ptr` nulo. `pure: false` (os aliases não carregam `pure`).
fn alias(
    name: &str,
    symbol: &str,
    sig: Sig,
    ts: &str,
    doc: &str,
    intrinsic: Option<Intrinsic>,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(core::ptr::null::<u8>()),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic,
    }
}

/// Registra a namespace `math` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("math")
        .doc("Floating-point / integer intrinsics and a seeded xorshift PRNG.")
        .member(func(
            "floor",
            "__RTS_FN_NS_MATH_FLOOR",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "floor(x: number): number",
            "Largest integer <= x.",
            __RTS_FN_NS_MATH_FLOOR as *const u8,
            true,
            None,
        ))
        .member(func(
            "ceil",
            "__RTS_FN_NS_MATH_CEIL",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "ceil(x: number): number",
            "Smallest integer >= x.",
            __RTS_FN_NS_MATH_CEIL as *const u8,
            true,
            None,
        ))
        .member(func(
            "round",
            "__RTS_FN_NS_MATH_ROUND",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "round(x: number): number",
            "Rounds to nearest; ties go to +Infinity to match JS semantics.",
            __RTS_FN_NS_MATH_ROUND as *const u8,
            true,
            None,
        ))
        .member(func(
            "trunc",
            "__RTS_FN_NS_MATH_TRUNC",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "trunc(x: number): number",
            "Truncates fractional part (rounds toward zero).",
            __RTS_FN_NS_MATH_TRUNC as *const u8,
            true,
            None,
        ))
        .member(func(
            "sqrt",
            "__RTS_FN_NS_MATH_SQRT",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "sqrt(x: number): number",
            "Square root.",
            __RTS_FN_NS_MATH_SQRT as *const u8,
            true,
            Some(Intrinsic::Sqrt),
        ))
        .member(func(
            "cbrt",
            "__RTS_FN_NS_MATH_CBRT",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "cbrt(x: number): number",
            "Cube root.",
            __RTS_FN_NS_MATH_CBRT as *const u8,
            true,
            None,
        ))
        .member(func(
            "pow",
            "__RTS_FN_NS_MATH_POW",
            Sig::new(vec![AbiType::F64, AbiType::F64], AbiType::F64),
            "pow(base: number, exp: number): number",
            "base raised to exp.",
            __RTS_FN_NS_MATH_POW as *const u8,
            true,
            None,
        ))
        .member(func(
            "exp",
            "__RTS_FN_NS_MATH_EXP",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "exp(x: number): number",
            "e^x.",
            __RTS_FN_NS_MATH_EXP as *const u8,
            true,
            None,
        ))
        .member(func(
            "ln",
            "__RTS_FN_NS_MATH_LN",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "ln(x: number): number",
            "Natural logarithm (base e).",
            __RTS_FN_NS_MATH_LN as *const u8,
            true,
            None,
        ))
        .member(alias(
            "log",
            "__RTS_FN_NS_MATH_LN",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "log(x: number): number",
            "Math.log(x) — natural log (alias de ln).",
            None,
        ))
        .member(func(
            "log2",
            "__RTS_FN_NS_MATH_LOG2",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "log2(x: number): number",
            "Base-2 logarithm.",
            __RTS_FN_NS_MATH_LOG2 as *const u8,
            true,
            None,
        ))
        .member(func(
            "log10",
            "__RTS_FN_NS_MATH_LOG10",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "log10(x: number): number",
            "Base-10 logarithm.",
            __RTS_FN_NS_MATH_LOG10 as *const u8,
            true,
            None,
        ))
        .member(func(
            "sign",
            "__RTS_FN_NS_MATH_SIGN",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "sign(x: number): number",
            "Sign of x: -1, 0, or 1; NaN for NaN.",
            __RTS_FN_NS_MATH_SIGN as *const u8,
            true,
            None,
        ))
        .member(func(
            "hypot",
            "__RTS_FN_NS_MATH_HYPOT",
            Sig::new(vec![AbiType::F64, AbiType::F64], AbiType::F64),
            "hypot(a: number, b: number): number",
            "sqrt(a² + b²) sem overflow intermediario. 2-arg em v0.",
            __RTS_FN_NS_MATH_HYPOT as *const u8,
            true,
            None,
        ))
        .member(func(
            "expm1",
            "__RTS_FN_NS_MATH_EXPM1",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "expm1(x: number): number",
            "exp(x) - 1, preciso para x perto de 0.",
            __RTS_FN_NS_MATH_EXPM1 as *const u8,
            true,
            None,
        ))
        .member(func(
            "log1p",
            "__RTS_FN_NS_MATH_LOG1P",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "log1p(x: number): number",
            "ln(1 + x), preciso para x perto de 0.",
            __RTS_FN_NS_MATH_LOG1P as *const u8,
            true,
            None,
        ))
        .member(func(
            "fround",
            "__RTS_FN_NS_MATH_FROUND",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "fround(x: number): number",
            "Arredonda para o f32 mais proximo (volta para f64).",
            __RTS_FN_NS_MATH_FROUND as *const u8,
            true,
            None,
        ))
        .member(func(
            "f16round",
            "__RTS_FN_NS_MATH_F16ROUND",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "f16round(x: number): number",
            "Arredonda para IEEE 754 binary16 (half) e volta para f64.",
            __RTS_FN_NS_MATH_F16ROUND as *const u8,
            true,
            None,
        ))
        .member(func(
            "sinh",
            "__RTS_FN_NS_MATH_SINH",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "sinh(x: number): number",
            "Seno hiperbolico.",
            __RTS_FN_NS_MATH_SINH as *const u8,
            true,
            None,
        ))
        .member(func(
            "cosh",
            "__RTS_FN_NS_MATH_COSH",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "cosh(x: number): number",
            "Cosseno hiperbolico.",
            __RTS_FN_NS_MATH_COSH as *const u8,
            true,
            None,
        ))
        .member(func(
            "tanh",
            "__RTS_FN_NS_MATH_TANH",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "tanh(x: number): number",
            "Tangente hiperbolica.",
            __RTS_FN_NS_MATH_TANH as *const u8,
            true,
            None,
        ))
        .member(func(
            "asinh",
            "__RTS_FN_NS_MATH_ASINH",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "asinh(x: number): number",
            "Arc seno hiperbolico.",
            __RTS_FN_NS_MATH_ASINH as *const u8,
            true,
            None,
        ))
        .member(func(
            "acosh",
            "__RTS_FN_NS_MATH_ACOSH",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "acosh(x: number): number",
            "Arc cosseno hiperbolico.",
            __RTS_FN_NS_MATH_ACOSH as *const u8,
            true,
            None,
        ))
        .member(func(
            "atanh",
            "__RTS_FN_NS_MATH_ATANH",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "atanh(x: number): number",
            "Arc tangente hiperbolica.",
            __RTS_FN_NS_MATH_ATANH as *const u8,
            true,
            None,
        ))
        .member(func(
            "imul",
            "__RTS_FN_NS_MATH_IMUL",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "imul(a: number, b: number): number",
            "C-style 32-bit signed multiplication (wrapping).",
            __RTS_FN_NS_MATH_IMUL as *const u8,
            true,
            None,
        ))
        .member(func(
            "clz32",
            "__RTS_FN_NS_MATH_CLZ32",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "clz32(x: number): number",
            "Count leading zeros em uint32.",
            __RTS_FN_NS_MATH_CLZ32 as *const u8,
            true,
            None,
        ))
        .member(func(
            "abs_f64",
            "__RTS_FN_NS_MATH_ABS_F64",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "abs_f64(x: number): number",
            "Absolute value (f64).",
            __RTS_FN_NS_MATH_ABS_F64 as *const u8,
            true,
            Some(Intrinsic::AbsF64),
        ))
        .member(func(
            "abs_i64",
            "__RTS_FN_NS_MATH_ABS_I64",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "abs_i64(x: number): number",
            "Absolute value (i64); i64::MIN maps to itself (wrapping).",
            __RTS_FN_NS_MATH_ABS_I64 as *const u8,
            true,
            Some(Intrinsic::AbsI64),
        ))
        .member(alias(
            "abs",
            "__RTS_FN_NS_MATH_ABS_F64",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "abs(x: number): number",
            "Math.abs(x) — alias de abs_f64.",
            Some(Intrinsic::AbsF64),
        ))
        .member(func(
            "sin",
            "__RTS_FN_NS_MATH_SIN",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "sin(x: number): number",
            "Sine (radians).",
            __RTS_FN_NS_MATH_SIN as *const u8,
            true,
            None,
        ))
        .member(func(
            "cos",
            "__RTS_FN_NS_MATH_COS",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "cos(x: number): number",
            "Cosine (radians).",
            __RTS_FN_NS_MATH_COS as *const u8,
            true,
            None,
        ))
        .member(func(
            "tan",
            "__RTS_FN_NS_MATH_TAN",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "tan(x: number): number",
            "Tangent (radians).",
            __RTS_FN_NS_MATH_TAN as *const u8,
            true,
            None,
        ))
        .member(func(
            "asin",
            "__RTS_FN_NS_MATH_ASIN",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "asin(x: number): number",
            "Arc sine (returns radians).",
            __RTS_FN_NS_MATH_ASIN as *const u8,
            true,
            None,
        ))
        .member(func(
            "acos",
            "__RTS_FN_NS_MATH_ACOS",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "acos(x: number): number",
            "Arc cosine (returns radians).",
            __RTS_FN_NS_MATH_ACOS as *const u8,
            true,
            None,
        ))
        .member(func(
            "atan",
            "__RTS_FN_NS_MATH_ATAN",
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "atan(x: number): number",
            "Arc tangent (returns radians).",
            __RTS_FN_NS_MATH_ATAN as *const u8,
            true,
            None,
        ))
        .member(func(
            "atan2",
            "__RTS_FN_NS_MATH_ATAN2",
            Sig::new(vec![AbiType::F64, AbiType::F64], AbiType::F64),
            "atan2(y: number, x: number): number",
            "atan2(y, x) — angle (radians) of the 2D vector (x, y).",
            __RTS_FN_NS_MATH_ATAN2 as *const u8,
            true,
            None,
        ))
        .member(func(
            "min_f64",
            "__RTS_FN_NS_MATH_MIN_F64",
            Sig::new(vec![AbiType::F64, AbiType::F64], AbiType::F64),
            "min_f64(a: number, b: number): number",
            "Minimum of two f64 values (NaN-aware).",
            __RTS_FN_NS_MATH_MIN_F64 as *const u8,
            true,
            Some(Intrinsic::MinF64),
        ))
        .member(func(
            "max_f64",
            "__RTS_FN_NS_MATH_MAX_F64",
            Sig::new(vec![AbiType::F64, AbiType::F64], AbiType::F64),
            "max_f64(a: number, b: number): number",
            "Maximum of two f64 values (NaN-aware).",
            __RTS_FN_NS_MATH_MAX_F64 as *const u8,
            true,
            Some(Intrinsic::MaxF64),
        ))
        .member(func(
            "min_i64",
            "__RTS_FN_NS_MATH_MIN_I64",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "min_i64(a: number, b: number): number",
            "Minimum of two i64 values.",
            __RTS_FN_NS_MATH_MIN_I64 as *const u8,
            true,
            Some(Intrinsic::MinI64),
        ))
        .member(alias(
            "min",
            "__RTS_FN_NS_MATH_MIN_F64",
            Sig::new(vec![AbiType::F64, AbiType::F64], AbiType::F64),
            "min(a: number, b: number): number",
            "Math.min(a, b) — alias de min_f64.",
            Some(Intrinsic::MinF64),
        ))
        .member(alias(
            "max",
            "__RTS_FN_NS_MATH_MAX_F64",
            Sig::new(vec![AbiType::F64, AbiType::F64], AbiType::F64),
            "max(a: number, b: number): number",
            "Math.max(a, b) — alias de max_f64.",
            Some(Intrinsic::MaxF64),
        ))
        .member(func(
            "max_i64",
            "__RTS_FN_NS_MATH_MAX_I64",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "max_i64(a: number, b: number): number",
            "Maximum of two i64 values.",
            __RTS_FN_NS_MATH_MAX_I64 as *const u8,
            true,
            Some(Intrinsic::MaxI64),
        ))
        .member(func(
            "clamp_f64",
            "__RTS_FN_NS_MATH_CLAMP_F64",
            Sig::new(vec![AbiType::F64, AbiType::F64, AbiType::F64], AbiType::F64),
            "clamp_f64(x: number, lo: number, hi: number): number",
            "Clamps x into [lo, hi]. NaN propagates.",
            __RTS_FN_NS_MATH_CLAMP_F64 as *const u8,
            true,
            None,
        ))
        .member(func(
            "clamp_i64",
            "__RTS_FN_NS_MATH_CLAMP_I64",
            Sig::new(vec![AbiType::I64, AbiType::I64, AbiType::I64], AbiType::I64),
            "clamp_i64(x: number, lo: number, hi: number): number",
            "Clamps x into [lo, hi].",
            __RTS_FN_NS_MATH_CLAMP_I64 as *const u8,
            true,
            None,
        ))
        .member(func(
            "random_f64",
            "__RTS_FN_NS_MATH_RANDOM_F64",
            Sig::new(Vec::new(), AbiType::F64),
            "random_f64(): number",
            "Uniform f64 in [0, 1) from a thread-local xorshift64 PRNG.",
            __RTS_FN_NS_MATH_RANDOM_F64 as *const u8,
            false,
            None,
        ))
        .member(alias(
            "random",
            "__RTS_FN_NS_MATH_RANDOM_F64",
            Sig::new(Vec::new(), AbiType::F64),
            "random(): number",
            "Math.random() — alias de random_f64. Uniform [0, 1).",
            None,
        ))
        .member(func(
            "random_i64_range",
            "__RTS_FN_NS_MATH_RANDOM_I64_RANGE",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "random_i64_range(lo: number, hi: number): number",
            "Uniform i64 in [lo, hi). Returns lo when lo >= hi.",
            __RTS_FN_NS_MATH_RANDOM_I64_RANGE as *const u8,
            false,
            None,
        ))
        .member(func(
            "seed",
            "__RTS_FN_NS_MATH_SEED",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "seed(s: number): void",
            "Seeds the PRNG. Zero is replaced by the default seed.",
            __RTS_FN_NS_MATH_SEED as *const u8,
            false,
            None,
        ))
        .member(cst(
            "PI",
            "__RTS_FN_NS_MATH_PI",
            "PI: number",
            "Archimedes' constant.",
            __RTS_FN_NS_MATH_PI as *const u8,
        ))
        .member(cst(
            "E",
            "__RTS_FN_NS_MATH_E",
            "E: number",
            "Euler's number.",
            __RTS_FN_NS_MATH_E as *const u8,
        ))
        .member(cst(
            "INFINITY",
            "__RTS_FN_NS_MATH_INFINITY",
            "INFINITY: number",
            "Positive infinity.",
            __RTS_FN_NS_MATH_INFINITY as *const u8,
        ))
        .member(cst(
            "NAN",
            "__RTS_FN_NS_MATH_NAN",
            "NAN: number",
            "Quiet NaN.",
            __RTS_FN_NS_MATH_NAN as *const u8,
        ))
        .member(cst(
            "SQRT2",
            "__RTS_FN_NS_MATH_SQRT2",
            "SQRT2: number",
            "Square root of 2 (~1.4142).",
            __RTS_FN_NS_MATH_SQRT2 as *const u8,
        ))
        .member(cst(
            "SQRT1_2",
            "__RTS_FN_NS_MATH_SQRT1_2",
            "SQRT1_2: number",
            "1/sqrt(2) (~0.7071).",
            __RTS_FN_NS_MATH_SQRT1_2 as *const u8,
        ))
        .member(cst(
            "LN2",
            "__RTS_FN_NS_MATH_LN2",
            "LN2: number",
            "Natural log of 2 (~0.6931).",
            __RTS_FN_NS_MATH_LN2 as *const u8,
        ))
        .member(cst(
            "LN10",
            "__RTS_FN_NS_MATH_LN10",
            "LN10: number",
            "Natural log of 10 (~2.302).",
            __RTS_FN_NS_MATH_LN10 as *const u8,
        ))
        .member(cst(
            "LOG2E",
            "__RTS_FN_NS_MATH_LOG2E",
            "LOG2E: number",
            "Log base 2 of e (~1.4427).",
            __RTS_FN_NS_MATH_LOG2E as *const u8,
        ))
        .member(cst(
            "LOG10E",
            "__RTS_FN_NS_MATH_LOG10E",
            "LOG10E: number",
            "Log base 10 of e (~0.4342).",
            __RTS_FN_NS_MATH_LOG10E as *const u8,
        ))
        .done();
}
