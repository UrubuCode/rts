//! Faithful copies of the generic operator trampolines, from
//! `adapters/value/genops.rs` and `genops_arith.rs`.
//!
//! The probe only ever feeds them NUMBERS, so each one takes its numeric arm.
//! That is the point: these are the paths a numeric program actually executes,
//! and the question is what the unconditional `call` costs versus an inline
//! guard that handles the same numeric case in IR.

use crate::poly;

/// `__rtsadp_strict_eq`, numeric arm: `av.number_as_f64() == bv.number_as_f64()`.
/// NaN !== NaN and +0 === -0 both fall out of IEEE `==`.
#[inline(never)]
pub extern "C" fn probe_strict_eq(a: i64, b: i64) -> i64 {
    let x = poly::to_number(a as u64);
    let y = poly::to_number(b as u64);
    poly::bool_word(x == y) as i64
}

/// `__rtsadp_loose_eq`. For two numbers the Abstract Equality algorithm defers
/// to the same value compare, after the same-kind dispatch.
#[inline(never)]
pub extern "C" fn probe_loose_eq(a: i64, b: i64) -> i64 {
    let av = a as u64;
    let bv = b as u64;
    let both_num = poly::is_number(av) && poly::is_number(bv);
    let eq = if both_num {
        poly::to_number(av) == poly::to_number(bv)
    } else {
        av == bv
    };
    poly::bool_word(eq) as i64
}

/// `__rtsadp_lt`.
#[inline(never)]
pub extern "C" fn probe_lt(a: i64, b: i64) -> i64 {
    let x = poly::to_number(a as u64);
    let y = poly::to_number(b as u64);
    poly::bool_word(x < y) as i64
}

/// `__rtsadp_mod` — `arith(a, b, %)`. Cranelift has no `frem`, so even the best
/// inline path still calls something for the float case; what an inline guard
/// removes is the boxing and the ToNumber dispatch, not the call itself.
#[inline(never)]
pub extern "C" fn probe_mod(a: i64, b: i64) -> i64 {
    let x = poly::to_number(a as u64);
    let y = poly::to_number(b as u64);
    poly::number_result(x % y) as i64
}

/// `__rtsadp_fmod_f64` — the RAW-f64 form the proven path already uses, so no
/// box/unbox crosses the boundary.
#[inline(never)]
pub extern "C" fn probe_fmod_f64(a: f64, b: f64) -> f64 {
    a % b
}

/// `__rtsadp_mul`.
#[inline(never)]
pub extern "C" fn probe_mul(a: i64, b: i64) -> i64 {
    let x = poly::to_number(a as u64);
    let y = poly::to_number(b as u64);
    poly::number_result(x * y) as i64
}

// --- bitwise / shifts: ToInt32 then the op, result re-boxed ----------------

fn to_int32(w: u64) -> i32 {
    let f = poly::to_number(w);
    if !f.is_finite() {
        return 0;
    }
    (f.trunc() as i64 as u64 as u32) as i32
}

/// `__rtsadp_band`.
#[inline(never)]
pub extern "C" fn probe_band(a: i64, b: i64) -> i64 {
    let r = to_int32(a as u64) & to_int32(b as u64);
    poly::encode(poly::TAG_INT32, r as u32 as u64) as i64
}

/// `__rtsadp_shl`.
#[inline(never)]
pub extern "C" fn probe_shl(a: i64, b: i64) -> i64 {
    let x = to_int32(a as u64);
    let s = (to_int32(b as u64) as u32) & 31;
    poly::encode(poly::TAG_INT32, x.wrapping_shl(s) as u32 as u64) as i64
}

/// `__rtsadp_ushr` — the zero-fill shift, whose uint32 result can exceed
/// `i32::MAX`, so it goes through `number_result` rather than `from_i32`.
#[inline(never)]
pub extern "C" fn probe_ushr(a: i64, b: i64) -> i64 {
    let x = to_int32(a as u64) as u32;
    let s = (to_int32(b as u64) as u32) & 31;
    poly::number_result(x.wrapping_shr(s) as f64) as i64
}

// --- the rest of the binary family -----------------------------------------

/// `__rtsadp_add`, numeric arm. A string operand would take the concat path
/// (`concat_via_real_pool`: ToString both, STRING_NEW, STRING_CONCAT — 3+ locked
/// heap ops), which the STR-APPEND kernel prices separately.
#[inline(never)]
pub extern "C" fn probe_add(a: i64, b: i64) -> i64 {
    let x = poly::to_number(a as u64);
    let y = poly::to_number(b as u64);
    poly::number_result(x + y) as i64
}

#[inline(never)]
pub extern "C" fn probe_sub(a: i64, b: i64) -> i64 {
    let x = poly::to_number(a as u64);
    let y = poly::to_number(b as u64);
    poly::number_result(x - y) as i64
}

#[inline(never)]
pub extern "C" fn probe_div(a: i64, b: i64) -> i64 {
    let x = poly::to_number(a as u64);
    let y = poly::to_number(b as u64);
    poly::number_result(x / y) as i64
}

/// `__rtsadp_pow`. The proven path calls `__RTS_FN_NS_MATH_POW` on raw f64
/// (`binop.rs:644`), so like `%` the CALL survives the proof.
#[inline(never)]
pub extern "C" fn probe_pow(a: i64, b: i64) -> i64 {
    let x = poly::to_number(a as u64);
    let y = poly::to_number(b as u64);
    poly::number_result(x.powf(y)) as i64
}

/// The raw-f64 form the proven path uses.
#[inline(never)]
pub extern "C" fn probe_pow_f64(a: f64, b: f64) -> f64 {
    a.powf(b)
}

#[inline(never)]
pub extern "C" fn probe_gt(a: i64, b: i64) -> i64 {
    poly::bool_word(poly::to_number(a as u64) > poly::to_number(b as u64)) as i64
}

#[inline(never)]
pub extern "C" fn probe_ge(a: i64, b: i64) -> i64 {
    poly::bool_word(poly::to_number(a as u64) >= poly::to_number(b as u64)) as i64
}

#[inline(never)]
pub extern "C" fn probe_le(a: i64, b: i64) -> i64 {
    poly::bool_word(poly::to_number(a as u64) <= poly::to_number(b as u64)) as i64
}

/// `__rtsadp_strict_neq`. The real one is written as `!__rtsadp_strict_eq(a,b)`,
/// but that inner call is an ordinary same-crate call LLVM is free to inline at
/// opt-3 — so the body is replicated here rather than calling the probe's
/// `#[inline(never)]` strict_eq, which would FORCE a second call the real build
/// probably does not make and turn this row into a strawman.
#[inline(never)]
pub extern "C" fn probe_strict_neq(a: i64, b: i64) -> i64 {
    let x = poly::to_number(a as u64);
    let y = poly::to_number(b as u64);
    poly::bool_word(!(x == y)) as i64
}

#[inline(never)]
pub extern "C" fn probe_loose_neq(a: i64, b: i64) -> i64 {
    let av = a as u64;
    let bv = b as u64;
    let eq = if poly::is_number(av) && poly::is_number(bv) {
        poly::to_number(av) == poly::to_number(bv)
    } else {
        av == bv
    };
    poly::bool_word(!eq) as i64
}

#[inline(never)]
pub extern "C" fn probe_bor(a: i64, b: i64) -> i64 {
    let r = to_int32(a as u64) | to_int32(b as u64);
    poly::encode(poly::TAG_INT32, r as u32 as u64) as i64
}

#[inline(never)]
pub extern "C" fn probe_bxor(a: i64, b: i64) -> i64 {
    let r = to_int32(a as u64) ^ to_int32(b as u64);
    poly::encode(poly::TAG_INT32, r as u32 as u64) as i64
}

#[inline(never)]
pub extern "C" fn probe_shr(a: i64, b: i64) -> i64 {
    let x = to_int32(a as u64);
    let s = (to_int32(b as u64) as u32) & 31;
    poly::encode(poly::TAG_INT32, x.wrapping_shr(s) as u32 as u64) as i64
}

// --- unary / special forms -------------------------------------------------

/// `__rtsadp_typeof` — returns a STRING word. The engine const-folds `typeof`
/// for every statically-known operand (`expr.rs:301-390`), so this call is only
/// reached for a genuinely dynamic operand; the probe measures that case.
/// CAVEAT: the real trampoline returns an interned STRING word, and user code
/// then compares it (`typeof x === "number"`). This returns the boolean of that
/// comparison directly, so the row omits the string compare and is a LOWER bound
/// on what `typeof` costs today.
#[inline(never)]
pub extern "C" fn probe_typeof(a: i64) -> i64 {
    poly::bool_word(poly::is_number(a as u64)) as i64
}

/// `__rtsadp_not` — ToBoolean then invert.
#[inline(never)]
pub extern "C" fn probe_not(a: i64) -> i64 {
    let w = a as u64;
    let truthy = if !poly::is_boxed(w) {
        let f = poly::as_f64(w);
        f != 0.0 && !f.is_nan()
    } else {
        (w & poly::PAYLOAD_MASK) == poly::SINGLETON_TRUE
    };
    poly::bool_word(!truthy) as i64
}

/// `__rtsadp_neg` — unary `-`, ToNumber then negate, re-tightened.
#[inline(never)]
pub extern "C" fn probe_neg(a: i64) -> i64 {
    poly::number_result(-poly::to_number(a as u64)) as i64
}

pub fn symbols() -> Vec<(&'static str, *const u8)> {
    vec![
        ("probe_strict_eq", probe_strict_eq as *const u8),
        ("probe_loose_eq", probe_loose_eq as *const u8),
        ("probe_lt", probe_lt as *const u8),
        ("probe_mod", probe_mod as *const u8),
        ("probe_fmod_f64", probe_fmod_f64 as *const u8),
        ("probe_mul", probe_mul as *const u8),
        ("probe_band", probe_band as *const u8),
        ("probe_shl", probe_shl as *const u8),
        ("probe_ushr", probe_ushr as *const u8),
        ("probe_add", probe_add as *const u8),
        ("probe_sub", probe_sub as *const u8),
        ("probe_div", probe_div as *const u8),
        ("probe_pow", probe_pow as *const u8),
        ("probe_pow_f64", probe_pow_f64 as *const u8),
        ("probe_gt", probe_gt as *const u8),
        ("probe_ge", probe_ge as *const u8),
        ("probe_le", probe_le as *const u8),
        ("probe_strict_neq", probe_strict_neq as *const u8),
        ("probe_loose_neq", probe_loose_neq as *const u8),
        ("probe_bor", probe_bor as *const u8),
        ("probe_bxor", probe_bxor as *const u8),
        ("probe_shr", probe_shr as *const u8),
        ("probe_typeof", probe_typeof as *const u8),
        ("probe_not", probe_not as *const u8),
        ("probe_neg", probe_neg as *const u8),
    ]
}
