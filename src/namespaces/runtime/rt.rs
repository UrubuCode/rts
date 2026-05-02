//! Re-exports for `runtime_support.a` staticlib (via `rt_all.rs`).
//! Only `eval.rs` — subprocess-based impls that have no dependency on the
//! main `rts` crate. `eval_jit.rs` is excluded here intentionally.

pub mod eval;

// Shim para `crate::runtime::async_rt::rt()` — http_server/ops.rs usa este
// caminho. Usa `crate::gc` que e' o path correto no contexto de rt_all.rs.
pub mod async_rt {
    use std::sync::OnceLock;
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
                    crate::gc::thread_registry::register_current();
                })
                .on_thread_stop(|| {
                    crate::gc::thread_registry::unregister_current();
                })
                .build()
                .expect("failed to build shared tokio runtime")
        })
    }
}
