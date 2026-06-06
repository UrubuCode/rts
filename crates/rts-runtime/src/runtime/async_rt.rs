//! Runtime tokio compartilhado para todas as features async do RTS.
//!
//! Issue #399 — antes deste modulo, cada feature async (atualmente
//! apenas `http_server`) criava seu proprio runtime. Multiplos runtimes
//! competem por threads, complicam reentrancia (`Cannot start a runtime
//! from within a runtime`), e fogem do `gc/thread_registry`.
//!
//! Aqui temos:
//! - 1 `Runtime` global, lazy-init via `OnceLock`
//! - `on_thread_start` / `on_thread_stop` registram cada worker tokio
//!   no `gc::thread_registry` para que o GC scanner enxergue handles
//!   vivos em tasks tokio (sem isto, sweep coleta indevidamente sob
//!   carga concorrente)
//!
//! Convencao de uso:
//! - Features async chamam `rt().block_on(async { ... })` quando precisam
//!   bloquear ate completar (ex: `serve()` que nao retorna)
//! - Para spawnar task fire-and-forget: `rt().spawn(async { ... })`
//! - Para chamar de dentro de uma fn async: `rt().handle().clone()` se
//!   precisar de Handle, ou apenas use as APIs do tokio

use std::sync::OnceLock;

/// Constroi um `tokio::runtime::Runtime` multi-thread no padrao do RTS:
/// worker count = `available_parallelism()` (fallback 4), `enable_all`
/// I/O+timer, thread name `"rts-tokio"`, hooks de start/stop chamando
/// as closures dadas.
///
/// Existe pra evitar copy-paste do builder entre `runtime/async_rt`
/// (crate principal) e `namespaces/runtime/rt` (shim do `rt_all.rs`
/// staticlib) — os dois contextos compartilham a mesma logica de
/// builder mas resolvem o caminho do `thread_registry` por modulos
/// diferentes (`crate::namespaces::gc::*` vs `crate::gc::*`).
pub fn build_shared_runtime<S, E>(on_thread_start: S, on_thread_stop: E) -> tokio::runtime::Runtime
where
    S: Fn() + Send + Sync + 'static,
    E: Fn() + Send + Sync + 'static,
{
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .thread_name("rts-tokio")
        .on_thread_start(on_thread_start)
        .on_thread_stop(on_thread_stop)
        .build()
        .expect("failed to build shared tokio runtime")
}

/// Acessa o runtime tokio global. Inicializa na primeira chamada.
///
/// Worker count = `available_parallelism()` (ou 4 como fallback).
/// `enable_all` ativa I/O + timer drivers.
///
/// Hooks `on_thread_start` / `on_thread_stop` registram cada worker no
/// `gc::thread_registry` — sem isso, o GC scanner so' varreria a thread
/// que disparou o tick e handles vivos em tasks tokio seriam swept.
pub fn rt() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        build_shared_runtime(
            || crate::namespaces::gc::thread_registry::register_current(),
            || crate::namespaces::gc::thread_registry::unregister_current(),
        )
    })
}

/// Atalho para `rt().handle()`. Tokio `Handle` pode ser clonado
/// livremente entre threads, ao contrario do `Runtime` que e' singleton.
pub fn handle() -> tokio::runtime::Handle {
    rt().handle().clone()
}

/// `true` se a thread atual ja' esta dentro do runtime tokio. Util pra
/// evitar o panic "Cannot start a runtime from within a runtime" — em
/// vez de `rt().block_on(...)` use `tokio::task::block_in_place` ou
/// despache pra outro Handle.
pub fn in_tokio_thread() -> bool {
    tokio::runtime::Handle::try_current().is_ok()
}

thread_local! {
    /// (cross-runtime #365) Marca quando a thread atual esta executando o
    /// corpo de uma async fn (via `promise.create` em `spawn_blocking`).
    /// Threads de `spawn_blocking` NAO tem contexto tokio (`try_current`
    /// retorna Err), entao `in_tokio_thread()` nao basta. `parallel.map`
    /// consulta esta flag pra rodar sequencial em vez de instalar no pool
    /// rayon — chamar o fn_ptr JIT em workers rayon nao-registrados na GC
    /// thread_registry crasha (ILLEGAL_INSTRUCTION).
    static IN_ASYNC_WORKER: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// RAII guard: marca a thread como async-worker enquanto vivo (reentrante).
pub struct AsyncWorkerGuard(());

impl AsyncWorkerGuard {
    pub fn enter() -> Self {
        IN_ASYNC_WORKER.with(|c| c.set(c.get() + 1));
        AsyncWorkerGuard(())
    }
}

impl Drop for AsyncWorkerGuard {
    fn drop(&mut self) {
        IN_ASYNC_WORKER.with(|c| c.set(c.get().saturating_sub(1)));
    }
}

/// True se a thread atual executa o corpo de uma async fn.
pub fn in_async_worker() -> bool {
    IN_ASYNC_WORKER.with(|c| c.get() > 0)
}
