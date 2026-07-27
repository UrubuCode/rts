//! `mem` namespace — std::mem: layout (size_of/align_of), swap, drop, forget.
//!
//! Os membros ABI sao declarados com `#[rtse::function]` / `#[rtse::constant]`
//! (F7 de `docs/specs/rts-macro-single-source.md`): simbolo, assinatura,
//! `ts_signature` e fn-ptr saem derivados da declaracao Rust.
//!
//! Os `size_of_*` / `align_of_*` sao CONSTANTES (`mem.size_of_i64`, sem
//! parenteses) — por isso `#[rtse::constant]` sobre um `const` Rust de verdade,
//! e nao uma fn: e a macro que emite o getter zero-arg que
//! `MemberKind::Constant` exige.

use rts_engine::abi::ty::{Handle, I64};
use rts_engine::Engine;

/// Tamanho em bytes de um i64 (= 8).
#[rtse::constant(module = "mem", value = "size_of_i64")]
pub const SIZE_OF_I64: i64 = std::mem::size_of::<i64>() as i64;

/// Tamanho em bytes de um f64 (= 8).
#[rtse::constant(module = "mem", value = "size_of_f64")]
pub const SIZE_OF_F64: i64 = std::mem::size_of::<f64>() as i64;

/// Tamanho em bytes de um i32 (= 4).
#[rtse::constant(module = "mem", value = "size_of_i32")]
pub const SIZE_OF_I32: i64 = std::mem::size_of::<i32>() as i64;

/// Tamanho em bytes de um bool (= 1).
#[rtse::constant(module = "mem", value = "size_of_bool")]
pub const SIZE_OF_BOOL: i64 = std::mem::size_of::<bool>() as i64;

/// Alinhamento de i64 (= 8).
#[rtse::constant(module = "mem", value = "align_of_i64")]
pub const ALIGN_OF_I64: i64 = std::mem::align_of::<i64>() as i64;

/// Alinhamento de f64 (= 8).
#[rtse::constant(module = "mem", value = "align_of_f64")]
pub const ALIGN_OF_F64: i64 = std::mem::align_of::<f64>() as i64;

/// Retorna `b` (use idiom: `let old = mem.swap_i64(a, b)`).
#[rtse::function(module = "mem", value = "swap_i64")]
pub fn swap_i64(_a: I64, b: I64) -> I64 {
    // RTS sem refs nao oferece swap-by-pointer real; retorna `b` para enfatizar
    // a operacao (caller faz a atribuicao).
    b
}

/// Forca free de um handle GC. Equivalente a gc.string_free / etc.
#[rtse::function(module = "mem", value = "drop_handle")]
pub fn drop_handle(h: Handle) {
    let _ = rts_engine::heap::handles::free_handle(h);
}

/// Esquece handle sem rodar drop — vaza memoria intencionalmente.
#[rtse::function(module = "mem", value = "forget_handle")]
pub fn forget_handle(_h: Handle) {
    // Intencional: nao chama free.
}

/// Idiom: `mem.replace_i64(slot, new)` — retorna slot e usa caller pra escrever.
#[rtse::function(module = "mem", value = "replace_i64")]
pub fn replace_i64(slot: I64, _new_val: I64) -> I64 {
    slot
}

/// Registra a namespace `mem` no motor.
pub fn register(e: &mut Engine) {
    e.module("mem", |m| {
        m.doc("std::mem: layout (size_of/align_of), swap, drop, forget.");
        m.member(size_of_i64_member());
        m.member(size_of_f64_member());
        m.member(size_of_i32_member());
        m.member(size_of_bool_member());
        m.member(align_of_i64_member());
        m.member(align_of_f64_member());
        m.registry(swap_i64_entry());
        m.registry(drop_handle_entry());
        m.registry(forget_handle_entry());
        m.registry(replace_i64_entry());
    });
}
