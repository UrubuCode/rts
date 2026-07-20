//! `num` namespace — explicit-overflow arithmetic (checked/saturating/wrapping)
//! and bit operations.
//!
//! Usa primitivas Rust core/std (i64::checked_*, etc). Overflow em checked_* eh
//! sinalizado retornando i64::MIN como sentinela (caller deve verificar).
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use rts_engine::abi::Intrinsic;
use rts_engine::abi::ty::{F64, I64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

const OVERFLOW_SENTINEL: i64 = i64::MIN;

/// a + b com overflow; retorna i64::MIN como sentinela em overflow.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_CHECKED_ADD(a: I64, b: I64) -> I64 {
    a.checked_add(b).unwrap_or(OVERFLOW_SENTINEL)
}

/// a - b com overflow; retorna i64::MIN como sentinela em overflow.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_CHECKED_SUB(a: I64, b: I64) -> I64 {
    a.checked_sub(b).unwrap_or(OVERFLOW_SENTINEL)
}

/// a * b com overflow; retorna i64::MIN como sentinela em overflow.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_CHECKED_MUL(a: I64, b: I64) -> I64 {
    a.checked_mul(b).unwrap_or(OVERFLOW_SENTINEL)
}

/// a / b; retorna i64::MIN se b == 0 ou overflow (i64::MIN / -1).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_CHECKED_DIV(a: I64, b: I64) -> I64 {
    a.checked_div(b).unwrap_or(OVERFLOW_SENTINEL)
}

/// a + b com saturation em i64::MIN/MAX.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_SATURATING_ADD(a: I64, b: I64) -> I64 {
    a.saturating_add(b)
}

/// a - b com saturation em i64::MIN/MAX.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_SATURATING_SUB(a: I64, b: I64) -> I64 {
    a.saturating_sub(b)
}

/// a * b com saturation em i64::MIN/MAX.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_SATURATING_MUL(a: I64, b: I64) -> I64 {
    a.saturating_mul(b)
}

/// a + b modulo 2^64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_WRAPPING_ADD(a: I64, b: I64) -> I64 {
    a.wrapping_add(b)
}

/// a - b modulo 2^64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_WRAPPING_SUB(a: I64, b: I64) -> I64 {
    a.wrapping_sub(b)
}

/// a * b modulo 2^64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_WRAPPING_MUL(a: I64, b: I64) -> I64 {
    a.wrapping_mul(b)
}

/// -a modulo 2^64 (i64::MIN.wrapping_neg() == i64::MIN).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_WRAPPING_NEG(a: I64) -> I64 {
    a.wrapping_neg()
}

/// a << (n & 63) — shift count masked.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_WRAPPING_SHL(a: I64, n: I64) -> I64 {
    a.wrapping_shl(n as u32)
}

/// a >> (n & 63) (arithmetic shift).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_WRAPPING_SHR(a: I64, n: I64) -> I64 {
    a.wrapping_shr(n as u32)
}

/// Numero de bits 1 em a.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_COUNT_ONES(a: I64) -> I64 {
    a.count_ones() as i64
}

/// Numero de bits 0 em a.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_COUNT_ZEROS(a: I64) -> I64 {
    a.count_zeros() as i64
}

/// Numero de zeros leading em a.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_LEADING_ZEROS(a: I64) -> I64 {
    a.leading_zeros() as i64
}

/// Numero de zeros trailing em a.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_TRAILING_ZEROS(a: I64) -> I64 {
    a.trailing_zeros() as i64
}

/// a rotacionado n bits para a esquerda.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_ROTATE_LEFT(a: I64, n: I64) -> I64 {
    a.rotate_left(n as u32)
}

/// a rotacionado n bits para a direita.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_ROTATE_RIGHT(a: I64, n: I64) -> I64 {
    a.rotate_right(n as u32)
}

/// Bits invertidos (LSB->MSB).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_REVERSE_BITS(a: I64) -> I64 {
    a.reverse_bits()
}

/// Bytes invertidos (endianness flip).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_SWAP_BYTES(a: I64) -> I64 {
    a.swap_bytes()
}

/// Reinterpreta os bits de um i64 como f64 (bit-cast). Util pra recuperar f64 de canais que so passam i64 (ex: thread.spawn arg).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_F64_FROM_BITS(bits: I64) -> F64 {
    f64::from_bits(bits as u64)
}

/// Reinterpreta os bits de um f64 como i64 (bit-cast). Util pra serializar f64 em canais i64-only.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_F64_TO_BITS(value: F64) -> I64 {
    value.to_bits() as i64
}

/// Função `num.f(args)` — sempre `pure: true` nesta namespace.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
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
        pure: true,
        intrinsic: None,
    }
}

/// Marca um membro como INTRINSIC: o codegen emite a instrução Cranelift
/// equivalente em vez do `call <symbol>`.
///
/// O símbolo continua existindo e registrado — a emissão inline é uma otimização
/// que o motor aplica só quando PROVA o `Repr` dos operandos; qualquer outro caso
/// cai no `call` normal. Ver `rts_engine::abi::Intrinsic`, onde cada variante
/// documenta por que a instrução tem semântica IDÊNTICA ao corpo Rust que
/// substitui (as regras de masking de shift/rotate em especial).
fn intr(mut m: Member, i: Intrinsic) -> Member {
    m.intrinsic = Some(i);
    m
}

/// Registra a namespace `num` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("num")
        .doc("Aritmetica com overflow explicito (checked/saturating/wrapping) e bit ops.")
        .member(func(
            "checked_add",
            "__RTS_FN_NS_NUM_CHECKED_ADD",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "checked_add(a: number, b: number): number",
            "a + b com overflow; retorna i64::MIN como sentinela em overflow.",
            __RTS_FN_NS_NUM_CHECKED_ADD as *const u8,
        ))
        .member(func(
            "checked_sub",
            "__RTS_FN_NS_NUM_CHECKED_SUB",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "checked_sub(a: number, b: number): number",
            "a - b com overflow; retorna i64::MIN como sentinela em overflow.",
            __RTS_FN_NS_NUM_CHECKED_SUB as *const u8,
        ))
        .member(func(
            "checked_mul",
            "__RTS_FN_NS_NUM_CHECKED_MUL",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "checked_mul(a: number, b: number): number",
            "a * b com overflow; retorna i64::MIN como sentinela em overflow.",
            __RTS_FN_NS_NUM_CHECKED_MUL as *const u8,
        ))
        .member(func(
            "checked_div",
            "__RTS_FN_NS_NUM_CHECKED_DIV",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "checked_div(a: number, b: number): number",
            "a / b; retorna i64::MIN se b == 0 ou overflow (i64::MIN / -1).",
            __RTS_FN_NS_NUM_CHECKED_DIV as *const u8,
        ))
        .member(func(
            "saturating_add",
            "__RTS_FN_NS_NUM_SATURATING_ADD",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "saturating_add(a: number, b: number): number",
            "a + b com saturation em i64::MIN/MAX.",
            __RTS_FN_NS_NUM_SATURATING_ADD as *const u8,
        ))
        .member(func(
            "saturating_sub",
            "__RTS_FN_NS_NUM_SATURATING_SUB",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "saturating_sub(a: number, b: number): number",
            "a - b com saturation em i64::MIN/MAX.",
            __RTS_FN_NS_NUM_SATURATING_SUB as *const u8,
        ))
        .member(func(
            "saturating_mul",
            "__RTS_FN_NS_NUM_SATURATING_MUL",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "saturating_mul(a: number, b: number): number",
            "a * b com saturation em i64::MIN/MAX.",
            __RTS_FN_NS_NUM_SATURATING_MUL as *const u8,
        ))
        .member(intr(func(
            "wrapping_add",
            "__RTS_FN_NS_NUM_WRAPPING_ADD",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "wrapping_add(a: number, b: number): number",
            "a + b modulo 2^64.",
            __RTS_FN_NS_NUM_WRAPPING_ADD as *const u8,
        ), Intrinsic::WrappingAdd))
        .member(intr(func(
            "wrapping_sub",
            "__RTS_FN_NS_NUM_WRAPPING_SUB",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "wrapping_sub(a: number, b: number): number",
            "a - b modulo 2^64.",
            __RTS_FN_NS_NUM_WRAPPING_SUB as *const u8,
        ), Intrinsic::WrappingSub))
        .member(intr(func(
            "wrapping_mul",
            "__RTS_FN_NS_NUM_WRAPPING_MUL",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "wrapping_mul(a: number, b: number): number",
            "a * b modulo 2^64.",
            __RTS_FN_NS_NUM_WRAPPING_MUL as *const u8,
        ), Intrinsic::WrappingMul))
        .member(intr(func(
            "wrapping_neg",
            "__RTS_FN_NS_NUM_WRAPPING_NEG",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "wrapping_neg(a: number): number",
            "-a modulo 2^64 (i64::MIN.wrapping_neg() == i64::MIN).",
            __RTS_FN_NS_NUM_WRAPPING_NEG as *const u8,
        ), Intrinsic::WrappingNeg))
        .member(intr(func(
            "wrapping_shl",
            "__RTS_FN_NS_NUM_WRAPPING_SHL",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "wrapping_shl(a: number, n: number): number",
            "a << (n & 63) — shift count masked.",
            __RTS_FN_NS_NUM_WRAPPING_SHL as *const u8,
        ), Intrinsic::WrappingShl))
        .member(intr(func(
            "wrapping_shr",
            "__RTS_FN_NS_NUM_WRAPPING_SHR",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "wrapping_shr(a: number, n: number): number",
            "a >> (n & 63) (arithmetic shift).",
            __RTS_FN_NS_NUM_WRAPPING_SHR as *const u8,
        ), Intrinsic::WrappingShr))
        .member(intr(func(
            "count_ones",
            "__RTS_FN_NS_NUM_COUNT_ONES",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "count_ones(a: number): number",
            "Numero de bits 1 em a.",
            __RTS_FN_NS_NUM_COUNT_ONES as *const u8,
        ), Intrinsic::CountOnes))
        .member(intr(func(
            "count_zeros",
            "__RTS_FN_NS_NUM_COUNT_ZEROS",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "count_zeros(a: number): number",
            "Numero de bits 0 em a.",
            __RTS_FN_NS_NUM_COUNT_ZEROS as *const u8,
        ), Intrinsic::CountZeros))
        .member(intr(func(
            "leading_zeros",
            "__RTS_FN_NS_NUM_LEADING_ZEROS",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "leading_zeros(a: number): number",
            "Numero de zeros leading em a.",
            __RTS_FN_NS_NUM_LEADING_ZEROS as *const u8,
        ), Intrinsic::LeadingZeros))
        .member(intr(func(
            "trailing_zeros",
            "__RTS_FN_NS_NUM_TRAILING_ZEROS",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "trailing_zeros(a: number): number",
            "Numero de zeros trailing em a.",
            __RTS_FN_NS_NUM_TRAILING_ZEROS as *const u8,
        ), Intrinsic::TrailingZeros))
        .member(intr(func(
            "rotate_left",
            "__RTS_FN_NS_NUM_ROTATE_LEFT",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "rotate_left(a: number, n: number): number",
            "a rotacionado n bits para a esquerda.",
            __RTS_FN_NS_NUM_ROTATE_LEFT as *const u8,
        ), Intrinsic::RotateLeft))
        .member(intr(func(
            "rotate_right",
            "__RTS_FN_NS_NUM_ROTATE_RIGHT",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "rotate_right(a: number, n: number): number",
            "a rotacionado n bits para a direita.",
            __RTS_FN_NS_NUM_ROTATE_RIGHT as *const u8,
        ), Intrinsic::RotateRight))
        .member(func(
            "reverse_bits",
            "__RTS_FN_NS_NUM_REVERSE_BITS",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "reverse_bits(a: number): number",
            "Bits invertidos (LSB->MSB).",
            __RTS_FN_NS_NUM_REVERSE_BITS as *const u8,
        ))
        .member(intr(func(
            "swap_bytes",
            "__RTS_FN_NS_NUM_SWAP_BYTES",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "swap_bytes(a: number): number",
            "Bytes invertidos (endianness flip).",
            __RTS_FN_NS_NUM_SWAP_BYTES as *const u8,
        ), Intrinsic::SwapBytes))
        .member(func(
            "f64_from_bits",
            "__RTS_FN_NS_NUM_F64_FROM_BITS",
            Sig::new(vec![AbiType::I64], AbiType::F64),
            "f64_from_bits(bits: number): number",
            "Reinterpreta os bits de um i64 como f64 (bit-cast). Util pra recuperar f64 de canais que so passam i64 (ex: thread.spawn arg).",
            __RTS_FN_NS_NUM_F64_FROM_BITS as *const u8,
        ))
        .member(func(
            "f64_to_bits",
            "__RTS_FN_NS_NUM_F64_TO_BITS",
            Sig::new(vec![AbiType::F64], AbiType::I64),
            "f64_to_bits(value: number): number",
            "Reinterpreta os bits de um f64 como i64 (bit-cast). Util pra serializar f64 em canais i64-only.",
            __RTS_FN_NS_NUM_F64_TO_BITS as *const u8,
        ))
        .done();
}
