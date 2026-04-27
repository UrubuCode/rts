//! thread::spawn / thread::scope — invoca `extern "C" fn(u64) -> u64`
//! em nova thread.
//!
//! O ponteiro de funcao chega como `u64` (passado pelo codegen/TS-side).
//! Reconstruimos como `extern "C" fn(u64) -> u64` via transmute e
//! invocamos dentro de `std::thread::spawn`. O `JoinHandle<u64>` vai pra
//! `HandleTable` como `Entry::JoinHandle`.

use std::cell::RefCell;
use std::thread;

use super::super::gc::handles::{Entry, alloc_entry};

thread_local! {
    /// Stack de scopes ativos. Cada scope acumula handles de spawns
    /// feitos durante sua execucao. `thread.scope` empilha um novo Vec
    /// no inicio e joina todos os handles no fim.
    static SCOPE_STACK: RefCell<Vec<Vec<u64>>> = RefCell::new(Vec::new());
}

fn record_scoped_handle(handle: u64) {
    SCOPE_STACK.with(|s| {
        if let Some(top) = s.borrow_mut().last_mut() {
            top.push(handle);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SPAWN(fn_ptr: u64, arg: u64) -> u64 {
    if fn_ptr == 0 {
        return 0;
    }
    // SAFETY: caller (codegen) garante que `fn_ptr` aponta para uma
    // funcao com assinatura `extern "C" fn(u64) -> u64`. Nao podemos
    // validar runtime — contrato com o compilador.
    let f: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let join_handle = thread::spawn(move || f(arg));
    let h = alloc_entry(Entry::JoinHandle(Box::new(join_handle)));
    record_scoped_handle(h);
    h
}

/// Variante com userdata: trampolim recebe `(ud, arg)`. Usado quando
/// arrow capturada por `thread.spawn` referencia `this` — o lifter
/// passa o handle do `this` como `ud`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SPAWN_WITH_UD(fn_ptr: u64, arg: u64, ud: u64) -> u64 {
    if fn_ptr == 0 {
        return 0;
    }
    // SAFETY: contrato com o codegen — `fn_ptr` aponta para
    // `extern "C" fn(u64, u64) -> u64`.
    let f: extern "C" fn(u64, u64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let join_handle = thread::spawn(move || f(ud, arg));
    let h = alloc_entry(Entry::JoinHandle(Box::new(join_handle)));
    record_scoped_handle(h);
    h
}

/// Roda `body()` num escopo que aguarda automaticamente todas as threads
/// spawnadas durante sua execucao. Analogo a `std::thread::scope` —
/// garante que nenhuma thread escapa do escopo.
///
/// Implementacao: empilha um Vec de handles thread-local antes de chamar
/// o body; ao retornar, joina todos os handles acumulados. Spawns
/// aninhados funcionam — cada scope guarda apenas seus proprios handles.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SCOPE(fn_ptr: u64) {
    if fn_ptr == 0 {
        return;
    }
    SCOPE_STACK.with(|s| s.borrow_mut().push(Vec::new()));
    // SAFETY: callback e trampolim sintetico gerado pelo codegen com
    // assinatura `extern "C" fn()`.
    let f: extern "C" fn() = unsafe { std::mem::transmute(fn_ptr as usize) };
    f();
    let handles = SCOPE_STACK.with(|s| s.borrow_mut().pop().unwrap_or_default());
    for h in handles {
        super::join::__RTS_FN_NS_THREAD_JOIN(h);
    }
}

/// Variante com userdata para `thread.scope` capturando `this`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SCOPE_WITH_UD(fn_ptr: u64, ud: u64) {
    if fn_ptr == 0 {
        return;
    }
    SCOPE_STACK.with(|s| s.borrow_mut().push(Vec::new()));
    // SAFETY: `extern "C" fn(u64)`.
    let f: extern "C" fn(u64) = unsafe { std::mem::transmute(fn_ptr as usize) };
    f(ud);
    let handles = SCOPE_STACK.with(|s| s.borrow_mut().pop().unwrap_or_default());
    for h in handles {
        super::join::__RTS_FN_NS_THREAD_JOIN(h);
    }
}
