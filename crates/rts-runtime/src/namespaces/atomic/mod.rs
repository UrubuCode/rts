//! `atomic` namespace — atomic primitives over `std::sync::atomic` (AtomicI64,
//! AtomicBool, AtomicF64-via-AtomicU64, fences).
//!
//! Toda operacao usa `Ordering::SeqCst`. Handles guardam `Box<Atomic*>` para
//! estabilizar o endereco enquanto o slot da HandleTable viver; o lock do shard
//! e liberado ANTES da operacao atomica (lock-free apos o lookup).
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering, fence};

use rts_abi::ty::{Bool, F64, Handle, I64, U64};
use rts_macro::rts_namespace;

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};

fn with_atomic_i64<R>(handle: u64, default: R, f: impl FnOnce(&AtomicI64) -> R) -> R {
    let ptr: *const AtomicI64 = with_entry(handle, |entry| match entry {
        Some(Entry::AtomicI64(a)) => a.as_ref() as *const _,
        _ => std::ptr::null(),
    });
    if ptr.is_null() {
        return default;
    }
    // SAFETY: Box<AtomicI64> is heap-stable; lock released — op runs lock-free.
    f(unsafe { &*ptr })
}

fn with_atomic_bool<R>(handle: u64, default: R, f: impl FnOnce(&AtomicBool) -> R) -> R {
    let ptr: *const AtomicBool = with_entry(handle, |entry| match entry {
        Some(Entry::AtomicBool(a)) => a.as_ref() as *const _,
        _ => std::ptr::null(),
    });
    if ptr.is_null() {
        return default;
    }
    // SAFETY: see `with_atomic_i64`.
    f(unsafe { &*ptr })
}

fn with_atomic_f64<R>(handle: u64, default: R, f: impl FnOnce(&AtomicU64) -> R) -> R {
    let ptr: *const AtomicU64 = with_entry(handle, |entry| match entry {
        Some(Entry::AtomicF64(a)) => a.as_ref() as *const _,
        _ => std::ptr::null(),
    });
    if ptr.is_null() {
        return default;
    }
    // SAFETY: see `with_atomic_i64`.
    f(unsafe { &*ptr })
}

/// Primitivas atomicas (AtomicI64, AtomicBool, fences) baseadas em std::sync::atomic.
#[rts_namespace(atomic)]
impl AtomicNs {
    /// Aloca um AtomicI64 inicializado com `value` e retorna o handle.
    #[rts_fn]
    pub fn i64_new(value: I64) -> Handle {
        alloc_entry(Entry::AtomicI64(Box::new(AtomicI64::new(value))))
    }

    /// Le o valor atual do AtomicI64 (SeqCst). 0 se handle invalido.
    #[rts_fn]
    pub fn i64_load(handle: U64) -> I64 {
        with_atomic_i64(handle, 0, |a| a.load(Ordering::SeqCst))
    }

    /// Escreve `value` no AtomicI64 (SeqCst). No-op se handle invalido.
    #[rts_fn]
    pub fn i64_store(handle: U64, value: I64) {
        with_atomic_i64(handle, (), |a| a.store(value, Ordering::SeqCst));
    }

    /// Soma `delta` e retorna o valor anterior. 0 se handle invalido.
    #[rts_fn]
    pub fn i64_fetch_add(handle: U64, delta: I64) -> I64 {
        with_atomic_i64(handle, 0, |a| a.fetch_add(delta, Ordering::SeqCst))
    }

    /// Subtrai `delta` e retorna o valor anterior. 0 se handle invalido.
    #[rts_fn]
    pub fn i64_fetch_sub(handle: U64, delta: I64) -> I64 {
        with_atomic_i64(handle, 0, |a| a.fetch_sub(delta, Ordering::SeqCst))
    }

    /// AND bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
    #[rts_fn]
    pub fn i64_fetch_and(handle: U64, mask: I64) -> I64 {
        with_atomic_i64(handle, 0, |a| a.fetch_and(mask, Ordering::SeqCst))
    }

    /// OR bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
    #[rts_fn]
    pub fn i64_fetch_or(handle: U64, mask: I64) -> I64 {
        with_atomic_i64(handle, 0, |a| a.fetch_or(mask, Ordering::SeqCst))
    }

    /// XOR bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
    #[rts_fn]
    pub fn i64_fetch_xor(handle: U64, mask: I64) -> I64 {
        with_atomic_i64(handle, 0, |a| a.fetch_xor(mask, Ordering::SeqCst))
    }

    /// Troca o valor por `value` e retorna o valor anterior. 0 se handle invalido.
    #[rts_fn]
    pub fn i64_swap(handle: U64, value: I64) -> I64 {
        with_atomic_i64(handle, 0, |a| a.swap(value, Ordering::SeqCst))
    }

    /// Compare-and-swap. Se valor atual == `expected`, escreve `new`. Retorna o valor anterior.
    #[rts_fn(ts = "i64_cas(handle: number, expected: number, new_value: number): number")]
    pub fn i64_cas(handle: U64, expected: I64, new_value: I64) -> I64 {
        with_atomic_i64(handle, 0, |a| {
            match a.compare_exchange(expected, new_value, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(prev) | Err(prev) => prev,
            }
        })
    }

    /// Aloca um AtomicBool inicializado com `value` e retorna o handle.
    #[rts_fn]
    pub fn bool_new(value: Bool) -> Handle {
        alloc_entry(Entry::AtomicBool(Box::new(AtomicBool::new(value != 0))))
    }

    /// Le o valor atual do AtomicBool (SeqCst). false se handle invalido.
    #[rts_fn]
    pub fn bool_load(handle: U64) -> Bool {
        with_atomic_bool(handle, 0, |a| a.load(Ordering::SeqCst) as i64)
    }

    /// Escreve `value` no AtomicBool (SeqCst). No-op se handle invalido.
    #[rts_fn]
    pub fn bool_store(handle: U64, value: Bool) {
        with_atomic_bool(handle, (), |a| a.store(value != 0, Ordering::SeqCst));
    }

    /// Troca o valor por `value` e retorna o valor anterior. false se handle invalido.
    #[rts_fn]
    pub fn bool_swap(handle: U64, value: Bool) -> Bool {
        with_atomic_bool(handle, 0, |a| a.swap(value != 0, Ordering::SeqCst) as i64)
    }

    /// Aloca um AtomicF64 inicializado com `value` e retorna o handle.
    #[rts_fn]
    pub fn f64_new(value: F64) -> Handle {
        alloc_entry(Entry::AtomicF64(Box::new(AtomicU64::new(value.to_bits()))))
    }

    /// Le o valor atual do AtomicF64 (SeqCst). 0.0 se handle invalido.
    #[rts_fn]
    pub fn f64_load(handle: U64) -> F64 {
        with_atomic_f64(handle, 0.0, |a| f64::from_bits(a.load(Ordering::SeqCst)))
    }

    /// Escreve `value` no AtomicF64 (SeqCst). No-op se handle invalido.
    #[rts_fn]
    pub fn f64_store(handle: U64, value: F64) {
        with_atomic_f64(handle, (), |a| a.store(value.to_bits(), Ordering::SeqCst));
    }

    /// Soma `delta` e retorna o valor anterior (loop CAS internamente). 0.0 se handle invalido.
    #[rts_fn]
    pub fn f64_fetch_add(handle: U64, delta: F64) -> F64 {
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

    /// Troca o valor por `value` e retorna o valor anterior. 0.0 se handle invalido.
    #[rts_fn]
    pub fn f64_swap(handle: U64, value: F64) -> F64 {
        with_atomic_f64(handle, 0.0, |a| {
            f64::from_bits(a.swap(value.to_bits(), Ordering::SeqCst))
        })
    }

    /// Memory fence Acquire.
    #[rts_fn]
    pub fn fence_acquire() {
        fence(Ordering::Acquire);
    }

    /// Memory fence Release.
    #[rts_fn]
    pub fn fence_release() {
        fence(Ordering::Release);
    }

    /// Memory fence SeqCst.
    #[rts_fn]
    pub fn fence_seq_cst() {
        fence(Ordering::SeqCst);
    }
}
