//! `hint` namespace — performance hints (std::hint).
//!
//! First namespace migrated to the `#[rts_namespace]` single-declaration model
//! (stage 2, `docs/specs/rts-core-engine.md`). The extern symbols, the
//! `NamespaceMember` table and the `SPEC` const are all derived from this one
//! `impl` block — no separate `abi.rs` / `ops.rs`.

use rts_abi::ty::{Bool, F64, I64};
use rts_macro::rts_namespace;

/// Performance hints (std::hint): spin_loop, black_box, unreachable, assert_unchecked.
#[rts_namespace(hint)]
impl Hint {
    /// Hint para spin-wait loop (PAUSE em x86, YIELD em ARM).
    #[rts_fn]
    pub fn spin_loop() {
        std::hint::spin_loop();
    }

    /// Opaque pra otimizador — impede que o valor seja eliminado.
    #[rts_fn]
    pub fn black_box_i64(value: I64) -> I64 {
        std::hint::black_box(value)
    }

    /// Opaque pra otimizador (variante f64).
    #[rts_fn]
    pub fn black_box_f64(value: F64) -> F64 {
        std::hint::black_box(value)
    }

    /// Marca codigo inalcancavel — em debug aborta, em release eh UB.
    #[rts_fn(ts = "unreachable(): never")]
    pub fn unreachable() {
        // Em debug, aborta com mensagem clara. Em release, eh UB —
        // mas exposto como function call portavel via panic.
        if cfg!(debug_assertions) {
            panic!("hint.unreachable() atingido");
        }
        // SAFETY: caller garantiu que esta linha nao executa.
        unsafe { std::hint::unreachable_unchecked() }
    }

    /// Assume cond=true sem verificar. Cond falsa = UB em release.
    #[rts_fn]
    pub fn assert_unchecked(cond: Bool) {
        if cfg!(debug_assertions) {
            if cond == 0 {
                panic!("hint.assert_unchecked: cond=false");
            }
            return;
        }
        // SAFETY: caller garantiu cond verdadeira.
        unsafe { std::hint::assert_unchecked(cond != 0) }
    }
}
