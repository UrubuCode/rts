//! `WeakSet` runtime — extern "C" implementations.

use std::collections::HashSet;

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry, with_entry_mut};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKSET_NEW() -> u64 {
    alloc_entry(Entry::WeakSet(Box::new(HashSet::new())))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKSET_ADD(ws: u64, val: u64) -> u64 {
    with_entry_mut(ws, |e| {
        if let Some(Entry::WeakSet(set)) = e {
            set.insert(val);
        }
    });
    ws
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKSET_HAS(ws: u64, val: u64) -> i64 {
    with_entry(ws, |e| match e {
        Some(Entry::WeakSet(set)) => i64::from(set.contains(&val)),
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKSET_DELETE(ws: u64, val: u64) -> i64 {
    with_entry_mut(ws, |e| match e {
        Some(Entry::WeakSet(set)) => i64::from(set.remove(&val)),
        _ => 0,
    })
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
