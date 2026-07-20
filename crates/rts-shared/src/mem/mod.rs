//! `mem` namespace — std::mem: layout (size_of/align_of), swap, drop, forget.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr). Os `size_of_*` /
//! `align_of_*` são constantes (`mem.size_of_i64`, sem parênteses).

use rts_engine::abi::ty::{Handle, I64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Tamanho em bytes de um i64 (= 8).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_SIZE_OF_I64() -> I64 {
    std::mem::size_of::<i64>() as i64
}

/// Tamanho em bytes de um f64 (= 8).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_SIZE_OF_F64() -> I64 {
    std::mem::size_of::<f64>() as i64
}

/// Tamanho em bytes de um i32 (= 4).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_SIZE_OF_I32() -> I64 {
    std::mem::size_of::<i32>() as i64
}

/// Tamanho em bytes de um bool (= 1).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_SIZE_OF_BOOL() -> I64 {
    std::mem::size_of::<bool>() as i64
}

/// Alinhamento de i64 (= 8).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_ALIGN_OF_I64() -> I64 {
    std::mem::align_of::<i64>() as i64
}

/// Alinhamento de f64 (= 8).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_ALIGN_OF_F64() -> I64 {
    std::mem::align_of::<f64>() as i64
}

/// Retorna `b` (use idiom: `let old = mem.swap_i64(a, b)`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_SWAP_I64(_a: I64, b: I64) -> I64 {
    // RTS sem refs nao oferece swap-by-pointer real; retorna `b` para enfatizar
    // a operacao (caller faz a atribuicao).
    b
}

/// Forca free de um handle GC. Equivalente a gc.string_free / etc.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_DROP_HANDLE(h: Handle) {
    let _ = rts_engine::heap::handles::free_handle(h);
}

/// Esquece handle sem rodar drop — vaza memoria intencionalmente.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_FORGET_HANDLE(_h: Handle) {
    // Intencional: nao chama free.
}

/// Idiom: `mem.replace_i64(slot, new)` — retorna slot e usa caller pra escrever.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_REPLACE_I64(slot: I64, _new_val: I64) -> I64 {
    slot
}

/// Constante `mem.K` (zero-arg, sem parênteses) — getter pure retornando I64.
fn cst(name: &str, symbol: &str, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Constant,
        sig: Sig::new(Vec::new(), AbiType::I64),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
        emit: None,
    }
}

/// Função `mem.f(args)`.
fn func(
    name: &str,
    symbol: &str,
    sig: Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
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
        intrinsic: None,
        emit: None,
    }
}

/// Registra a namespace `mem` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("mem")
        .doc("std::mem: layout (size_of/align_of), swap, drop, forget.")
        .member(cst(
            "size_of_i64",
            "__RTS_FN_NS_MEM_SIZE_OF_I64",
            "size_of_i64: number",
            "Tamanho em bytes de um i64 (= 8).",
            __RTS_FN_NS_MEM_SIZE_OF_I64 as *const u8,
        ))
        .member(cst(
            "size_of_f64",
            "__RTS_FN_NS_MEM_SIZE_OF_F64",
            "size_of_f64: number",
            "Tamanho em bytes de um f64 (= 8).",
            __RTS_FN_NS_MEM_SIZE_OF_F64 as *const u8,
        ))
        .member(cst(
            "size_of_i32",
            "__RTS_FN_NS_MEM_SIZE_OF_I32",
            "size_of_i32: number",
            "Tamanho em bytes de um i32 (= 4).",
            __RTS_FN_NS_MEM_SIZE_OF_I32 as *const u8,
        ))
        .member(cst(
            "size_of_bool",
            "__RTS_FN_NS_MEM_SIZE_OF_BOOL",
            "size_of_bool: number",
            "Tamanho em bytes de um bool (= 1).",
            __RTS_FN_NS_MEM_SIZE_OF_BOOL as *const u8,
        ))
        .member(cst(
            "align_of_i64",
            "__RTS_FN_NS_MEM_ALIGN_OF_I64",
            "align_of_i64: number",
            "Alinhamento de i64 (= 8).",
            __RTS_FN_NS_MEM_ALIGN_OF_I64 as *const u8,
        ))
        .member(cst(
            "align_of_f64",
            "__RTS_FN_NS_MEM_ALIGN_OF_F64",
            "align_of_f64: number",
            "Alinhamento de f64 (= 8).",
            __RTS_FN_NS_MEM_ALIGN_OF_F64 as *const u8,
        ))
        .member(func(
            "swap_i64",
            "__RTS_FN_NS_MEM_SWAP_I64",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "swap_i64(a: number, b: number): number",
            "Retorna `b` (use idiom: `let old = mem.swap_i64(a, b)`).",
            __RTS_FN_NS_MEM_SWAP_I64 as *const u8,
            true,
        ))
        .member(func(
            "drop_handle",
            "__RTS_FN_NS_MEM_DROP_HANDLE",
            Sig::new(vec![AbiType::Handle], AbiType::Void),
            "drop_handle(h: number): void",
            "Forca free de um handle GC. Equivalente a gc.string_free / etc.",
            __RTS_FN_NS_MEM_DROP_HANDLE as *const u8,
            false,
        ))
        .member(func(
            "forget_handle",
            "__RTS_FN_NS_MEM_FORGET_HANDLE",
            Sig::new(vec![AbiType::Handle], AbiType::Void),
            "forget_handle(h: number): void",
            "Esquece handle sem rodar drop — vaza memoria intencionalmente.",
            __RTS_FN_NS_MEM_FORGET_HANDLE as *const u8,
            false,
        ))
        .member(func(
            "replace_i64",
            "__RTS_FN_NS_MEM_REPLACE_I64",
            Sig::new(vec![AbiType::I64, AbiType::I64], AbiType::I64),
            "replace_i64(slot: number, new_val: number): number",
            "Idiom: `mem.replace_i64(slot, new)` — retorna slot e usa caller pra escrever.",
            __RTS_FN_NS_MEM_REPLACE_I64 as *const u8,
            false,
        ))
        .done();
}
