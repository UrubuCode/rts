//! `WeakMap` runtime — extern "C" implementations.
//!
//! v0: comporta como Map forte (sem weak references). Coleta automatica
//! quando key e' freed fica para PR futura junto com FinalizationRegistry.

use std::collections::HashMap;

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry, with_entry_mut};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKMAP_NEW() -> u64 {
    alloc_entry(Entry::WeakMap(Box::new(HashMap::new())))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKMAP_SET(wm: u64, key: u64, value: i64) -> u64 {
    with_entry_mut(wm, |e| {
        if let Some(Entry::WeakMap(map)) = e {
            map.insert(key, value);
        }
    });
    wm
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKMAP_GET(wm: u64, key: u64) -> i64 {
    with_entry(wm, |e| match e {
        Some(Entry::WeakMap(map)) => map.get(&key).copied().unwrap_or(0),
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKMAP_HAS(wm: u64, key: u64) -> i64 {
    with_entry(wm, |e| match e {
        Some(Entry::WeakMap(map)) => i64::from(map.contains_key(&key)),
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKMAP_DELETE(wm: u64, key: u64) -> i64 {
    with_entry_mut(wm, |e| match e {
        Some(Entry::WeakMap(map)) => i64::from(map.remove(&key).is_some()),
        _ => 0,
    })
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
