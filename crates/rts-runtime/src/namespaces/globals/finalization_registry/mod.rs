//! `FinalizationRegistry` global class (#685 v0) — stub sem GC weak real.
//!
//! v0: register/unregister sao noops. Callback nunca dispara — RTS nao tem
//! weak ref real ainda. Migrado ao modelo `#[rts_class]` (stage 5). Símbolos
//! abreviam a classe como `FINREG` (via `symbol=` override).

use rts_engine::abi::ty::{Bool, Handle, I64};
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

/// Built-in FinalizationRegistry (#685 v0). Stub — register/unregister sao noops; callback nunca dispara.
#[rts_class(
    FinalizationRegistry,
    prefix = "FINREG",
    spec = "FINALIZATION_REGISTRY_CLASS_SPEC"
)]
impl FinalizationRegistryClass {
    /// Creates a new FinalizationRegistry with cleanup callback.
    #[rts_ctor(
        ts = "new FinalizationRegistry(callback: (heldValue: any) => void): FinalizationRegistry"
    )]
    pub fn new(callback: Handle) -> Handle {
        alloc_entry(Entry::FinalizationRegistry {
            callback,
            entries: Vec::new(),
        })
    }

    /// Registers target for cleanup with held value. v0: noop (sem weak ref real).
    #[rts_method(ts = "register(target: object, heldValue: any, unregisterToken?: object): void")]
    pub fn register(reg: Handle, target: Handle, held: I64) {
        with_entry_mut(reg, |e| {
            if let Some(Entry::FinalizationRegistry { entries, .. }) = e {
                entries.push((target, held));
            }
        });
    }

    /// Unregisters target. v0: sempre retorna false (nada registrado).
    #[rts_method(ts = "unregister(unregisterToken: object): boolean")]
    pub fn unregister(reg: Handle, token: Handle) -> Bool {
        with_entry_mut(reg, |e| {
            if let Some(Entry::FinalizationRegistry { entries, .. }) = e {
                let before = entries.len();
                entries.retain(|(t, _)| *t != token);
                return i64::from(entries.len() != before);
            }
            0
        })
    }
}

// callback field intentionally unused for now — silence dead_code.
#[allow(dead_code)]
fn _touch_callback(reg: u64) -> u64 {
    with_entry(reg, |e| match e {
        Some(Entry::FinalizationRegistry { callback, .. }) => *callback,
        _ => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_unregister_roundtrip() {
        let cb = alloc_entry(Entry::String(b"cb".to_vec()));
        let reg = __RTS_FN_GL_FINREG_NEW(cb);
        let target = 0xdeadbeef;
        __RTS_FN_GL_FINREG_REGISTER(reg, target, 7);
        assert_eq!(__RTS_FN_GL_FINREG_UNREGISTER(reg, target), 1);
        assert_eq!(__RTS_FN_GL_FINREG_UNREGISTER(reg, target), 0);
    }
}
