//! `BigInt` global class — stub minimal (cross-runtime #742).
//!
//! RTS nao tem BigInt real (issue #219), tratamos como i64. Essa class
//! expoe os helpers staticos `asIntN(bits, n)` / `asUintN(bits, n)` usados
//! em testes cross-runtime.
//!
//! Migrado do `#[rts_class]` (macro) pro modelo builder hand-written do
//! `rts-engine` (rumo à remoção da `rts-macro`). Os externs
//! `__RTS_FN_GL_BIGINT_*` + `register_bigint_class_spec()` são escritos à mão.

use rts_engine::abi::ty::I64;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// BigInt.asIntN(bits, n) — n modulo 2^bits, interpretado como signed.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BIGINT_AS_INT_N(bits: I64, n: I64) -> I64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BIGINT_AS_UINT_N(bits: I64, n: I64) -> I64 {
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

/// Membro de classe global (helper hand-written, espelha `leak_class` da macro).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        emit: None,
    }
}

/// Registra a classe global `BigInt` no motor (hand-written, sem macro).
pub fn register_bigint_class_spec(e: &mut Engine) {
    e.class("BigInt")
        .doc("BigInt namespace (helpers staticos asIntN/asUintN). RTS trata BigInt como i64.")
        .member(m(
            "asIntN",
            MemberKind::Function,
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_BIGINT_AS_INT_N",
            "static asIntN(bits: number, n: bigint): bigint",
            "BigInt.asIntN(bits, n) — n modulo 2^bits, interpretado como signed.",
            __RTS_FN_GL_BIGINT_AS_INT_N as *const u8,
            true,
        ))
        .member(m(
            "asUintN",
            MemberKind::Function,
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_BIGINT_AS_UINT_N",
            "static asUintN(bits: number, n: bigint): bigint",
            "BigInt.asUintN(bits, n) — n modulo 2^bits, sem signo.",
            __RTS_FN_GL_BIGINT_AS_UINT_N as *const u8,
            true,
        ))
        .done();
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
