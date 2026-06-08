//! `WeakSet` global class (#217 v0) — semantica forte temporaria.
//!
//! v0 nao implementa coleta automatica quando elementos sao freed. Migrado ao
//! modelo `#[rts_class]` (stage 5).

use std::collections::HashSet;

use rts_abi::ty::{Bool, Handle};
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

/// Built-in WeakSet (#217 v0). Comporta como Set forte; weak semantics em PR futura.
#[rts_class(WeakSet)]
impl WeakSetClass {
    /// Creates a new empty WeakSet.
    #[rts_ctor(ts = "new WeakSet(): WeakSet")]
    pub fn new() -> Handle {
        alloc_entry(Entry::WeakSet(Box::new(HashSet::new())))
    }

    /// Adds object handle to set. Returns the WeakSet.
    #[rts_method(ts = "add(value: object): WeakSet")]
    pub fn add(ws: Handle, val: Handle) -> Handle {
        with_entry_mut(ws, |e| {
            if let Some(Entry::WeakSet(set)) = e {
                set.insert(val);
            }
        });
        ws
    }

    /// Returns true if value present, false otherwise.
    #[rts_method(ts = "has(value: object): boolean", pure)]
    pub fn has(ws: Handle, val: Handle) -> Bool {
        with_entry(ws, |e| match e {
            Some(Entry::WeakSet(set)) => i64::from(set.contains(&val)),
            _ => 0,
        })
    }

    /// Removes value. Returns true if existed, false otherwise.
    #[rts_method(ts = "delete(value: object): boolean")]
    pub fn delete(ws: Handle, val: Handle) -> Bool {
        with_entry_mut(ws, |e| match e {
            Some(Entry::WeakSet(set)) => i64::from(set.remove(&val)),
            _ => 0,
        })
    }
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
