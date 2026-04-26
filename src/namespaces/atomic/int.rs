//! AtomicI64 — operacoes atomicas i64 com Ordering::SeqCst.

use std::sync::atomic::{AtomicI64, Ordering};

use super::super::gc::handles::{Entry, table};

fn with_atomic_i64<R>(handle: u64, default: R, f: impl FnOnce(&AtomicI64) -> R) -> R {
    let guard = table().lock().unwrap();
    match guard.get(handle) {
        Some(Entry::AtomicI64(a)) => f(a.as_ref()),
        _ => default,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_NEW(value: i64) -> u64 {
    table()
        .lock()
        .unwrap()
        .alloc(Entry::AtomicI64(Box::new(AtomicI64::new(value))))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_LOAD(handle: u64) -> i64 {
    with_atomic_i64(handle, 0, |a| a.load(Ordering::SeqCst))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_STORE(handle: u64, value: i64) {
    with_atomic_i64(handle, (), |a| a.store(value, Ordering::SeqCst));
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_FETCH_ADD(handle: u64, delta: i64) -> i64 {
    with_atomic_i64(handle, 0, |a| a.fetch_add(delta, Ordering::SeqCst))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_FETCH_SUB(handle: u64, delta: i64) -> i64 {
    with_atomic_i64(handle, 0, |a| a.fetch_sub(delta, Ordering::SeqCst))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_FETCH_AND(handle: u64, mask: i64) -> i64 {
    with_atomic_i64(handle, 0, |a| a.fetch_and(mask, Ordering::SeqCst))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_FETCH_OR(handle: u64, mask: i64) -> i64 {
    with_atomic_i64(handle, 0, |a| a.fetch_or(mask, Ordering::SeqCst))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_FETCH_XOR(handle: u64, mask: i64) -> i64 {
    with_atomic_i64(handle, 0, |a| a.fetch_xor(mask, Ordering::SeqCst))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_SWAP(handle: u64, value: i64) -> i64 {
    with_atomic_i64(handle, 0, |a| a.swap(value, Ordering::SeqCst))
}

/// Compare-and-swap. Retorna o valor anterior — caller decide sucesso
/// comparando com `expected`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_CAS(handle: u64, expected: i64, new_value: i64) -> i64 {
    with_atomic_i64(handle, 0, |a| {
        match a.compare_exchange(expected, new_value, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(prev) => prev,
            Err(actual) => actual,
        }
    })
}
