//! `FinalizationRegistry` global class (#685 v0) — stub sem GC weak real.
//!
//! v0: register/unregister sao noops. Callback nunca dispara — RTS nao tem
//! weak ref real ainda. Migrado do `#[rts_class]` (macro) pro modelo builder
//! hand-written do `rts-engine` (rumo à remoção da `rts-macro`). Símbolos
//! abreviam a classe como `FINREG`. Os externs `__RTS_FN_GL_FINREG_*` +
//! `register_finalization_registry_class_spec()` são escritos à mão.

use rts_engine::abi::ty::{Bool, Handle, I64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry, with_entry_mut};

/// Creates a new FinalizationRegistry with cleanup callback.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FINREG_NEW(callback: Handle) -> Handle {
    alloc_entry(Entry::FinalizationRegistry {
        callback,
        entries: Vec::new(),
    })
}

/// Registers target for cleanup with held value. v0: noop (sem weak ref real).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FINREG_REGISTER(reg: Handle, target: Handle, held: I64) {
    with_entry_mut(reg, |e| {
        if let Some(Entry::FinalizationRegistry { entries, .. }) = e {
            entries.push((target, held));
        }
    });
}

/// Unregisters target. v0: sempre retorna false (nada registrado).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FINREG_UNREGISTER(reg: Handle, token: Handle) -> Bool {
    with_entry_mut(reg, |e| {
        if let Some(Entry::FinalizationRegistry { entries, .. }) = e {
            let before = entries.len();
            entries.retain(|(t, _)| *t != token);
            return i64::from(entries.len() != before);
        }
        0
    })
}

// callback field intentionally unused for now — silence dead_code.
#[allow(dead_code)]
fn _touch_callback(reg: u64) -> u64 {
    with_entry(reg, |e| match e {
        Some(Entry::FinalizationRegistry { callback, .. }) => *callback,
        _ => 0,
    })
}

/// Membro de classe global (helper hand-written, espelha `leak_class` da macro).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        emit: None,
    }
}

/// Registra a classe global `FinalizationRegistry` no motor (hand-written, sem macro).
pub fn register_finalization_registry_class_spec(e: &mut Engine) {
    e.class("FinalizationRegistry")
        .doc("Built-in FinalizationRegistry (#685 v0). Stub — register/unregister sao noops; callback nunca dispara.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FINREG_NEW",
            "new FinalizationRegistry(callback: (heldValue: any) => void): FinalizationRegistry",
            "Creates a new FinalizationRegistry with cleanup callback.",
            __RTS_FN_GL_FINREG_NEW as *const u8,
            false,
        ))
        .member(m(
            "register",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle, AbiType::I64], AbiType::Void),
            "__RTS_FN_GL_FINREG_REGISTER",
            "register(target: object, heldValue: any, unregisterToken?: object): void",
            "Registers target for cleanup with held value. v0: noop (sem weak ref real).",
            __RTS_FN_GL_FINREG_REGISTER as *const u8,
            false,
        ))
        .member(m(
            "unregister",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
            "__RTS_FN_GL_FINREG_UNREGISTER",
            "unregister(unregisterToken: object): boolean",
            "Unregisters target. v0: sempre retorna false (nada registrado).",
            __RTS_FN_GL_FINREG_UNREGISTER as *const u8,
            false,
        ))
        .done();
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
