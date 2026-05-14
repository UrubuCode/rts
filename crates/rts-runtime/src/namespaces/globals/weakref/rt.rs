//! `WeakRef` runtime — extern "C" implementations.
//!
//! v0: armazena strong reference. `deref()` retorna o target enquanto
//! o handle for valido. Coleta automatica fica para PR futura.

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKREF_NEW(target: u64) -> u64 {
    alloc_entry(Entry::WeakRef(target))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKREF_DEREF(wr: u64) -> u64 {
    with_entry(wr, |e| match e {
        Some(Entry::WeakRef(target)) => *target,
        _ => 0,
    })
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
