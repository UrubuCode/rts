//! `atomic` namespace — atomic primitives over `std::sync::atomic` (AtomicI64,
//! AtomicBool, AtomicF64-via-AtomicU64, fences).
//!
//! Toda operacao usa `Ordering::SeqCst`. Handles guardam `Box<Atomic*>` para
//! estabilizar o endereco enquanto o slot da HandleTable viver; o lock do shard
//! e liberado ANTES da operacao atomica (lock-free apos o lookup).
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering, fence};

use rts_engine::abi::ty::{Bool, F64, Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

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

/// Aloca um AtomicI64 inicializado com `value` e retorna o handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_NEW(value: I64) -> Handle {
    alloc_entry(Entry::AtomicI64(Box::new(AtomicI64::new(value))))
}

/// Le o valor atual do AtomicI64 (SeqCst). 0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_LOAD(handle: U64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.load(Ordering::SeqCst))
}

/// Escreve `value` no AtomicI64 (SeqCst). No-op se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_STORE(handle: U64, value: I64) {
    with_atomic_i64(handle, (), |a| a.store(value, Ordering::SeqCst));
}

/// Soma `delta` e retorna o valor anterior. 0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_FETCH_ADD(handle: U64, delta: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.fetch_add(delta, Ordering::SeqCst))
}

/// Subtrai `delta` e retorna o valor anterior. 0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_FETCH_SUB(handle: U64, delta: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.fetch_sub(delta, Ordering::SeqCst))
}

/// AND bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_FETCH_AND(handle: U64, mask: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.fetch_and(mask, Ordering::SeqCst))
}

/// OR bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_FETCH_OR(handle: U64, mask: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.fetch_or(mask, Ordering::SeqCst))
}

/// XOR bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_FETCH_XOR(handle: U64, mask: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.fetch_xor(mask, Ordering::SeqCst))
}

/// Troca o valor por `value` e retorna o valor anterior. 0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_SWAP(handle: U64, value: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| a.swap(value, Ordering::SeqCst))
}

/// Compare-and-swap. Se valor atual == `expected`, escreve `new`. Retorna o valor anterior.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_I64_CAS(handle: U64, expected: I64, new_value: I64) -> I64 {
    with_atomic_i64(handle, 0, |a| {
        match a.compare_exchange(expected, new_value, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(prev) | Err(prev) => prev,
        }
    })
}

/// Aloca um AtomicBool inicializado com `value` e retorna o handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_BOOL_NEW(value: Bool) -> Handle {
    alloc_entry(Entry::AtomicBool(Box::new(AtomicBool::new(value != 0))))
}

/// Le o valor atual do AtomicBool (SeqCst). false se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_BOOL_LOAD(handle: U64) -> Bool {
    with_atomic_bool(handle, 0, |a| a.load(Ordering::SeqCst) as i64)
}

/// Escreve `value` no AtomicBool (SeqCst). No-op se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_BOOL_STORE(handle: U64, value: Bool) {
    with_atomic_bool(handle, (), |a| a.store(value != 0, Ordering::SeqCst));
}

/// Troca o valor por `value` e retorna o valor anterior. false se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_BOOL_SWAP(handle: U64, value: Bool) -> Bool {
    with_atomic_bool(handle, 0, |a| a.swap(value != 0, Ordering::SeqCst) as i64)
}

/// Aloca um AtomicF64 inicializado com `value` e retorna o handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_NEW(value: F64) -> Handle {
    alloc_entry(Entry::AtomicF64(Box::new(AtomicU64::new(value.to_bits()))))
}

/// Le o valor atual do AtomicF64 (SeqCst). 0.0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_LOAD(handle: U64) -> F64 {
    with_atomic_f64(handle, 0.0, |a| f64::from_bits(a.load(Ordering::SeqCst)))
}

/// Escreve `value` no AtomicF64 (SeqCst). No-op se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_STORE(handle: U64, value: F64) {
    with_atomic_f64(handle, (), |a| a.store(value.to_bits(), Ordering::SeqCst));
}

/// Soma `delta` e retorna o valor anterior (loop CAS internamente). 0.0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_FETCH_ADD(handle: U64, delta: F64) -> F64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_F64_SWAP(handle: U64, value: F64) -> F64 {
    with_atomic_f64(handle, 0.0, |a| {
        f64::from_bits(a.swap(value.to_bits(), Ordering::SeqCst))
    })
}

/// Memory fence Acquire.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_FENCE_ACQUIRE() {
    fence(Ordering::Acquire);
}

/// Memory fence Release.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_FENCE_RELEASE() {
    fence(Ordering::Release);
}

/// Memory fence SeqCst.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ATOMIC_FENCE_SEQ_CST() {
    fence(Ordering::SeqCst);
}

/// Função `atomic.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace `atomic` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("atomic")
        .doc("Primitivas atomicas (AtomicI64, AtomicBool, fences) baseadas em std::sync::atomic.")
        .member(func(
            "i64_new",
            "__RTS_FN_NS_ATOMIC_I64_NEW",
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "i64_new(value: number): number",
            "Aloca um AtomicI64 inicializado com `value` e retorna o handle.",
            __RTS_FN_NS_ATOMIC_I64_NEW as *const u8,
        ))
        .member(func(
            "i64_load",
            "__RTS_FN_NS_ATOMIC_I64_LOAD",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "i64_load(handle: number): number",
            "Le o valor atual do AtomicI64 (SeqCst). 0 se handle invalido.",
            __RTS_FN_NS_ATOMIC_I64_LOAD as *const u8,
        ))
        .member(func(
            "i64_store",
            "__RTS_FN_NS_ATOMIC_I64_STORE",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::Void),
            "i64_store(handle: number, value: number): void",
            "Escreve `value` no AtomicI64 (SeqCst). No-op se handle invalido.",
            __RTS_FN_NS_ATOMIC_I64_STORE as *const u8,
        ))
        .member(func(
            "i64_fetch_add",
            "__RTS_FN_NS_ATOMIC_I64_FETCH_ADD",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I64),
            "i64_fetch_add(handle: number, delta: number): number",
            "Soma `delta` e retorna o valor anterior. 0 se handle invalido.",
            __RTS_FN_NS_ATOMIC_I64_FETCH_ADD as *const u8,
        ))
        .member(func(
            "i64_fetch_sub",
            "__RTS_FN_NS_ATOMIC_I64_FETCH_SUB",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I64),
            "i64_fetch_sub(handle: number, delta: number): number",
            "Subtrai `delta` e retorna o valor anterior. 0 se handle invalido.",
            __RTS_FN_NS_ATOMIC_I64_FETCH_SUB as *const u8,
        ))
        .member(func(
            "i64_fetch_and",
            "__RTS_FN_NS_ATOMIC_I64_FETCH_AND",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I64),
            "i64_fetch_and(handle: number, mask: number): number",
            "AND bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.",
            __RTS_FN_NS_ATOMIC_I64_FETCH_AND as *const u8,
        ))
        .member(func(
            "i64_fetch_or",
            "__RTS_FN_NS_ATOMIC_I64_FETCH_OR",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I64),
            "i64_fetch_or(handle: number, mask: number): number",
            "OR bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.",
            __RTS_FN_NS_ATOMIC_I64_FETCH_OR as *const u8,
        ))
        .member(func(
            "i64_fetch_xor",
            "__RTS_FN_NS_ATOMIC_I64_FETCH_XOR",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I64),
            "i64_fetch_xor(handle: number, mask: number): number",
            "XOR bit-a-bit com `mask` e retorna o valor anterior. 0 se handle invalido.",
            __RTS_FN_NS_ATOMIC_I64_FETCH_XOR as *const u8,
        ))
        .member(func(
            "i64_swap",
            "__RTS_FN_NS_ATOMIC_I64_SWAP",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I64),
            "i64_swap(handle: number, value: number): number",
            "Troca o valor por `value` e retorna o valor anterior. 0 se handle invalido.",
            __RTS_FN_NS_ATOMIC_I64_SWAP as *const u8,
        ))
        .member(func(
            "i64_cas",
            "__RTS_FN_NS_ATOMIC_I64_CAS",
            Sig::new(vec![AbiType::U64, AbiType::I64, AbiType::I64], AbiType::I64),
            "i64_cas(handle: number, expected: number, new_value: number): number",
            "Compare-and-swap. Se valor atual == `expected`, escreve `new`. Retorna o valor anterior.",
            __RTS_FN_NS_ATOMIC_I64_CAS as *const u8,
        ))
        .member(func(
            "bool_new",
            "__RTS_FN_NS_ATOMIC_BOOL_NEW",
            Sig::new(vec![AbiType::Bool], AbiType::Handle),
            "bool_new(value: boolean): number",
            "Aloca um AtomicBool inicializado com `value` e retorna o handle.",
            __RTS_FN_NS_ATOMIC_BOOL_NEW as *const u8,
        ))
        .member(func(
            "bool_load",
            "__RTS_FN_NS_ATOMIC_BOOL_LOAD",
            Sig::new(vec![AbiType::U64], AbiType::Bool),
            "bool_load(handle: number): boolean",
            "Le o valor atual do AtomicBool (SeqCst). false se handle invalido.",
            __RTS_FN_NS_ATOMIC_BOOL_LOAD as *const u8,
        ))
        .member(func(
            "bool_store",
            "__RTS_FN_NS_ATOMIC_BOOL_STORE",
            Sig::new(vec![AbiType::U64, AbiType::Bool], AbiType::Void),
            "bool_store(handle: number, value: boolean): void",
            "Escreve `value` no AtomicBool (SeqCst). No-op se handle invalido.",
            __RTS_FN_NS_ATOMIC_BOOL_STORE as *const u8,
        ))
        .member(func(
            "bool_swap",
            "__RTS_FN_NS_ATOMIC_BOOL_SWAP",
            Sig::new(vec![AbiType::U64, AbiType::Bool], AbiType::Bool),
            "bool_swap(handle: number, value: boolean): boolean",
            "Troca o valor por `value` e retorna o valor anterior. false se handle invalido.",
            __RTS_FN_NS_ATOMIC_BOOL_SWAP as *const u8,
        ))
        .member(func(
            "f64_new",
            "__RTS_FN_NS_ATOMIC_F64_NEW",
            Sig::new(vec![AbiType::F64], AbiType::Handle),
            "f64_new(value: number): number",
            "Aloca um AtomicF64 inicializado com `value` e retorna o handle.",
            __RTS_FN_NS_ATOMIC_F64_NEW as *const u8,
        ))
        .member(func(
            "f64_load",
            "__RTS_FN_NS_ATOMIC_F64_LOAD",
            Sig::new(vec![AbiType::U64], AbiType::F64),
            "f64_load(handle: number): number",
            "Le o valor atual do AtomicF64 (SeqCst). 0.0 se handle invalido.",
            __RTS_FN_NS_ATOMIC_F64_LOAD as *const u8,
        ))
        .member(func(
            "f64_store",
            "__RTS_FN_NS_ATOMIC_F64_STORE",
            Sig::new(vec![AbiType::U64, AbiType::F64], AbiType::Void),
            "f64_store(handle: number, value: number): void",
            "Escreve `value` no AtomicF64 (SeqCst). No-op se handle invalido.",
            __RTS_FN_NS_ATOMIC_F64_STORE as *const u8,
        ))
        .member(func(
            "f64_fetch_add",
            "__RTS_FN_NS_ATOMIC_F64_FETCH_ADD",
            Sig::new(vec![AbiType::U64, AbiType::F64], AbiType::F64),
            "f64_fetch_add(handle: number, delta: number): number",
            "Soma `delta` e retorna o valor anterior (loop CAS internamente). 0.0 se handle invalido.",
            __RTS_FN_NS_ATOMIC_F64_FETCH_ADD as *const u8,
        ))
        .member(func(
            "f64_swap",
            "__RTS_FN_NS_ATOMIC_F64_SWAP",
            Sig::new(vec![AbiType::U64, AbiType::F64], AbiType::F64),
            "f64_swap(handle: number, value: number): number",
            "Troca o valor por `value` e retorna o valor anterior. 0.0 se handle invalido.",
            __RTS_FN_NS_ATOMIC_F64_SWAP as *const u8,
        ))
        .member(func(
            "fence_acquire",
            "__RTS_FN_NS_ATOMIC_FENCE_ACQUIRE",
            Sig::new(Vec::new(), AbiType::Void),
            "fence_acquire(): void",
            "Memory fence Acquire.",
            __RTS_FN_NS_ATOMIC_FENCE_ACQUIRE as *const u8,
        ))
        .member(func(
            "fence_release",
            "__RTS_FN_NS_ATOMIC_FENCE_RELEASE",
            Sig::new(Vec::new(), AbiType::Void),
            "fence_release(): void",
            "Memory fence Release.",
            __RTS_FN_NS_ATOMIC_FENCE_RELEASE as *const u8,
        ))
        .member(func(
            "fence_seq_cst",
            "__RTS_FN_NS_ATOMIC_FENCE_SEQ_CST",
            Sig::new(Vec::new(), AbiType::Void),
            "fence_seq_cst(): void",
            "Memory fence SeqCst.",
            __RTS_FN_NS_ATOMIC_FENCE_SEQ_CST as *const u8,
        ))
        .done();
}
