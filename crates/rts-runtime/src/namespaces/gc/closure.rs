//! Closure handles — fase 1 do refator de #195 (env-record real).
//!
//! Constroi um par (fn_ptr, env_handle) num unico handle GC. O codegen vivo
//! ainda usa promote-to-global; estes simbolos existem para os passos 2-3 do
//! plano da issue (lifted fn ABI ganha `env: u64` como primeiro arg).

use super::handles::{alloc_entry, with_entry, Entry};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_CLOSURE_ALLOC(fn_ptr: i64, env: u64) -> u64 {
    alloc_entry(Entry::Closure { fn_ptr, env })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_CLOSURE_FN_PTR(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Closure { fn_ptr, .. }) => *fn_ptr,
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_CLOSURE_ENV(handle: u64) -> u64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Closure { env, .. }) => *env,
        _ => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::super::env::{__RTS_FN_NS_GC_ENV_ALLOC, __RTS_FN_NS_GC_ENV_GET, __RTS_FN_NS_GC_ENV_SET};
    use super::*;

    #[test]
    fn alloc_roundtrip() {
        let env = __RTS_FN_NS_GC_ENV_ALLOC(2);
        __RTS_FN_NS_GC_ENV_SET(env, 0, 42);
        let c = __RTS_FN_NS_GC_CLOSURE_ALLOC(0xdead_beef, env);
        assert_ne!(c, 0);
        assert_eq!(__RTS_FN_NS_GC_CLOSURE_FN_PTR(c), 0xdead_beef);
        assert_eq!(__RTS_FN_NS_GC_CLOSURE_ENV(c), env);
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(__RTS_FN_NS_GC_CLOSURE_ENV(c), 0), 42);
    }

    #[test]
    fn invalid_handle_returns_zero() {
        assert_eq!(__RTS_FN_NS_GC_CLOSURE_FN_PTR(0), 0);
        assert_eq!(__RTS_FN_NS_GC_CLOSURE_ENV(999_999), 0);
    }
}
