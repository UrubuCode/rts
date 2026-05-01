//! `thread.spawn_async` — spawn que reusa o runtime tokio compartilhado
//! (issue #399).
//!
//! Mantido em arquivo separado de `spawn.rs` porque `spawn.rs` faz parte
//! do `runtime_support` (compilado pelo `build.rs` standalone, sem acesso
//! ao `crate::runtime::async_rt`). Este arquivo so' compila no crate
//! principal — o JIT registra o simbolo direto via `add_fn!`.

/// Submete `fn_ptr(arg)` ao runtime tokio compartilhado.
/// Roda em `spawn_blocking` para nao travar o reactor com codigo JIT
/// sincrono. Vantagem sobre `std::thread::spawn`: nao cria OS thread
/// nova — reusa o pool blocking do tokio (cresce sob demanda, default
/// max 512). Para tarefas leves ou que aguardem I/O, escala melhor que
/// `spawn`/`spawn_detached`.
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
