//! `hash` namespace — non-cryptographic hashing via `std::hash::DefaultHasher`
//! (SipHash-1-3).
//!
//! Uso principal: chaves de HashMap proprio, deduplicacao, checksums rapidos.
//! **Nao use para seguranca** — SipHash resiste a HashDoS mas nao e pre-image
//! resistente. Para SHA/BLAKE veja namespace `crypto`.
//!
//! Migrado pro modelo builder do `rts-engine` (Fase 2; ver `namespaces/hint`).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, sig};

fn hash_slice(bytes: &[u8]) -> i64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish() as i64
}

/// SipHash de uma string UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HASH_HASH_STR(ptr: *const u8, len: i64) -> i64 {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) } {
        Some(s) => s,
        None => return 0,
    };
    hash_slice(s.as_bytes())
}

/// SipHash de uma regiao de memoria (ptr + len). Use com buffer.ptr(handle).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HASH_HASH_BYTES(ptr: i64, len: i64) -> i64 {
    if ptr == 0 || len < 0 {
        return 0;
    }
    // SAFETY: caller passou um ponteiro valido vindo de buffer/gc.
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    hash_slice(slice)
}

/// SipHash de um inteiro de 64 bits.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HASH_HASH_I64(value: i64) -> i64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish() as i64
}

/// Combina dois hashes preservando entropia (estilo boost::hash_combine).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HASH_HASH_COMBINE(h1: i64, h2: i64) -> i64 {
    // Constante = golden ratio truncada. Aritmetica bitwise em u64 pra preservar
    // os bits altos; shift << 6 em i64 sinalizado seria ambiguo em negativos.
    let (a, b) = (h1 as u64, h2 as u64);
    let combined = a ^ b
        .wrapping_add(0x517c_c1b7_2722_0a95)
        .wrapping_add(a << 6)
        .wrapping_add(a >> 2);
    combined as i64
}

/// Membro-função puro com ts/doc explícitos (preserva o que a macro derivava).
fn pure_func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
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
        pure: true,
        emit: None,
    }
}

/// Registra a namespace `hash` no motor (chamado no seed do registry do codegen).
pub fn register(e: &mut Engine) {
    e.ns("hash")
        .doc("Non-cryptographic hashing via std::hash::DefaultHasher (SipHash-1-3).")
        .member(pure_func(
            "hash_str",
            "__RTS_FN_NS_HASH_HASH_STR",
            sig!(StrPtr => I64),
            "hash_str(s: string): number",
            "SipHash de uma string UTF-8.",
            __RTS_FN_NS_HASH_HASH_STR as *const u8,
        ))
        .member(pure_func(
            "hash_bytes",
            "__RTS_FN_NS_HASH_HASH_BYTES",
            sig!(I64, I64 => I64),
            "hash_bytes(ptr: number, len: number): number",
            "SipHash de uma regiao de memoria (ptr + len). Use com buffer.ptr(handle).",
            __RTS_FN_NS_HASH_HASH_BYTES as *const u8,
        ))
        .member(pure_func(
            "hash_i64",
            "__RTS_FN_NS_HASH_HASH_I64",
            sig!(I64 => I64),
            "hash_i64(value: number): number",
            "SipHash de um inteiro de 64 bits.",
            __RTS_FN_NS_HASH_HASH_I64 as *const u8,
        ))
        .member(pure_func(
            "hash_combine",
            "__RTS_FN_NS_HASH_HASH_COMBINE",
            sig!(I64, I64 => I64),
            "hash_combine(h1: number, h2: number): number",
            "Combina dois hashes preservando entropia (estilo boost::hash_combine).",
            __RTS_FN_NS_HASH_HASH_COMBINE as *const u8,
        ))
        .done();
}
