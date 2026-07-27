//! `ptr` namespace — raw pointer operations (std::ptr).
//!
//! Toda funcao eh `unsafe` por natureza — caller eh responsavel por
//! validez/alinhamento/lifetime. Use com `buffer.ptr(handle)` para ler/escrever
//! buffers. Enderecos viajam como i64.
//!
//! Os membros ABI sao declarados com `#[rtse::function]` (F7 de
//! `docs/specs/rts-macro-single-source.md`): simbolo, assinatura, `ts_signature`
//! e fn-ptr saem derivados da fn Rust.

use rts_engine::abi::ty::{Bool, F64, I64};
use rts_engine::Engine;

/// Retorna ponteiro nulo (0).
#[rtse::function(module = "ptr", value = "null")]
pub fn null() -> I64 {
    0
}

/// True se ptr == 0.
#[rtse::function(module = "ptr", value = "is_null")]
pub fn is_null(p: I64) -> Bool {
    if p == 0 { 1 } else { 0 }
}

/// Le i64 do endereco. UNSAFE: caller garante validade/alinhamento.
#[rtse::function(module = "ptr", value = "read_i64")]
pub fn read_i64(p: I64) -> I64 {
    if p == 0 {
        return 0;
    }
    unsafe { std::ptr::read_unaligned(p as *const i64) }
}

/// Le i32 do endereco e estende para i64.
#[rtse::function(module = "ptr", value = "read_i32")]
pub fn read_i32(p: I64) -> I64 {
    if p == 0 {
        return 0;
    }
    let v = unsafe { std::ptr::read_unaligned(p as *const i32) };
    v as i64
}

/// Le u8 do endereco e estende para i64 (0..255).
#[rtse::function(module = "ptr", value = "read_u8")]
pub fn read_u8(p: I64) -> I64 {
    if p == 0 {
        return 0;
    }
    let v = unsafe { std::ptr::read_unaligned(p as *const u8) };
    v as i64
}

/// Le f64 do endereco.
#[rtse::function(module = "ptr", value = "read_f64")]
pub fn read_f64(p: I64) -> F64 {
    if p == 0 {
        return 0.0;
    }
    unsafe { std::ptr::read_unaligned(p as *const f64) }
}

/// Escreve i64 no endereco.
#[rtse::function(module = "ptr", value = "write_i64")]
pub fn write_i64(p: I64, value: I64) {
    if p == 0 {
        return;
    }
    unsafe { std::ptr::write_unaligned(p as *mut i64, value) };
}

/// Escreve i32 (low 32 bits) no endereco.
#[rtse::function(module = "ptr", value = "write_i32")]
pub fn write_i32(p: I64, value: I64) {
    if p == 0 {
        return;
    }
    unsafe { std::ptr::write_unaligned(p as *mut i32, value as i32) };
}

/// Escreve u8 (low 8 bits) no endereco.
#[rtse::function(module = "ptr", value = "write_u8")]
pub fn write_u8(p: I64, value: I64) {
    if p == 0 {
        return;
    }
    unsafe { std::ptr::write_unaligned(p as *mut u8, value as u8) };
}

/// Escreve f64 no endereco.
#[rtse::function(module = "ptr", value = "write_f64")]
pub fn write_f64(p: I64, value: F64) {
    if p == 0 {
        return;
    }
    unsafe { std::ptr::write_unaligned(p as *mut f64, value) };
}

/// memmove: copia n bytes de src para dst (overlapping ok).
#[rtse::function(module = "ptr", value = "copy")]
pub fn copy(dst: I64, src: I64, n: I64) {
    if dst == 0 || src == 0 || n <= 0 {
        return;
    }
    unsafe { std::ptr::copy(src as *const u8, dst as *mut u8, n as usize) };
}

/// memcpy: copia n bytes (regioes nao podem se sobrepor).
#[rtse::function(module = "ptr", value = "copy_nonoverlapping")]
pub fn copy_nonoverlapping(dst: I64, src: I64, n: I64) {
    if dst == 0 || src == 0 || n <= 0 {
        return;
    }
    unsafe { std::ptr::copy_nonoverlapping(src as *const u8, dst as *mut u8, n as usize) };
}

/// memset: preenche n bytes com value (low 8 bits).
#[rtse::function(module = "ptr", value = "write_bytes")]
pub fn write_bytes(dst: I64, value: I64, n: I64) {
    if dst == 0 || n <= 0 {
        return;
    }
    unsafe { std::ptr::write_bytes(dst as *mut u8, value as u8, n as usize) };
}

/// Adiciona n bytes ao ptr.
#[rtse::function(module = "ptr", value = "offset")]
pub fn offset(p: I64, n: I64) -> I64 {
    p.wrapping_add(n)
}

/// Registra a namespace `ptr` no motor.
pub fn register(e: &mut Engine) {
    e.module("ptr", |m| {
        m.doc("Operacoes raw sobre ponteiros (std::ptr). UNSAFE — caller verifica validez.");
        m.registry(null_entry());
        m.registry(is_null_entry());
        m.registry(read_i64_entry());
        m.registry(read_i32_entry());
        m.registry(read_u8_entry());
        m.registry(read_f64_entry());
        m.registry(write_i64_entry());
        m.registry(write_i32_entry());
        m.registry(write_u8_entry());
        m.registry(write_f64_entry());
        m.registry(copy_entry());
        m.registry(copy_nonoverlapping_entry());
        m.registry(write_bytes_entry());
        m.registry(offset_entry());
    });
}
