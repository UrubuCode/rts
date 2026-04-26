//! thread::spawn — invoca `extern "C" fn(u64) -> u64` em nova thread.
//!
//! O ponteiro de funcao chega como `u64` (passado pelo codegen/TS-side).
//! Reconstruimos como `extern "C" fn(u64) -> u64` via transmute e
//! invocamos dentro de `std::thread::spawn`. O `JoinHandle<u64>` vai pra
//! `HandleTable` como `Entry::JoinHandle`.

use std::thread;

use super::super::gc::handles::{Entry, table};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SPAWN(fn_ptr: u64, arg: u64) -> u64 {
    if fn_ptr == 0 {
        return 0;
    }
    // SAFETY: caller (codegen) garante que `fn_ptr` aponta para uma
    // funcao com assinatura `extern "C" fn(u64) -> u64`. Nao podemos
    // validar runtime — contrato com o compilador.
    let f: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let handle = thread::spawn(move || f(arg));
    table()
        .lock()
        .unwrap()
        .alloc(Entry::JoinHandle(Box::new(handle)))
}

/// Variante com userdata (#227): trampolim recebe `(ud, arg)`. Usado
/// quando arrow capturada por `thread.spawn` referencia `this` — o
/// lifter passa o handle do `this` como `ud`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SPAWN_WITH_UD(fn_ptr: u64, arg: u64, ud: u64) -> u64 {
    if fn_ptr == 0 {
        return 0;
    }
    // SAFETY: contrato com o codegen — `fn_ptr` aponta para
    // `extern "C" fn(u64, u64) -> u64`.
    let f: extern "C" fn(u64, u64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let handle = thread::spawn(move || f(ud, arg));
    table()
        .lock()
        .unwrap()
        .alloc(Entry::JoinHandle(Box::new(handle)))
}
