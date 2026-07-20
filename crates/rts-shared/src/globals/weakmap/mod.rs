//! `WeakMap` global class (#217 v0) — semantica forte temporaria.
//!
//! v0 nao implementa coleta automatica de entries quando key e' freed
//! (faltando FinalizationRegistry / weak refs reais). Comporta como Map
//! forte com chaves u64 (handles).
//!
//! Migrado do `#[rts_class]` (macro) pro modelo builder hand-written do
//! `rts-engine` (rumo à remoção da `rts-macro`). Os externs
//! `__RTS_FN_GL_WEAKMAP_*` + `register_weakmap_class_spec()` são escritos à mão.

use std::collections::HashMap;

use rts_engine::abi::ty::{Bool, Handle, I64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry, with_entry_mut};

/// Creates a new empty WeakMap.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKMAP_NEW() -> Handle {
    alloc_entry(Entry::WeakMap(Box::new(HashMap::new())))
}

/// Sets value for key (handle). Returns the WeakMap.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKMAP_SET(wm: Handle, key: Handle, value: I64) -> Handle {
    with_entry_mut(wm, |e| {
        if let Some(Entry::WeakMap(map)) = e {
            map.insert(key, value);
        }
    });
    wm
}

/// Returns value for key (handle/string), or 0 if absent.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKMAP_GET(wm: Handle, key: Handle) -> Handle {
    with_entry(wm, |e| match e {
        Some(Entry::WeakMap(map)) => map.get(&key).copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// Returns true if key exists, false otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKMAP_HAS(wm: Handle, key: Handle) -> Bool {
    with_entry(wm, |e| match e {
        Some(Entry::WeakMap(map)) => i64::from(map.contains_key(&key)),
        _ => 0,
    })
}

/// Removes key. Returns true if existed, false otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKMAP_DELETE(wm: Handle, key: Handle) -> Bool {
    with_entry_mut(wm, |e| match e {
        Some(Entry::WeakMap(map)) => i64::from(map.remove(&key).is_some()),
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
        emit: None,
    }
}

/// Registra a classe global `WeakMap` no motor (Fase 2 — hand-written, sem macro).
pub fn register_weakmap_class_spec(e: &mut Engine) {
    e.class("WeakMap")
        .doc("Built-in WeakMap (#217 v0). Comporta como Map forte; weak semantics em PR futura.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_WEAKMAP_NEW",
            "new WeakMap(): WeakMap",
            "Creates a new empty WeakMap.",
            __RTS_FN_GL_WEAKMAP_NEW as *const u8,
            false,
        ))
        .member(m(
            "set",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::Handle, AbiType::I64],
                AbiType::Handle,
            ),
            "__RTS_FN_GL_WEAKMAP_SET",
            "set(key: object, value: any): WeakMap",
            "Sets value for key (handle). Returns the WeakMap.",
            __RTS_FN_GL_WEAKMAP_SET as *const u8,
            false,
        ))
        .member(m(
            "get",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_WEAKMAP_GET",
            "get(key: object): any",
            "Returns value for key (handle/string), or 0 if absent.",
            __RTS_FN_GL_WEAKMAP_GET as *const u8,
            true,
        ))
        .member(m(
            "has",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
            "__RTS_FN_GL_WEAKMAP_HAS",
            "has(key: object): boolean",
            "Returns true if key exists, false otherwise.",
            __RTS_FN_GL_WEAKMAP_HAS as *const u8,
            true,
        ))
        .member(m(
            "delete",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
            "__RTS_FN_GL_WEAKMAP_DELETE",
            "delete(key: object): boolean",
            "Removes key. Returns true if existed, false otherwise.",
            __RTS_FN_GL_WEAKMAP_DELETE as *const u8,
            false,
        ))
        .instanceof_predicate("__RTS_FN_NS_GC_IS_MAP_LIKE")
        .done();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_roundtrip() {
        let wm = __RTS_FN_GL_WEAKMAP_NEW();
        let key = 0xdead_beef;
        assert_eq!(__RTS_FN_GL_WEAKMAP_SET(wm, key, 42), wm);
        assert_eq!(__RTS_FN_GL_WEAKMAP_GET(wm, key), 42);
    }

    #[test]
    fn has_and_missing_returns_zero() {
        let wm = __RTS_FN_GL_WEAKMAP_NEW();
        let key = 1234;
        assert_eq!(__RTS_FN_GL_WEAKMAP_HAS(wm, key), 0);
        __RTS_FN_GL_WEAKMAP_SET(wm, key, 99);
        assert_eq!(__RTS_FN_GL_WEAKMAP_HAS(wm, key), 1);
        assert_eq!(__RTS_FN_GL_WEAKMAP_GET(wm, 9999), 0);
    }

    #[test]
    fn delete_existing_and_missing() {
        let wm = __RTS_FN_GL_WEAKMAP_NEW();
        let key = 7;
        __RTS_FN_GL_WEAKMAP_SET(wm, key, 100);
        assert_eq!(__RTS_FN_GL_WEAKMAP_DELETE(wm, key), 1);
        assert_eq!(__RTS_FN_GL_WEAKMAP_DELETE(wm, key), 0);
        assert_eq!(__RTS_FN_GL_WEAKMAP_HAS(wm, key), 0);
    }

    #[test]
    fn overwrite_value() {
        let wm = __RTS_FN_GL_WEAKMAP_NEW();
        let key = 5;
        __RTS_FN_GL_WEAKMAP_SET(wm, key, 1);
        __RTS_FN_GL_WEAKMAP_SET(wm, key, 2);
        assert_eq!(__RTS_FN_GL_WEAKMAP_GET(wm, key), 2);
    }
}
