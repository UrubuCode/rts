//! Re-exports for `runtime_support.a` staticlib (via `rt_all.rs`).
//! Only `eval.rs` — subprocess-based impls that have no dependency on the
//! main `rts` crate. `eval_jit.rs` is excluded here intentionally.

pub mod eval;
