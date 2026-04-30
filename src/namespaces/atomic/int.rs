//! AtomicI64 — operacoes atomicas i64 com Ordering::SeqCst.

use std::sync::atomic::{AtomicI64, Ordering};

use super::super::gc::handles::{Entry, alloc_entry, with_entry};

fn with_atomic_i64<R>(handle: u64, default: R, f: impl FnOnce(&AtomicI64) -> R) -> R {
    let ptr: *const AtomicI64 = with_entry(handle, |entry| match entry {
        Some(Entry::AtomicI64(a)) => a.as_ref() as *const _,
        _ => std::ptr::null(),
    });
    if ptr.is_null() {
        return default;
    }
    // SAFETY: Box<AtomicI64> is heap-allocated and stable. Lock released —
    // atomic op runs lock-free. Caller must not free the handle concurrently.
    f(unsafe { &*ptr })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_NEW(value: i64) -> u64 {
    alloc_entry(Entry::AtomicI64(Box::new(AtomicI64::new(value))))
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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_CAS(handle: u64, expected: i64, new: i64) -> i64 {
    with_atomic_i64(handle, 0, |a| {
        match a.compare_exchange(expected, new, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(prev) | Err(prev) => prev,
        }
    })
}
