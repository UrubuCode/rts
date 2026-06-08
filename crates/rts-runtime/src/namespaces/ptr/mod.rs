//! `ptr` namespace — raw pointer operations (std::ptr).
//!
//! Toda funcao eh `unsafe` por natureza — caller eh responsavel por
//! validez/alinhamento/lifetime. Use com `buffer.ptr(handle)` para ler/escrever
//! buffers. Enderecos viajam como i64.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use rts_abi::ty::{Bool, F64, I64};
use rts_macro::rts_namespace;

/// Operacoes raw sobre ponteiros (std::ptr). UNSAFE — caller verifica validez.
#[rts_namespace(ptr)]
impl PtrNs {
    /// Retorna ponteiro nulo (0).
    #[rts_fn]
    pub fn null() -> I64 {
        0
    }

    /// True se ptr == 0.
    #[rts_fn]
    pub fn is_null(p: I64) -> Bool {
        if p == 0 { 1 } else { 0 }
    }

    /// Le i64 do endereco. UNSAFE: caller garante validade/alinhamento.
    #[rts_fn]
    pub fn read_i64(p: I64) -> I64 {
        if p == 0 {
            return 0;
        }
        unsafe { std::ptr::read_unaligned(p as *const i64) }
    }

    /// Le i32 do endereco e estende para i64.
    #[rts_fn]
    pub fn read_i32(p: I64) -> I64 {
        if p == 0 {
            return 0;
        }
        let v = unsafe { std::ptr::read_unaligned(p as *const i32) };
        v as i64
    }

    /// Le u8 do endereco e estende para i64 (0..255).
    #[rts_fn]
    pub fn read_u8(p: I64) -> I64 {
        if p == 0 {
            return 0;
        }
        let v = unsafe { std::ptr::read_unaligned(p as *const u8) };
        v as i64
    }

    /// Le f64 do endereco.
    #[rts_fn]
    pub fn read_f64(p: I64) -> F64 {
        if p == 0 {
            return 0.0;
        }
        unsafe { std::ptr::read_unaligned(p as *const f64) }
    }

    /// Escreve i64 no endereco.
    #[rts_fn]
    pub fn write_i64(p: I64, value: I64) {
        if p == 0 {
            return;
        }
        unsafe { std::ptr::write_unaligned(p as *mut i64, value) };
    }

    /// Escreve i32 (low 32 bits) no endereco.
    #[rts_fn]
    pub fn write_i32(p: I64, value: I64) {
        if p == 0 {
            return;
        }
        unsafe { std::ptr::write_unaligned(p as *mut i32, value as i32) };
    }

    /// Escreve u8 (low 8 bits) no endereco.
    #[rts_fn]
    pub fn write_u8(p: I64, value: I64) {
        if p == 0 {
            return;
        }
        unsafe { std::ptr::write_unaligned(p as *mut u8, value as u8) };
    }

    /// Escreve f64 no endereco.
    #[rts_fn]
    pub fn write_f64(p: I64, value: F64) {
        if p == 0 {
            return;
        }
        unsafe { std::ptr::write_unaligned(p as *mut f64, value) };
    }

    /// memmove: copia n bytes de src para dst (overlapping ok).
    #[rts_fn]
    pub fn copy(dst: I64, src: I64, n: I64) {
        if dst == 0 || src == 0 || n <= 0 {
            return;
        }
        unsafe { std::ptr::copy(src as *const u8, dst as *mut u8, n as usize) };
    }

    /// memcpy: copia n bytes (regioes nao podem se sobrepor).
    #[rts_fn]
    pub fn copy_nonoverlapping(dst: I64, src: I64, n: I64) {
        if dst == 0 || src == 0 || n <= 0 {
            return;
        }
        unsafe { std::ptr::copy_nonoverlapping(src as *const u8, dst as *mut u8, n as usize) };
    }

    /// memset: preenche n bytes com value (low 8 bits).
    #[rts_fn]
    pub fn write_bytes(dst: I64, value: I64, n: I64) {
        if dst == 0 || n <= 0 {
            return;
        }
        unsafe { std::ptr::write_bytes(dst as *mut u8, value as u8, n as usize) };
    }

    /// Adiciona n bytes ao ptr.
    #[rts_fn]
    pub fn offset(p: I64, n: I64) -> I64 {
        p.wrapping_add(n)
    }
}
