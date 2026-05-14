//! `FinalizationRegistry` runtime — extern "C" implementations.
//!
//! v0: armazena callback + entries mas nunca dispara cleanup. Callback
//! real exigiria GC weak ref (#393).

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry, with_entry_mut};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FINREG_NEW(callback: u64) -> u64 {
    alloc_entry(Entry::FinalizationRegistry {
        callback,
        entries: Vec::new(),
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FINREG_REGISTER(reg: u64, target: u64, held: i64) {
    with_entry_mut(reg, |e| {
        if let Some(Entry::FinalizationRegistry { entries, .. }) = e {
            entries.push((target, held));
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FINREG_UNREGISTER(reg: u64, token: u64) -> i64 {
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
