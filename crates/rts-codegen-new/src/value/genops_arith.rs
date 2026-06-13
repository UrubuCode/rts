//! Generic JS arithmetic / comparison / unary / bitwise operators on
//! [`PolyValue`] words (P4.8) — the ONE tag-dispatched path the lowering routes
//! to whenever an operand is `Tagged` (untyped / `any` / unknown), keeping the
//! proven-numeric fast path native.
//!
//! Each is `extern "C" fn(a: u64[, b: u64]) -> u64` over raw PolyValue words
//! (tagged-in/tagged-out). They reuse [`super::genops`]'s ToNumber / ToString /
//! ToBoolean so coercion + number formatting NEVER diverge from the `+`/`===`
//! path.
//!
//! ## Why these are reachable but `==`/`!`/`+` are NOT
//!
//! swc collapses `==`/`===` onto one HIR op and `!`/unary-`+` onto one HIR op, so
//! the lowering CANNOT tell them apart and must bail (the honesty floor — see
//! `front/run/expr.rs`). The operators here (`- * / % **`, `< <= > >=`, unary
//! `-`, `~`, `& | ^ << >> >>>`) each map to a DISTINCT HIR op, so they are sound
//! to lower generically. `__rtsadp_strict_eq`/`_neq` already live in
//! [`super::genops`]; equality is not re-added here.

use super::genops::{number_result, string_content, to_boolean, to_number};
use super::PolyValue;

// ===========================================================================
// Arithmetic — both operands ToNumber, result a PolyValue number.
// ===========================================================================

/// Numeric `a <op> b` where `op` is one of the f64 binary arithmetic ops; the
/// result is re-tightened to int32 when exact-in-range (via [`number_result`]).
#[inline]
fn arith(a: u64, b: u64, f: impl Fn(f64, f64) -> f64) -> u64 {
    let x = to_number(PolyValue::from_raw(a));
    let y = to_number(PolyValue::from_raw(b));
    number_result(f(x, y)).raw()
}

/// `__rtsadp_sub` — JS `-` (ToNumber both, subtract). NaN where either is NaN.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_sub(a: u64, b: u64) -> u64 {
    arith(a, b, |x, y| x - y)
}

/// `__rtsadp_mul` — JS `*`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_mul(a: u64, b: u64) -> u64 {
    arith(a, b, |x, y| x * y)
}

/// `__rtsadp_div` — JS `/` (IEEE division; `1/0 = Infinity`, `0/0 = NaN`).
/// Always yields a double-style result unless it happens to be an exact small
/// integer (e.g. `6/2 = 3` → int32; `5/2 = 2.5` → double).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_div(a: u64, b: u64) -> u64 {
    arith(a, b, |x, y| x / y)
}

/// `__rtsadp_mod` — JS `%`. JS remainder takes the **sign of the dividend** and
/// is `fmod`-style (NOT Euclidean): `-7 % 3 == -1`, `7 % -3 == 1`. Rust's `%` on
/// `f64` has exactly these semantics, so it is the faithful primitive. `x % 0`
/// and `Infinity % y` are `NaN`, which `f64::%` also produces.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_mod(a: u64, b: u64) -> u64 {
    arith(a, b, |x, y| x % y)
}

/// `__rtsadp_pow` — JS `**` (`Math.pow` semantics via `f64::powf`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_pow(a: u64, b: u64) -> u64 {
    arith(a, b, |x, y| x.powf(y))
}

// ===========================================================================
// Relational comparison — `< <= > >=`, result a PolyValue bool.
// ===========================================================================

/// The JS Abstract Relational Comparison, returning `Some(less)` or `None` when
/// the comparison is undefined (a `NaN` operand). Both-string → byte-lexicographic
/// (JS compares by UTF-16 code unit; for the ASCII/BMP fixtures byte order
/// agrees — full UTF-16 ordering is a later increment); otherwise ToNumber both.
fn relational_less(a: PolyValue, b: PolyValue) -> Option<bool> {
    if a.is_string() && b.is_string() {
        return Some(string_content(a) < string_content(b));
    }
    let x = to_number(a);
    let y = to_number(b);
    if x.is_nan() || y.is_nan() {
        return None;
    }
    Some(x < y)
}

/// Emit a PolyValue bool from a relational op over `(a, b)`. `op` selects which
/// of `< <= > >=` by mapping the primitive `less`/`equal` facts; a `NaN` operand
/// makes every relational op `false` (JS).
#[inline]
fn relational(a: u64, b: u64, want: Rel) -> u64 {
    let av = PolyValue::from_raw(a);
    let bv = PolyValue::from_raw(b);
    let result = match want {
        Rel::Lt => relational_less(av, bv).unwrap_or(false),
        Rel::Gt => relational_less(bv, av).unwrap_or(false),
        // a <= b  ≡  !(b < a)  — but a NaN operand must yield false, so guard.
        Rel::Le => match relational_less(bv, av) {
            Some(b_lt_a) => !b_lt_a,
            None => false,
        },
        // a >= b  ≡  !(a < b), with the same NaN guard.
        Rel::Ge => match relational_less(av, bv) {
            Some(a_lt_b) => !a_lt_b,
            None => false,
        },
    };
    PolyValue::bool(result).raw()
}

#[derive(Clone, Copy)]
enum Rel {
    Lt,
    Le,
    Gt,
    Ge,
}

/// `__rtsadp_lt` — JS `<`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_lt(a: u64, b: u64) -> u64 {
    relational(a, b, Rel::Lt)
}

/// `__rtsadp_le` — JS `<=`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_le(a: u64, b: u64) -> u64 {
    relational(a, b, Rel::Le)
}

/// `__rtsadp_gt` — JS `>`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_gt(a: u64, b: u64) -> u64 {
    relational(a, b, Rel::Gt)
}

/// `__rtsadp_ge` — JS `>=`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_ge(a: u64, b: u64) -> u64 {
    relational(a, b, Rel::Ge)
}

// ===========================================================================
// Unary — `-` and `~`, result a PolyValue.
// ===========================================================================

/// `__rtsadp_neg` — JS unary `-` (ToNumber then negate). `-"3"` → `-3`,
/// `-true` → `-1`, `-"x"` → `NaN`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_neg(a: u64) -> u64 {
    let x = to_number(PolyValue::from_raw(a));
    number_result(-x).raw()
}

/// `__rtsadp_bnot` — JS `~` (`ToInt32` then bitwise NOT). The result is always a
/// small integer, so it boxes as int32.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_bnot(a: u64) -> u64 {
    let i = to_int32(PolyValue::from_raw(a));
    PolyValue::from_i32(!i).raw()
}

// ===========================================================================
// Bitwise + shifts — JS ToInt32 / ToUint32 semantics, result int32/uint32.
// ===========================================================================

/// JS `ToInt32`: ToNumber, then take the value mod 2^32 as a signed 32-bit int
/// (NaN/±Infinity → 0). The `as i64 as i32` wrap reproduces the modular step for
/// finite in-range magnitudes; out-of-range/non-finite go through 0.
fn to_int32(v: PolyValue) -> i32 {
    let f = to_number(v);
    if !f.is_finite() {
        return 0;
    }
    // JS truncates toward zero, then takes the low 32 bits (modular 2^32).
    let trunc = f.trunc();
    (trunc as i64 as u64 as u32) as i32
}

/// JS `ToUint32` — same modular step, read as unsigned.
fn to_uint32(v: PolyValue) -> u32 {
    to_int32(v) as u32
}

/// `a <bitop> b` over `ToInt32`, result a (signed) int32 PolyValue.
#[inline]
fn bitop(a: u64, b: u64, f: impl Fn(i32, i32) -> i32) -> u64 {
    let x = to_int32(PolyValue::from_raw(a));
    let y = to_int32(PolyValue::from_raw(b));
    PolyValue::from_i32(f(x, y)).raw()
}

/// `__rtsadp_band` — JS `&`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_band(a: u64, b: u64) -> u64 {
    bitop(a, b, |x, y| x & y)
}

/// `__rtsadp_bor` — JS `|`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_bor(a: u64, b: u64) -> u64 {
    bitop(a, b, |x, y| x | y)
}

/// `__rtsadp_bxor` — JS `^`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_bxor(a: u64, b: u64) -> u64 {
    bitop(a, b, |x, y| x ^ y)
}

/// `__rtsadp_shl` — JS `<<` (left operand `ToInt32`, right `& 31`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_shl(a: u64, b: u64) -> u64 {
    let x = to_int32(PolyValue::from_raw(a));
    let shift = to_uint32(PolyValue::from_raw(b)) & 31;
    PolyValue::from_i32(x.wrapping_shl(shift)).raw()
}

/// `__rtsadp_shr` — JS `>>` (sign-propagating arithmetic shift).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_shr(a: u64, b: u64) -> u64 {
    let x = to_int32(PolyValue::from_raw(a));
    let shift = to_uint32(PolyValue::from_raw(b)) & 31;
    PolyValue::from_i32(x.wrapping_shr(shift)).raw()
}

/// `__rtsadp_ushr` — JS `>>>` (zero-fill: left operand `ToUint32`). The result is
/// a JS number in `0..2^32`, which may exceed `i32::MAX` — box as a double in
/// that case so the value is preserved.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_ushr(a: u64, b: u64) -> u64 {
    let x = to_uint32(PolyValue::from_raw(a));
    let shift = to_uint32(PolyValue::from_raw(b)) & 31;
    number_result((x.wrapping_shr(shift)) as f64).raw()
}

/// `__rtsadp_not` — JS logical `!` (ToBoolean then invert). NOT wired from the
/// HIR (swc conflates `!`/unary-`+`), but provided as the honest primitive so a
/// future AST-aware front-end can use it without re-deriving ToBoolean.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_not(a: u64) -> u64 {
    PolyValue::bool(!to_boolean(PolyValue::from_raw(a))).raw()
}
