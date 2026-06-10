//! `WeakSet` global class (#217 v0) — semantica forte temporaria.
//!
//! v0 nao implementa coleta automatica quando elementos sao freed. Migrado do
//! `#[rts_class]` (macro) pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`). Os externs `__RTS_FN_GL_WEAKSET_*` +
//! `register_weakset_class_spec()` são escritos à mão.

use std::collections::HashSet;

use rts_engine::abi::ty::{Bool, Handle};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

/// Creates a new empty WeakSet.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKSET_NEW() -> Handle {
    alloc_entry(Entry::WeakSet(Box::new(HashSet::new())))
}

/// Adds object handle to set. Returns the WeakSet.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKSET_ADD(ws: Handle, val: Handle) -> Handle {
    with_entry_mut(ws, |e| {
        if let Some(Entry::WeakSet(set)) = e {
            set.insert(val);
        }
    });
    ws
}

/// Returns true if value present, false otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKSET_HAS(ws: Handle, val: Handle) -> Bool {
    with_entry(ws, |e| match e {
        Some(Entry::WeakSet(set)) => i64::from(set.contains(&val)),
        _ => 0,
    })
}

/// Removes value. Returns true if existed, false otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKSET_DELETE(ws: Handle, val: Handle) -> Bool {
    with_entry_mut(ws, |e| match e {
        Some(Entry::WeakSet(set)) => i64::from(set.remove(&val)),
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
        intrinsic: None,
    }
}

/// Registra a classe global `WeakSet` no motor (Fase 2 — hand-written, sem macro).
pub fn register_weakset_class_spec(e: &mut Engine) {
    e.class("WeakSet")
        .doc("Built-in WeakSet (#217 v0). Comporta como Set forte; weak semantics em PR futura.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_WEAKSET_NEW",
            "new WeakSet(): WeakSet",
            "Creates a new empty WeakSet.",
            __RTS_FN_GL_WEAKSET_NEW as *const u8,
            false,
        ))
        .member(m(
            "add",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_WEAKSET_ADD",
            "add(value: object): WeakSet",
            "Adds object handle to set. Returns the WeakSet.",
            __RTS_FN_GL_WEAKSET_ADD as *const u8,
            false,
        ))
        .member(m(
            "has",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
            "__RTS_FN_GL_WEAKSET_HAS",
            "has(value: object): boolean",
            "Returns true if value present, false otherwise.",
            __RTS_FN_GL_WEAKSET_HAS as *const u8,
            true,
        ))
        .member(m(
            "delete",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
            "__RTS_FN_GL_WEAKSET_DELETE",
            "delete(value: object): boolean",
            "Removes value. Returns true if existed, false otherwise.",
            __RTS_FN_GL_WEAKSET_DELETE as *const u8,
            false,
        ))
        .done();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_has() {
        let ws = __RTS_FN_GL_WEAKSET_NEW();
        let v = 0xcafe;
        assert_eq!(__RTS_FN_GL_WEAKSET_HAS(ws, v), 0);
        assert_eq!(__RTS_FN_GL_WEAKSET_ADD(ws, v), ws);
        assert_eq!(__RTS_FN_GL_WEAKSET_HAS(ws, v), 1);
    }

    #[test]
    fn delete_existing_and_missing() {
        let ws = __RTS_FN_GL_WEAKSET_NEW();
        let v = 42;
        __RTS_FN_GL_WEAKSET_ADD(ws, v);
        assert_eq!(__RTS_FN_GL_WEAKSET_DELETE(ws, v), 1);
        assert_eq!(__RTS_FN_GL_WEAKSET_DELETE(ws, v), 0);
        assert_eq!(__RTS_FN_GL_WEAKSET_HAS(ws, v), 0);
    }

    #[test]
    fn add_idempotent() {
        let ws = __RTS_FN_GL_WEAKSET_NEW();
        __RTS_FN_GL_WEAKSET_ADD(ws, 1);
        __RTS_FN_GL_WEAKSET_ADD(ws, 1);
        assert_eq!(__RTS_FN_GL_WEAKSET_HAS(ws, 1), 1);
        assert_eq!(__RTS_FN_GL_WEAKSET_DELETE(ws, 1), 1);
        assert_eq!(__RTS_FN_GL_WEAKSET_HAS(ws, 1), 0);
    }
}
