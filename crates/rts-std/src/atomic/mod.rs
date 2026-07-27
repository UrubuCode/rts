//! `atomic` namespace — atomic primitives over `std::sync::atomic` (AtomicI64,
//! AtomicBool, AtomicF64-via-AtomicU64, fences).
//!
//! Toda operacao usa `Ordering::SeqCst`. Handles guardam `Box<Atomic*>` para
//! estabilizar o endereco enquanto o slot da HandleTable viver; o lock do shard
//! e liberado ANTES da operacao atomica (lock-free apos o lookup).
//!
//! Os membros ABI sao declarados com `#[rtse::function]` (F7 de
//! `docs/specs/rts-macro-single-source.md`): simbolo, assinatura,
//! `ts_signature` e fn-ptr saem derivados da fn Rust.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering, fence};

use rts_engine::abi::ty::{Bool, F64, Handle, I64, U64};
use rts_engine::Engine;

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry};

// ── pointer memo: keep the shard MUTEX off the atomic fast path ──────────────
//
// Resolving `handle -> &Atomic*` used to lock the shard on EVERY operation. So a
// namespace whose entire purpose is lock-free concurrency serialized every op
// through a mutex: 4 threads doing 8M `fetch_add` took ~1.0 s, while the same 8M
// operations on ONE thread cost ~37 ms — scaling was NEGATIVE, the signature of a
// lock convoy, not of atomic contention.
//
// The fix is not a different atomic instruction (the `lock xadd` itself measured
// ~1 ns of the 4.6 ns per op); it is not taking a lock to find the address. Each
// helper memoizes the LAST handle it resolved, per thread. The hot pattern — many
// operations on one counter — then hits the memo and never touches the shard.
//
// SAFETY, and why this is not a new class of hazard:
//  * `Box<Atomic*>` is heap-stable, so a resolved address stays valid for the
//    entry's lifetime.
//  * The pre-existing code ALREADY released the shard lock before dereferencing
//    the pointer (that was the point of the original design). Caching the address
//    does not introduce the "entry dies between lookup and use" window — it was
//    already there.
//  * The `atomic` namespace exposes NO free/close: an entry is reclaimed only by
//    the GC, and only once it is UNREACHABLE. If TS code can still name the handle
//    to perform an operation, the entry is reachable and therefore not collected.
//  * The memo is keyed by the FULL handle (16-bit generation + slot), so a recycled
//    slot yields a different handle value and cannot hit a stale entry.
//  * One memo PER TYPE, never a shared one keyed by handle alone — that would let
//    `bool_load(h)` reinterpret an `AtomicI64` address as `AtomicBool`.
macro_rules! atomic_accessor {
    ($name:ident, $memo:ident, $ty:ty, $variant:ident) => {
        thread_local! {
            static $memo: std::cell::Cell<(u64, *const $ty)> =
                const { std::cell::Cell::new((0, std::ptr::null())) };
        }

        fn $name<R>(handle: u64, default: R, f: impl FnOnce(&$ty) -> R) -> R {
            if handle == 0 {
                return default;
            }
            let (memo_handle, memo_ptr) = $memo.with(|c| c.get());
            let ptr = if memo_handle == handle {
                memo_ptr
            } else {
                let p: *const $ty = with_entry(handle, |entry| match entry {
                    Some(Entry::$variant(a)) => a.as_ref() as *const _,
                    _ => std::ptr::null(),
                });
                if !p.is_null() {
                    $memo.with(|c| c.set((handle, p)));
                }
                p
            };
            if ptr.is_null() {
                return default;
            }
            // SAFETY: see the argument above this macro.
            f(unsafe { &*ptr })
        }
    };
}

atomic_accessor!(with_atomic_i64, I64_PTR_MEMO, AtomicI64, AtomicI64);
atomic_accessor!(with_atomic_bool, BOOL_PTR_MEMO, AtomicBool, AtomicBool);
atomic_accessor!(with_atomic_f64, F64_PTR_MEMO, AtomicU64, AtomicF64);

/// Aloca um AtomicI64 inicializado com `value` e retorna o handle.
///
/// `#[ts("number")]`: o handle e uma chave OPACA que o TS trata como numero.
/// Derivado (`object`) o motor reboxaria como objeto e todo `atomic.i64_*(h)`
/// seguinte receberia um handle invalido.
#[rtse::function(module = "atomic", value = "i64_new")]
#[ts("number")]
pub fn i64_new(value: I64) -> Handle {
    alloc_entry(Entry::AtomicI64(Box::new(AtomicI64::new(value))))
}

/// Le o valor atual do AtomicI64 (SeqCst). 0 se handle invalido.
#[rtse::function(module = "atomic", value = "i64_load")]
pub fn i64_load(handle: U64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.load(Ordering::SeqCst))
}

/// Escreve `value` no AtomicI64 (SeqCst). No-op se handle invalido.
#[rtse::function(module = "atomic", value = "i64_store")]
pub fn i64_store(handle: U64, value: I64) {
    with_atomic_i64(handle, (), |a| a.store(value, Ordering::SeqCst));
}

/// Soma `delta` e retorna o valor anterior. 0 se handle invalido.
#[rtse::function(module = "atomic", value = "i64_fetch_add")]
pub fn i64_fetch_add(handle: U64, delta: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.fetch_add(delta, Ordering::SeqCst))
}

/// Subtrai `delta` e retorna o valor anterior. 0 se handle invalido.
#[rtse::function(module = "atomic", value = "i64_fetch_sub")]
pub fn i64_fetch_sub(handle: U64, delta: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.fetch_sub(delta, Ordering::SeqCst))
}

/// AND bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
#[rtse::function(module = "atomic", value = "i64_fetch_and")]
pub fn i64_fetch_and(handle: U64, mask: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.fetch_and(mask, Ordering::SeqCst))
}

/// OR bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
#[rtse::function(module = "atomic", value = "i64_fetch_or")]
pub fn i64_fetch_or(handle: U64, mask: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.fetch_or(mask, Ordering::SeqCst))
}

/// XOR bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
#[rtse::function(module = "atomic", value = "i64_fetch_xor")]
pub fn i64_fetch_xor(handle: U64, mask: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.fetch_xor(mask, Ordering::SeqCst))
}

/// Troca o valor por `value` e retorna o valor anterior. 0 se handle invalido.
#[rtse::function(module = "atomic", value = "i64_swap")]
pub fn i64_swap(handle: U64, value: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.swap(value, Ordering::SeqCst))
}

/// Compare-and-swap. Se valor atual == `expected`, escreve `new`. Retorna o valor anterior.
#[rtse::function(module = "atomic", value = "i64_cas")]
pub fn i64_cas(handle: U64, expected: I64, new_value: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| {
        match a.compare_exchange(expected, new_value, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(prev) | Err(prev) => prev,
        }
    })
}

/// Aloca um AtomicBool inicializado com `value` e retorna o handle.
#[rtse::function(module = "atomic", value = "bool_new")]
#[ts("number")]
pub fn bool_new(value: Bool) -> Handle {
    alloc_entry(Entry::AtomicBool(Box::new(AtomicBool::new(value != 0))))
}

/// Le o valor atual do AtomicBool (SeqCst). false se handle invalido.
#[rtse::function(module = "atomic", value = "bool_load")]
pub fn bool_load(handle: U64) -> Bool {
    with_atomic_bool(handle, 0, |a| a.load(Ordering::SeqCst) as i64)
}

/// Escreve `value` no AtomicBool (SeqCst). No-op se handle invalido.
#[rtse::function(module = "atomic", value = "bool_store")]
pub fn bool_store(handle: U64, value: Bool) {
    with_atomic_bool(handle, (), |a| a.store(value != 0, Ordering::SeqCst));
}

/// Troca o valor por `value` e retorna o valor anterior. false se handle invalido.
#[rtse::function(module = "atomic", value = "bool_swap")]
pub fn bool_swap(handle: U64, value: Bool) -> Bool {
    with_atomic_bool(handle, 0, |a| a.swap(value != 0, Ordering::SeqCst) as i64)
}

/// Aloca um AtomicF64 inicializado com `value` e retorna o handle.
#[rtse::function(module = "atomic", value = "f64_new")]
#[ts("number")]
pub fn f64_new(value: F64) -> Handle {
    alloc_entry(Entry::AtomicF64(Box::new(AtomicU64::new(value.to_bits()))))
}

/// Le o valor atual do AtomicF64 (SeqCst). 0.0 se handle invalido.
#[rtse::function(module = "atomic", value = "f64_load")]
pub fn f64_load(handle: U64) -> F64 {
    with_atomic_f64(handle, 0.0, |a| f64::from_bits(a.load(Ordering::SeqCst)))
}

/// Escreve `value` no AtomicF64 (SeqCst). No-op se handle invalido.
#[rtse::function(module = "atomic", value = "f64_store")]
pub fn f64_store(handle: U64, value: F64) {
    with_atomic_f64(handle, (), |a| a.store(value.to_bits(), Ordering::SeqCst));
}

/// Soma `delta` e retorna o valor anterior (loop CAS internamente). 0.0 se handle invalido.
#[rtse::function(module = "atomic", value = "f64_fetch_add")]
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
#[rtse::function(module = "atomic", value = "f64_swap")]
pub fn f64_swap(handle: U64, value: F64) -> F64 {
    with_atomic_f64(handle, 0.0, |a| {
        f64::from_bits(a.swap(value.to_bits(), Ordering::SeqCst))
    })
}

/// Memory fence Acquire.
#[rtse::function(module = "atomic", value = "fence_acquire")]
pub fn fence_acquire() {
    fence(Ordering::Acquire);
}

/// Memory fence Release.
#[rtse::function(module = "atomic", value = "fence_release")]
pub fn fence_release() {
    fence(Ordering::Release);
}

/// Memory fence SeqCst.
#[rtse::function(module = "atomic", value = "fence_seq_cst")]
pub fn fence_seq_cst() {
    fence(Ordering::SeqCst);
}

/// Registra a namespace `atomic` no motor.
pub fn register(e: &mut Engine) {
    e.module("atomic", |m| {
        m.doc("Primitivas atomicas (AtomicI64, AtomicBool, fences) baseadas em std::sync::atomic.");
        m.registry(i64_new_entry());
        m.registry(i64_load_entry());
        m.registry(i64_store_entry());
        m.registry(i64_fetch_add_entry());
        m.registry(i64_fetch_sub_entry());
        m.registry(i64_fetch_and_entry());
        m.registry(i64_fetch_or_entry());
        m.registry(i64_fetch_xor_entry());
        m.registry(i64_swap_entry());
        m.registry(i64_cas_entry());
        m.registry(bool_new_entry());
        m.registry(bool_load_entry());
        m.registry(bool_store_entry());
        m.registry(bool_swap_entry());
        m.registry(f64_new_entry());
        m.registry(f64_load_entry());
        m.registry(f64_store_entry());
        m.registry(f64_fetch_add_entry());
        m.registry(f64_swap_entry());
        m.registry(fence_acquire_entry());
        m.registry(fence_release_entry());
        m.registry(fence_seq_cst_entry());
    });
}
