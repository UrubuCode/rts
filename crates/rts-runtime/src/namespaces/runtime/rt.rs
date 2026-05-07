//! Re-exports for `runtime_support.a` staticlib (via `rt_all.rs`).
//! Only `eval.rs` — subprocess-based impls that have no dependency on the
//! main `rts` crate. `eval_jit.rs` is excluded here intentionally.

pub mod eval;

// Shim para `crate::runtime::async_rt::rt()` — http_server/ops.rs usa este
// caminho. Diferente do canonico em `runtime/async_rt.rs` apenas no path do
// `gc::thread_registry`: aqui `crate::gc::*` (rt_all.rs reorganiza modulos),
// la' `crate::namespaces::gc::*`. A logica do builder eh compartilhada via
// `crate::runtime::async_rt::build_shared_runtime`.
pub mod async_rt {
    use std::sync::OnceLock;
    pub fn rt() -> &'static tokio::runtime::Runtime {
        static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        RT.get_or_init(|| {
            crate::runtime::async_rt::build_shared_runtime(
                || crate::gc::thread_registry::register_current(),
                || crate::gc::thread_registry::unregister_current(),
            )
        })
    }
}
