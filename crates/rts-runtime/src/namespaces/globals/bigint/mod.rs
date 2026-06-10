//! `BigInt` global class — stub minimal (cross-runtime #742).
//!
//! RTS nao tem BigInt real (issue #219), tratamos como i64. Essa class
//! expoe os helpers staticos `asIntN(bits, n)` / `asUintN(bits, n)` usados
//! em testes cross-runtime. Migrado ao modelo `#[rts_class]` (stage 5).

use rts_engine::abi::ty::I64;
use rts_macro::rts_class;

/// BigInt namespace (helpers staticos asIntN/asUintN). RTS trata BigInt como i64.
#[rts_class(BigInt)]
impl BigIntClass {
    /// BigInt.asIntN(bits, n) — n modulo 2^bits, interpretado como signed.
    #[rts_fn(
        name = "asIntN",
        ts = "static asIntN(bits: number, n: bigint): bigint",
        pure
    )]
    pub fn as_int_n(bits: I64, n: I64) -> I64 {
        // JS spec: result is n mod 2^bits, interpreted as signed (range [-2^(bits-1), 2^(bits-1) - 1]).
        if bits <= 0 {
            return 0;
        }
        if bits >= 64 {
            return n;
        }
        let bits = bits as u32;
        let modulus = 1i128 << bits;
        let half = 1i128 << (bits - 1);
        let mut r = (n as i128) % modulus;
        if r < 0 {
            r += modulus;
        }
        // Range adjust: se r >= 2^(bits-1), subtract modulus pra ficar signed.
        if r >= half {
            r -= modulus;
        }
        r as i64
    }

    /// BigInt.asUintN(bits, n) — n modulo 2^bits, sem signo.
    #[rts_fn(
        name = "asUintN",
        ts = "static asUintN(bits: number, n: bigint): bigint",
        pure
    )]
    pub fn as_uint_n(bits: I64, n: I64) -> I64 {
        // JS spec: result is n mod 2^bits (unsigned). Resultado e' positivo.
        if bits <= 0 {
            return 0;
        }
        if bits >= 64 {
            // Para bits == 64, retorna n unsigned interpretado, mas como i64
            // mantemos o bit pattern.
            return n;
        }
        let bits = bits as u32;
        let modulus = 1i128 << bits;
        let mut r = (n as i128) % modulus;
        if r < 0 {
            r += modulus;
        }
        r as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_int_n_8_255() {
        assert_eq!(__RTS_FN_GL_BIGINT_AS_INT_N(8, 255), -1);
    }

    #[test]
    fn as_uint_n_8_neg1() {
        assert_eq!(__RTS_FN_GL_BIGINT_AS_UINT_N(8, -1), 255);
    }

    #[test]
    fn as_int_n_16_65535() {
        assert_eq!(__RTS_FN_GL_BIGINT_AS_INT_N(16, 65535), -1);
    }

    #[test]
    fn as_uint_n_16_65536() {
        assert_eq!(__RTS_FN_GL_BIGINT_AS_UINT_N(16, 65536), 0);
    }
}
