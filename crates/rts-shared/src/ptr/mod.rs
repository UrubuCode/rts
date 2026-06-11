//! `ptr` namespace — raw pointer operations (std::ptr).
//!
//! Toda funcao eh `unsafe` por natureza — caller eh responsavel por
//! validez/alinhamento/lifetime. Use com `buffer.ptr(handle)` para ler/escrever
//! buffers. Enderecos viajam como i64.
//!
//! Migrado pro modelo builder do `rts-engine` (Fase 2; ver `namespaces/hint`).

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

/// Retorna ponteiro nulo (0).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_NULL() -> i64 {
    0
}

/// True se ptr == 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_IS_NULL(p: i64) -> i64 {
    if p == 0 { 1 } else { 0 }
}

/// Le i64 do endereco. UNSAFE: caller garante validade/alinhamento.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_READ_I64(p: i64) -> i64 {
    if p == 0 {
        return 0;
    }
    unsafe { std::ptr::read_unaligned(p as *const i64) }
}

/// Le i32 do endereco e estende para i64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_READ_I32(p: i64) -> i64 {
    if p == 0 {
        return 0;
    }
    let v = unsafe { std::ptr::read_unaligned(p as *const i32) };
    v as i64
}

/// Le u8 do endereco e estende para i64 (0..255).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_READ_U8(p: i64) -> i64 {
    if p == 0 {
        return 0;
    }
    let v = unsafe { std::ptr::read_unaligned(p as *const u8) };
    v as i64
}

/// Le f64 do endereco.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_READ_F64(p: i64) -> f64 {
    if p == 0 {
        return 0.0;
    }
    unsafe { std::ptr::read_unaligned(p as *const f64) }
}

/// Escreve i64 no endereco.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_WRITE_I64(p: i64, value: i64) {
    if p == 0 {
        return;
    }
    unsafe { std::ptr::write_unaligned(p as *mut i64, value) };
}

/// Escreve i32 (low 32 bits) no endereco.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_WRITE_I32(p: i64, value: i64) {
    if p == 0 {
        return;
    }
    unsafe { std::ptr::write_unaligned(p as *mut i32, value as i32) };
}

/// Escreve u8 (low 8 bits) no endereco.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_WRITE_U8(p: i64, value: i64) {
    if p == 0 {
        return;
    }
    unsafe { std::ptr::write_unaligned(p as *mut u8, value as u8) };
}

/// Escreve f64 no endereco.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_WRITE_F64(p: i64, value: f64) {
    if p == 0 {
        return;
    }
    unsafe { std::ptr::write_unaligned(p as *mut f64, value) };
}

/// memmove: copia n bytes de src para dst (overlapping ok).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_COPY(dst: i64, src: i64, n: i64) {
    if dst == 0 || src == 0 || n <= 0 {
        return;
    }
    unsafe { std::ptr::copy(src as *const u8, dst as *mut u8, n as usize) };
}

/// memcpy: copia n bytes (regioes nao podem se sobrepor).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_COPY_NONOVERLAPPING(dst: i64, src: i64, n: i64) {
    if dst == 0 || src == 0 || n <= 0 {
        return;
    }
    unsafe { std::ptr::copy_nonoverlapping(src as *const u8, dst as *mut u8, n as usize) };
}

/// memset: preenche n bytes com value (low 8 bits).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_WRITE_BYTES(dst: i64, value: i64, n: i64) {
    if dst == 0 || n <= 0 {
        return;
    }
    unsafe { std::ptr::write_bytes(dst as *mut u8, value as u8, n as usize) };
}

/// Adiciona n bytes ao ptr.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PTR_OFFSET(p: i64, n: i64) -> i64 {
    p.wrapping_add(n)
}

fn func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
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
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace `ptr` no motor (Fase 2).
pub fn register(e: &mut Engine) {
    e.ns("ptr")
        .doc("Operacoes raw sobre ponteiros (std::ptr). UNSAFE — caller verifica validez.")
        .member(func("null", "__RTS_FN_NS_PTR_NULL", sig!(=> I64), "null(): number", "Retorna ponteiro nulo (0).", __RTS_FN_NS_PTR_NULL as *const u8))
        .member(func("is_null", "__RTS_FN_NS_PTR_IS_NULL", sig!(I64 => Bool), "is_null(p: number): boolean", "True se ptr == 0.", __RTS_FN_NS_PTR_IS_NULL as *const u8))
        .member(func("read_i64", "__RTS_FN_NS_PTR_READ_I64", sig!(I64 => I64), "read_i64(p: number): number", "Le i64 do endereco. UNSAFE: caller garante validade/alinhamento.", __RTS_FN_NS_PTR_READ_I64 as *const u8))
        .member(func("read_i32", "__RTS_FN_NS_PTR_READ_I32", sig!(I64 => I64), "read_i32(p: number): number", "Le i32 do endereco e estende para i64.", __RTS_FN_NS_PTR_READ_I32 as *const u8))
        .member(func("read_u8", "__RTS_FN_NS_PTR_READ_U8", sig!(I64 => I64), "read_u8(p: number): number", "Le u8 do endereco e estende para i64 (0..255).", __RTS_FN_NS_PTR_READ_U8 as *const u8))
        .member(func("read_f64", "__RTS_FN_NS_PTR_READ_F64", sig!(I64 => F64), "read_f64(p: number): number", "Le f64 do endereco.", __RTS_FN_NS_PTR_READ_F64 as *const u8))
        .member(func("write_i64", "__RTS_FN_NS_PTR_WRITE_I64", sig!(I64, I64 => Void), "write_i64(p: number, value: number): void", "Escreve i64 no endereco.", __RTS_FN_NS_PTR_WRITE_I64 as *const u8))
        .member(func("write_i32", "__RTS_FN_NS_PTR_WRITE_I32", sig!(I64, I64 => Void), "write_i32(p: number, value: number): void", "Escreve i32 (low 32 bits) no endereco.", __RTS_FN_NS_PTR_WRITE_I32 as *const u8))
        .member(func("write_u8", "__RTS_FN_NS_PTR_WRITE_U8", sig!(I64, I64 => Void), "write_u8(p: number, value: number): void", "Escreve u8 (low 8 bits) no endereco.", __RTS_FN_NS_PTR_WRITE_U8 as *const u8))
        .member(func("write_f64", "__RTS_FN_NS_PTR_WRITE_F64", sig!(I64, F64 => Void), "write_f64(p: number, value: number): void", "Escreve f64 no endereco.", __RTS_FN_NS_PTR_WRITE_F64 as *const u8))
        .member(func("copy", "__RTS_FN_NS_PTR_COPY", sig!(I64, I64, I64 => Void), "copy(dst: number, src: number, n: number): void", "memmove: copia n bytes de src para dst (overlapping ok).", __RTS_FN_NS_PTR_COPY as *const u8))
        .member(func("copy_nonoverlapping", "__RTS_FN_NS_PTR_COPY_NONOVERLAPPING", sig!(I64, I64, I64 => Void), "copy_nonoverlapping(dst: number, src: number, n: number): void", "memcpy: copia n bytes (regioes nao podem se sobrepor).", __RTS_FN_NS_PTR_COPY_NONOVERLAPPING as *const u8))
        .member(func("write_bytes", "__RTS_FN_NS_PTR_WRITE_BYTES", sig!(I64, I64, I64 => Void), "write_bytes(dst: number, value: number, n: number): void", "memset: preenche n bytes com value (low 8 bits).", __RTS_FN_NS_PTR_WRITE_BYTES as *const u8))
        .member(func("offset", "__RTS_FN_NS_PTR_OFFSET", sig!(I64, I64 => I64), "offset(p: number, n: number): number", "Adiciona n bytes ao ptr.", __RTS_FN_NS_PTR_OFFSET as *const u8))
        .done();
}
