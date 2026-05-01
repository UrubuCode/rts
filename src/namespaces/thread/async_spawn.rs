//! `thread.spawn_async` / `spawn_async_join` / `join_async` — spawn que
//! reusa o runtime tokio compartilhado (issue #399).
//!
//! Mantido em arquivo separado de `spawn.rs` porque `spawn.rs` faz parte
//! do `runtime_support` (compilado pelo `build.rs` standalone, sem acesso
//! ao `crate::runtime::async_rt`). Este arquivo so' compila no crate
//! principal — o JIT registra o simbolo direto via `add_fn!`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::task::JoinHandle;

/// Store dedicado para `JoinHandle<u64>` do tokio. Nao usamos
/// `tokio_ctx::put` porque `JoinHandle: Send` mas nao `Sync`, e o
/// store generico exige ambos. Aqui usamos `Mutex<Option<...>>` por
/// slot — `take` move o handle.
fn join_store() -> &'static Mutex<HashMap<u64, JoinHandle<u64>>> {
    static S: OnceLock<Mutex<HashMap<u64, JoinHandle<u64>>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_join_id() -> u64 {
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Submete `fn_ptr(arg)` ao runtime tokio compartilhado, fire-and-forget.
/// Roda em `spawn_blocking` (nao trava reactor com codigo JIT sincrono).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SPAWN_ASYNC(fn_ptr: u64, arg: u64) {
    if fn_ptr == 0 {
        return;
    }
    let handle = crate::runtime::async_rt::handle();
    handle.spawn_blocking(move || {
        // SAFETY: contrato com codegen — `fn_ptr` aponta para
        // `extern "C" fn(u64) -> u64`.
        let f: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
        let _ = f(arg);
    });
}

/// Como `spawn_async` mas retorna um id `u64` que pode ser passado pra
/// `join_async` para aguardar e pegar o valor de retorno.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SPAWN_ASYNC_JOIN(fn_ptr: u64, arg: u64) -> u64 {
    if fn_ptr == 0 {
        return 0;
    }
    let handle = crate::runtime::async_rt::handle();
    let jh: JoinHandle<u64> = handle.spawn_blocking(move || {
        // SAFETY: contrato com codegen — `fn_ptr` aponta para
        // `extern "C" fn(u64) -> u64`.
        let f: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
        f(arg)
    });
    let id = next_join_id();
    join_store()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, jh);
    id
}

/// Aguarda task `id` (criada via `spawn_async_join`) terminar e retorna
/// seu valor. Consome o id. 0 se id invalido ou panic na task.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_JOIN_ASYNC(id: u64) -> u64 {
    let jh = {
        let mut map = join_store().lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&id)
    };
    let Some(jh) = jh else { return 0 };
    // Bloqueia thread atual ate task terminar. `block_on` precisa de
    // runtime tokio — usamos o handle global e `Handle::block_on` que
    // funciona mesmo quando a thread nao e' worker tokio.
    let rt = crate::runtime::async_rt::rt();
    match rt.block_on(jh) {
        Ok(v) => v,
        Err(_) => 0,
    }
}
