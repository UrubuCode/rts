//! OnceLock — execucao unica de uma funcao via `std::sync::Once`.
//!
//! `once_call` recebe um ponteiro de funcao `extern "C" fn()` (codificado
//! como i64 pelo codegen) e o invoca dentro de `Once::call_once`. Chamadas
//! subsequentes sao no-op, garantindo execucao unica mesmo sob multiplas
//! threads.

use std::sync::Once;

use super::super::gc::handles::{Entry, alloc_entry, shard_for_handle};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_ONCE_NEW() -> u64 {
    alloc_entry(Entry::SyncOnce(Box::new(Once::new())))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_ONCE_CALL(handle: u64, fn_ptr: i64) {
    if fn_ptr == 0 {
        return;
    }
    // Obtem ponteiro estavel para o Once dentro do Box.
    let once_ptr: *const Once = {
        let guard = shard_for_handle(handle).lock().unwrap();
        match guard.get(handle) {
            Some(Entry::SyncOnce(o)) => o.as_ref() as *const Once,
            _ => return,
        }
    };
    // SAFETY: once_ptr permanece valido enquanto o slot na HandleTable
    // existir (caller nao deveria liberar durante a chamada). fn_ptr e
    // tratado como `extern "C" fn()` por contrato com o codegen.
    let once: &'static Once = unsafe { &*once_ptr };
    let f: extern "C" fn() = unsafe { std::mem::transmute(fn_ptr as usize) };
    once.call_once(|| f());
}
