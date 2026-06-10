//! `num` namespace — explicit-overflow arithmetic (checked/saturating/wrapping)
//! and bit operations.
//!
//! Usa primitivas Rust core/std (i64::checked_*, etc). Overflow em checked_* eh
//! sinalizado retornando i64::MIN como sentinela (caller deve verificar).
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use rts_engine::abi::ty::{F64, I64};
use rts_macro::rts_namespace;

const OVERFLOW_SENTINEL: i64 = i64::MIN;

/// Aritmetica com overflow explicito (checked/saturating/wrapping) e bit ops.
#[rts_namespace(num)]
impl NumNs {
    /// a + b com overflow; retorna i64::MIN como sentinela em overflow.
    #[rts_fn(pure)]
    pub fn checked_add(a: I64, b: I64) -> I64 {
        a.checked_add(b).unwrap_or(OVERFLOW_SENTINEL)
    }

    /// a - b com overflow; retorna i64::MIN como sentinela em overflow.
    #[rts_fn(pure)]
    pub fn checked_sub(a: I64, b: I64) -> I64 {
        a.checked_sub(b).unwrap_or(OVERFLOW_SENTINEL)
    }

    /// a * b com overflow; retorna i64::MIN como sentinela em overflow.
    #[rts_fn(pure)]
    pub fn checked_mul(a: I64, b: I64) -> I64 {
        a.checked_mul(b).unwrap_or(OVERFLOW_SENTINEL)
    }

    /// a / b; retorna i64::MIN se b == 0 ou overflow (i64::MIN / -1).
    #[rts_fn(pure)]
    pub fn checked_div(a: I64, b: I64) -> I64 {
        a.checked_div(b).unwrap_or(OVERFLOW_SENTINEL)
    }

    /// a + b com saturation em i64::MIN/MAX.
    #[rts_fn(pure)]
    pub fn saturating_add(a: I64, b: I64) -> I64 {
        a.saturating_add(b)
    }

    /// a - b com saturation em i64::MIN/MAX.
    #[rts_fn(pure)]
    pub fn saturating_sub(a: I64, b: I64) -> I64 {
        a.saturating_sub(b)
    }

    /// a * b com saturation em i64::MIN/MAX.
    #[rts_fn(pure)]
    pub fn saturating_mul(a: I64, b: I64) -> I64 {
        a.saturating_mul(b)
    }

    /// a + b modulo 2^64.
    #[rts_fn(pure)]
    pub fn wrapping_add(a: I64, b: I64) -> I64 {
        a.wrapping_add(b)
    }

    /// a - b modulo 2^64.
    #[rts_fn(pure)]
    pub fn wrapping_sub(a: I64, b: I64) -> I64 {
        a.wrapping_sub(b)
    }

    /// a * b modulo 2^64.
    #[rts_fn(pure)]
    pub fn wrapping_mul(a: I64, b: I64) -> I64 {
        a.wrapping_mul(b)
    }

    /// -a modulo 2^64 (i64::MIN.wrapping_neg() == i64::MIN).
    #[rts_fn(pure)]
    pub fn wrapping_neg(a: I64) -> I64 {
        a.wrapping_neg()
    }

    /// a << (n & 63) — shift count masked.
    #[rts_fn(pure)]
    pub fn wrapping_shl(a: I64, n: I64) -> I64 {
        a.wrapping_shl(n as u32)
    }

    /// a >> (n & 63) (arithmetic shift).
    #[rts_fn(pure)]
    pub fn wrapping_shr(a: I64, n: I64) -> I64 {
        a.wrapping_shr(n as u32)
    }

    /// Numero de bits 1 em a.
    #[rts_fn(pure)]
    pub fn count_ones(a: I64) -> I64 {
        a.count_ones() as i64
    }

    /// Numero de bits 0 em a.
    #[rts_fn(pure)]
    pub fn count_zeros(a: I64) -> I64 {
        a.count_zeros() as i64
    }

    /// Numero de zeros leading em a.
    #[rts_fn(pure)]
    pub fn leading_zeros(a: I64) -> I64 {
        a.leading_zeros() as i64
    }

    /// Numero de zeros trailing em a.
    #[rts_fn(pure)]
    pub fn trailing_zeros(a: I64) -> I64 {
        a.trailing_zeros() as i64
    }

    /// a rotacionado n bits para a esquerda.
    #[rts_fn(pure)]
    pub fn rotate_left(a: I64, n: I64) -> I64 {
        a.rotate_left(n as u32)
    }

    /// a rotacionado n bits para a direita.
    #[rts_fn(pure)]
    pub fn rotate_right(a: I64, n: I64) -> I64 {
        a.rotate_right(n as u32)
    }

    /// Bits invertidos (LSB->MSB).
    #[rts_fn(pure)]
    pub fn reverse_bits(a: I64) -> I64 {
        a.reverse_bits()
    }

    /// Bytes invertidos (endianness flip).
    #[rts_fn(pure)]
    pub fn swap_bytes(a: I64) -> I64 {
        a.swap_bytes()
    }

    /// Reinterpreta os bits de um i64 como f64 (bit-cast). Util pra recuperar f64 de canais que so passam i64 (ex: thread.spawn arg).
    #[rts_fn(pure)]
    pub fn f64_from_bits(bits: I64) -> F64 {
        f64::from_bits(bits as u64)
    }

    /// Reinterpreta os bits de um f64 como i64 (bit-cast). Util pra serializar f64 em canais i64-only.
    #[rts_fn(pure)]
    pub fn f64_to_bits(value: F64) -> I64 {
        value.to_bits() as i64
    }
}
