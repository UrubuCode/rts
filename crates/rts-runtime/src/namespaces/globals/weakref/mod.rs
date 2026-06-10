//! `WeakRef` global class (#685 v0) — semantica strong temporaria.
//!
//! v0: armazena referencia forte ao target. `deref()` retorna o target
//! enquanto vivo. Coleta automatica fica para PR futura. Migrado do
//! `#[rts_class]` (macro) pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`). Os externs `__RTS_FN_GL_WEAKREF_*` +
//! `register_weakref_class_spec()` são escritos à mão.

use rts_engine::abi::ty::Handle;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use crate::namespaces::gc::handles::{alloc_entry, with_entry, Entry};

/// Creates a new WeakRef wrapping target.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKREF_NEW(target: Handle) -> Handle {
    alloc_entry(Entry::WeakRef(target))
}

/// Returns the target object (or undefined if collected). v0 sempre retorna target.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKREF_DEREF(wr: Handle) -> Handle {
    with_entry(wr, |e| match e {
        Some(Entry::WeakRef(target)) => *target,
        _ => 0,
    })
}

/// Registra a classe global `WeakRef` no motor (hand-written, sem macro).
pub fn register_weakref_class_spec(e: &mut Engine) {
    e.class("WeakRef")
        .doc("Built-in WeakRef (#685 v0). Strong reference; weak semantics em PR futura.")
        .member(Member {
            name: "new".to_string(),
            kind: MemberKind::Constructor,
            sig: Sig::new(vec![AbiType::Handle], AbiType::Handle),
            symbol: "__RTS_FN_GL_WEAKREF_NEW".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_WEAKREF_NEW as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "new WeakRef(target: object): WeakRef".to_string(),
            doc: "Creates a new WeakRef wrapping target.".to_string(),
            pure: false,
            intrinsic: None,
        })
        .member(Member {
            name: "deref".to_string(),
            kind: MemberKind::InstanceMethod,
            sig: Sig::new(vec![AbiType::Handle], AbiType::Handle),
            symbol: "__RTS_FN_GL_WEAKREF_DEREF".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_WEAKREF_DEREF as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "deref(): object | undefined".to_string(),
            doc: "Returns the target object (or undefined if collected). v0 sempre retorna target.".to_string(),
            pure: true,
            intrinsic: None,
        })
        .done();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deref_returns_target() {
        let target = alloc_entry(Entry::String(b"hello".to_vec()));
        let wr = __RTS_FN_GL_WEAKREF_NEW(target);
        assert_eq!(__RTS_FN_GL_WEAKREF_DEREF(wr), target);
    }
}
