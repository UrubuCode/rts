//! AtomicF64 — operacoes atomicas f64 emuladas via AtomicU64 + transmute
//! de bits. Rust nao tem AtomicF64 nativo. Ordering::SeqCst pra simetria
//! com i64/bool.
//!
//! O lock do shard e liberado ANTES de qualquer operacao atomica para
//! que threads concorrentes possam operar lock-free apos o lookup.

use std::sync::atomic::{AtomicU64, Ordering};

use super::super::gc::handles::{Entry, alloc_entry, shard_for_handle};

fn with_atomic_f64<R>(handle: u64, default: R, f: impl FnOnce(&AtomicU64) -> R) -> R {
    let ptr: *const AtomicU64 = {
        let guard = shard_for_handle(handle).lock().unwrap();
        match guard.get(handle) {
            Some(Entry::AtomicF64(a)) => a.as_ref() as *const _,
            _ => return default,
        }
    }; // shard lock released here — atomic op runs lock-free
    // SAFETY: Box<AtomicU64> is heap-allocated and stable. Same contract
    // as AtomicI64: caller must not free the handle during this call.
    f(unsafe { &*ptr })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_NEW(value: f64) -> u64 {
    alloc_entry(Entry::AtomicF64(Box::new(AtomicU64::new(value.to_bits()))))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_LOAD(handle: u64) -> f64 {
    with_atomic_f64(handle, 0.0, |a| f64::from_bits(a.load(Ordering::SeqCst)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_STORE(handle: u64, value: f64) {
    with_atomic_f64(handle, (), |a| a.store(value.to_bits(), Ordering::SeqCst));
}

/// CAS-loop para fetch_add: f64 nao e nativo em AtomicU64.
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
