//! Environment records para closures — fase 1 da #195.

use super::handles::{alloc_entry, free_handle, with_entry, with_entry_mut, Entry};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ENV_ALLOC(slot_count: i32) -> u64 {
    if slot_count < 0 || slot_count > 65536 {
        return 0;
    }
    alloc_entry(Entry::Env(vec![0i64; slot_count as usize]))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ENV_GET(env: u64, slot: i32) -> i64 {
    if slot < 0 {
        return 0;
    }
    with_entry(env, |entry| match entry {
        Some(Entry::Env(slots)) => slots.get(slot as usize).copied().unwrap_or(0),
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ENV_SET(env: u64, slot: i32, value: i64) -> i64 {
    if slot < 0 {
        return 0;
    }
    with_entry_mut(env, |entry| match entry {
        Some(Entry::Env(slots)) => match slots.get_mut(slot as usize) {
            Some(cell) => { *cell = value; 1 }
            None => 0,
        },
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ENV_FREE(env: u64) -> i64 {
    if free_handle(env) { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_get_set_roundtrip() {
        let env = __RTS_FN_NS_GC_ENV_ALLOC(4);
        assert_ne!(env, 0);
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(env, 0), 0);
        assert_eq!(__RTS_FN_NS_GC_ENV_SET(env, 2, 42), 1);
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(env, 2), 42);
        assert_eq!(__RTS_FN_NS_GC_ENV_FREE(env), 1);
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(env, 2), 0);
    }

    #[test]
    fn out_of_range_slot_returns_zero() {
        let env = __RTS_FN_NS_GC_ENV_ALLOC(2);
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(env, 5), 0);
        assert_eq!(__RTS_FN_NS_GC_ENV_SET(env, 5, 99), 0);
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(env, -1), 0);
        __RTS_FN_NS_GC_ENV_FREE(env);
    }

    #[test]
    fn invalid_handle_safe() {
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(0, 0), 0);
        assert_eq!(__RTS_FN_NS_GC_ENV_SET(999_999, 0, 1), 0);
        assert_eq!(__RTS_FN_NS_GC_ENV_FREE(0), 0);
    }
}
