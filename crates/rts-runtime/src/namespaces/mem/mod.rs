//! `mem` namespace — std::mem: layout (size_of/align_of), swap, drop, forget.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`). The `size_of_*` / `align_of_*` members are
//! `#[rts_const]` — accessed without parens (`mem.size_of_i64`).

use rts_engine::abi::ty::{Handle, I64};
use rts_macro::rts_namespace;

/// std::mem: layout (size_of/align_of), swap, drop, forget.
#[rts_namespace(mem)]
impl MemNs {
    /// Tamanho em bytes de um i64 (= 8).
    #[rts_const(pure)]
    pub fn size_of_i64() -> I64 {
        std::mem::size_of::<i64>() as i64
    }

    /// Tamanho em bytes de um f64 (= 8).
    #[rts_const(pure)]
    pub fn size_of_f64() -> I64 {
        std::mem::size_of::<f64>() as i64
    }

    /// Tamanho em bytes de um i32 (= 4).
    #[rts_const(pure)]
    pub fn size_of_i32() -> I64 {
        std::mem::size_of::<i32>() as i64
    }

    /// Tamanho em bytes de um bool (= 1).
    #[rts_const(pure)]
    pub fn size_of_bool() -> I64 {
        std::mem::size_of::<bool>() as i64
    }

    /// Alinhamento de i64 (= 8).
    #[rts_const(pure)]
    pub fn align_of_i64() -> I64 {
        std::mem::align_of::<i64>() as i64
    }

    /// Alinhamento de f64 (= 8).
    #[rts_const(pure)]
    pub fn align_of_f64() -> I64 {
        std::mem::align_of::<f64>() as i64
    }

    /// Retorna `b` (use idiom: `let old = mem.swap_i64(a, b)`).
    #[rts_fn(pure)]
    pub fn swap_i64(_a: I64, b: I64) -> I64 {
        // RTS sem refs nao oferece swap-by-pointer real; retorna `b` para
        // enfatizar a operacao (caller faz a atribuicao).
        b
    }

    /// Forca free de um handle GC. Equivalente a gc.string_free / etc.
    #[rts_fn]
    pub fn drop_handle(h: Handle) {
        let _ = crate::namespaces::gc::handles::free_handle(h);
    }

    /// Esquece handle sem rodar drop — vaza memoria intencionalmente.
    #[rts_fn]
    pub fn forget_handle(_h: Handle) {
        // Intencional: nao chama free.
    }

    /// Idiom: `mem.replace_i64(slot, new)` — retorna slot e usa caller pra escrever.
    #[rts_fn]
    pub fn replace_i64(slot: I64, _new_val: I64) -> I64 {
        slot
    }
}
