//! AtomicBool — operacoes atomicas bool com Ordering::SeqCst.
//!
//! Bool no contrato ABI lowera para i64 (ver `abi::signature`); por isso
//! os parametros e retornos aqui sao `i64` (0 = false, !=0 = true).

use std::sync::atomic::{AtomicBool, Ordering};

use super::super::gc::handles::{Entry, table};

fn with_atomic_bool<R>(handle: u64, default: R, f: impl FnOnce(&AtomicBool) -> R) -> R {
    let guard = table().lock().unwrap();
    match guard.get(handle) {
        Some(Entry::AtomicBool(a)) => f(a.as_ref()),
        _ => default,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_BOOL_NEW(value: i64) -> u64 {
    table()
        .lock()
        .unwrap()
        .alloc(Entry::AtomicBool(Box::new(AtomicBool::new(value != 0))))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_BOOL_LOAD(handle: u64) -> i64 {
    with_atomic_bool(handle, 0i64, |a| a.load(Ordering::SeqCst) as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_BOOL_STORE(handle: u64, value: i64) {
    with_atomic_bool(handle, (), |a| a.store(value != 0, Ordering::SeqCst));
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_BOOL_SWAP(handle: u64, value: i64) -> i64 {
    with_atomic_bool(handle, 0i64, |a| a.swap(value != 0, Ordering::SeqCst) as i64)
}
