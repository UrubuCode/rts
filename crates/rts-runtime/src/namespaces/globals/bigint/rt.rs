//! BigInt asIntN/asUintN — modular reduction em i64.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BIGINT_AS_INT_N(bits: i64, n: i64) -> i64 {
    // JS spec: result is n mod 2^bits, interpreted as signed (range [-2^(bits-1), 2^(bits-1) - 1]).
    if bits <= 0 { return 0; }
    if bits >= 64 { return n; }
    let bits = bits as u32;
    let modulus = 1i128 << bits;
    let half = 1i128 << (bits - 1);
    let mut r = (n as i128) % modulus;
    if r < 0 { r += modulus; }
    // Range adjust: se r >= 2^(bits-1), subtract modulus pra ficar signed.
    if r >= half { r -= modulus; }
    r as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BIGINT_AS_UINT_N(bits: i64, n: i64) -> i64 {
    // JS spec: result is n mod 2^bits (unsigned). Resultado e' positivo.
    if bits <= 0 { return 0; }
    if bits >= 64 {
        // Para bits == 64, retorna n unsigned interpretado, mas como i64
        // mantemos o bit pattern.
        return n;
    }
    let bits = bits as u32;
    let modulus = 1i128 << bits;
    let mut r = (n as i128) % modulus;
    if r < 0 { r += modulus; }
    r as i64
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
