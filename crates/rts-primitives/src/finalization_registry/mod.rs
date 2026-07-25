//! `FinalizationRegistry` global class (#685 v0) — stub sem GC weak real.
//!
//! v0: register/unregister são noops de verdade — callback nunca dispara (RTS
//! não tem weak ref real ainda, ver #217). Migrado do modelo builder
//! hand-written pro `#[rtse::class]` (macro G1+F1): storage vira o
//! `Entry::Rtse(Box<dyn Any>)` genérico em vez do `Entry::FinalizationRegistry`
//! dedicado. `callback`/`entries` continuam campos Rust normais (NÃO
//! `#[rtse::variable]` — não são propriedades JS visíveis).

use rts_engine::abi::ty::Handle;

#[rtse::class("FinalizationRegistry")]
#[derive(Clone)]
pub struct FinalizationRegistry {
    // v0 stub: callback nunca é invocado (sem GC weak real ainda, #217) — só
    // guardado pra quando a fase weak ligar o disparo de verdade.
    #[allow(dead_code)]
    callback: u64,
    entries: Vec<(u64, i64)>,
}

#[rtse::class("FinalizationRegistry")]
impl FinalizationRegistry {
    /// Creates a new FinalizationRegistry with cleanup callback.
    #[rtse::ctor]
    fn new(callback: Handle) -> Self {
        FinalizationRegistry {
            callback,
            entries: Vec::new(),
        }
    }

    /// Registers target for cleanup with held value. v0: noop (sem weak ref real).
    #[rtse::method]
    fn register(self: &mut FinalizationRegistry, target: Handle, held: i64) {
        self.entries.push((target, held));
    }

    /// Unregisters target. v0: sempre retorna false (nada registrado de verdade
    /// dispara, só o bookkeeping do stub).
    #[rtse::method]
    fn unregister(self: &mut FinalizationRegistry, token: Handle) -> bool {
        let before = self.entries.len();
        self.entries.retain(|(t, _)| *t != token);
        self.entries.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_engine::heap::handles::{Entry, alloc_entry};

    #[test]
    fn register_unregister_roundtrip() {
        let cb = alloc_entry(Entry::String(b"cb".to_vec()));
        let reg = __RTS_FN_GL_FINALIZATIONREGISTRY_new(cb);
        let target = 0xdeadbeef;
        __RTS_FN_GL_FINALIZATIONREGISTRY_register(reg, target, 7);
        assert_eq!(__RTS_FN_GL_FINALIZATIONREGISTRY_unregister(reg, target), 1);
        assert_eq!(__RTS_FN_GL_FINALIZATIONREGISTRY_unregister(reg, target), 0);
    }
}
