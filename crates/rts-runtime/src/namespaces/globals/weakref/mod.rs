//! `WeakRef` global class (#685 v0) — semantica strong temporaria.
//!
//! v0: armazena referencia forte ao target. `deref()` retorna o target
//! enquanto vivo. Coleta automatica fica para PR futura. Migrado ao modelo
//! `#[rts_class]` (stage 5).

use rts_engine::abi::ty::Handle;
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, Entry};

/// Built-in WeakRef (#685 v0). Strong reference; weak semantics em PR futura.
#[rts_class(WeakRef)]
impl WeakRefClass {
    /// Creates a new WeakRef wrapping target.
    #[rts_ctor(ts = "new WeakRef(target: object): WeakRef")]
    pub fn new(target: Handle) -> Handle {
        alloc_entry(Entry::WeakRef(target))
    }

    /// Returns the target object (or undefined if collected). v0 sempre retorna target.
    #[rts_method(ts = "deref(): object | undefined", pure)]
    pub fn deref(wr: Handle) -> Handle {
        with_entry(wr, |e| match e {
            Some(Entry::WeakRef(target)) => *target,
            _ => 0,
        })
    }
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
