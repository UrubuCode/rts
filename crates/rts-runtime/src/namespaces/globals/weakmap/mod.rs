//! `WeakMap` global class (#217 v0) — semantica forte temporaria.
//!
//! v0 nao implementa coleta automatica de entries quando key e' freed
//! (faltando FinalizationRegistry / weak refs reais). Comporta como Map
//! forte com chaves u64 (handles). Migrado ao modelo `#[rts_class]` (stage 5).

use std::collections::HashMap;

use rts_engine::abi::ty::{Bool, Handle, I64};
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

/// Built-in WeakMap (#217 v0). Comporta como Map forte; weak semantics em PR futura.
#[rts_class(WeakMap)]
impl WeakMapClass {
    /// Creates a new empty WeakMap.
    #[rts_ctor(ts = "new WeakMap(): WeakMap")]
    pub fn new() -> Handle {
        alloc_entry(Entry::WeakMap(Box::new(HashMap::new())))
    }

    /// Sets value for key (handle). Returns the WeakMap.
    #[rts_method(ts = "set(key: object, value: any): WeakMap")]
    pub fn set(wm: Handle, key: Handle, value: I64) -> Handle {
        with_entry_mut(wm, |e| {
            if let Some(Entry::WeakMap(map)) = e {
                map.insert(key, value);
            }
        });
        wm
    }

    /// Returns value for key (handle/string), or 0 if absent.
    #[rts_method(ts = "get(key: object): any", pure)]
    pub fn get(wm: Handle, key: Handle) -> Handle {
        with_entry(wm, |e| match e {
            Some(Entry::WeakMap(map)) => map.get(&key).copied().unwrap_or(0) as u64,
            _ => 0,
        })
    }

    /// Returns true if key exists, false otherwise.
    #[rts_method(ts = "has(key: object): boolean", pure)]
    pub fn has(wm: Handle, key: Handle) -> Bool {
        with_entry(wm, |e| match e {
            Some(Entry::WeakMap(map)) => i64::from(map.contains_key(&key)),
            _ => 0,
        })
    }

    /// Removes key. Returns true if existed, false otherwise.
    #[rts_method(ts = "delete(key: object): boolean")]
    pub fn delete(wm: Handle, key: Handle) -> Bool {
        with_entry_mut(wm, |e| match e {
            Some(Entry::WeakMap(map)) => i64::from(map.remove(&key).is_some()),
            _ => 0,
        })
    }
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
