//! AtomicF64 — operações atômicas f64 emuladas via AtomicU64 + transmute
//! de bits. Rust não tem AtomicF64 nativo. Ordering::SeqCst pra simetria
//! com i64/bool.

use std::sync::atomic::{AtomicU64, Ordering};

use super::super::gc::handles::{Entry, table};

fn with_atomic_f64<R>(handle: u64, default: R, f: impl FnOnce(&AtomicU64) -> R) -> R {
    let guard = table().lock().unwrap();
    match guard.get(handle) {
        Some(Entry::AtomicF64(a)) => f(a.as_ref()),
        _ => default,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_NEW(value: f64) -> u64 {
    table()
        .lock()
        .unwrap()
        .alloc(Entry::AtomicF64(Box::new(AtomicU64::new(value.to_bits()))))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_LOAD(handle: u64) -> f64 {
    with_atomic_f64(handle, 0.0, |a| f64::from_bits(a.load(Ordering::SeqCst)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_STORE(handle: u64, value: f64) {
    with_atomic_f64(handle, (), |a| a.store(value.to_bits(), Ordering::SeqCst));
}

/// CAS-loop para fetch_add: f64 não é safe atomicamente sem CAS.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_FETCH_ADD(handle: u64, delta: f64) -> f64 {
    with_atomic_f64(handle, 0.0, |a| {
        let mut prev_bits = a.load(Ordering::Relaxed);
        loop {
            let prev = f64::from_bits(prev_bits);
            let new = prev + delta;
            match a.compare_exchange_weak(
                prev_bits,
                new.to_bits(),
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return prev,
                Err(actual) => prev_bits = actual,
            }
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_SWAP(handle: u64, value: f64) -> f64 {
    with_atomic_f64(handle, 0.0, |a| {
        f64::from_bits(a.swap(value.to_bits(), Ordering::SeqCst))
    })
}
