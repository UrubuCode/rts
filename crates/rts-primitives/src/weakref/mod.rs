//! `WeakRef` global class (#685 / #217 A1.1) — semântica weak REAL.
//!
//! Migrado do modelo builder hand-written pro `#[rtse::class]` (macro G1+F1):
//! agora é um struct Rust normal (`Entry::Rtse(Box<dyn Any>)` genérico) em vez
//! do `Entry::WeakRef(u64)` dedicado. A semântica weak continua válida porque
//! `Traceable::trace_children` para `Entry` cai no braço `_ => {}` tanto pra
//! `Entry::Rtse` quanto pro antigo `Entry::WeakRef` — o coletor NÃO marca
//! através de nenhum dos dois, então o `target` morre se não houver outra
//! referência forte (ver `crates/rts-engine/src/heap/handles.rs`
//! `impl Traceable for Entry`). `deref()` só devolve o target se o slot ainda
//! estiver vivo com a MESMA generation (`with_entry` valida gen+slot O(1));
//! caso contrário devolve `undefined` (0) — sem use-after-free.

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::with_entry;

/// Wrapper weak sobre um handle de objeto-target. Campo NÃO anotado
/// `#[rtse::variable]` de propósito — `target` não é uma propriedade JS
/// visível (não queremos getter/setter automático), só estado interno lido
/// pelos métodos.
#[rtse::class("WeakRef")]
#[derive(Clone)]
pub struct WeakRef {
    target: u64,
}

#[rtse::class("WeakRef")]
impl WeakRef {
    /// Creates a new WeakRef wrapping target.
    #[rtse::ctor]
    fn new(target: Handle) -> Self {
        WeakRef { target }
    }

    /// Returns the target object, or `undefined` (0) if it was collected/reused.
    ///
    /// Weak de verdade: o target não é mantido vivo pelo WeakRef (não é
    /// traçado). Só o devolvemos se o handle ainda apontar pra um slot vivo com
    /// a mesma generation — `with_entry` rejeita (None) handle stale (slot Free
    /// ou gen bumpou). O(1), sem scan.
    #[rtse::method]
    fn deref(self: &WeakRef) -> Handle {
        if self.target == 0 {
            return 0;
        }
        let alive = with_entry(self.target, |e| e.is_some());
        if alive { self.target } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_engine::heap::handles::{Entry, alloc_entry};

    #[test]
    fn deref_returns_target() {
        let target = alloc_entry(Entry::String(b"hello".to_vec()));
        let wr = __rtsm_global_weakref_new(target);
        assert_eq!(__rtsm_global_weakref_deref(wr), target);
    }

    #[test]
    fn deref_undefined_for_stale_target() {
        // Um handle com generation 0 (16 bits altos zerados) nunca corresponde a
        // um slot vivo — slots vivos têm generation >= 1. WeakRef sobre ele
        // simula um target já coletado/reusado: deref deve devolver undefined (0),
        // não o handle stale (o use-after-free que o v0 tinha).
        let stale: Handle = 5; // gen=0, slot=5
        let wr = __rtsm_global_weakref_new(stale);
        assert_eq!(__rtsm_global_weakref_deref(wr), 0);
    }
}
