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
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(workers)
            .enable_all()
            .thread_name("rts-tokio")
            .on_thread_start(|| {
                crate::namespaces::gc::thread_registry::register_current();
            })
            .on_thread_stop(|| {
                crate::namespaces::gc::thread_registry::unregister_current();
            })
            .build()
            .expect("failed to build shared tokio runtime")
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
