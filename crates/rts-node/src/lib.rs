//! `rts-node` — the native Node.js API surface for RTS.
//!
//! Independent crate: it owns its OWN native Rust implementations and **never
//! mirrors `rts-std` externs**. Symbols use the `__RTS_FN_NODE_<MOD>_<NAME>`
//! convention (its own symbol space, distinct from rts-std's `__RTS_FN_NS_*`).
//!
//! Each module exposes a `register(&mut rts_engine::Engine)` that publishes its
//! surface into the codegen Registry (reached through the `rts-runtime` facade,
//! wired in `registry_build.rs`). Modules name themselves as DATA via
//! `e.module("node", "<mod>").alias("<mod>")`, so a `node:<mod>` import resolves
//! with no `node`→`rts` special-case in codegen. See `docs/node-implementation/`.
//!
//! HONESTY FLOOR: only REAL implementations land here — no stubs, no mock/fixed
//! placeholder values. A member exists only when it computes the genuine answer
//! (a real syscall, algorithm, clock, or a real constant value). Modules that
//! would need a runtime that does not exist yet (worker_threads, cluster,
//! async_hooks, inspector, diagnostics_channel, v8 heap stats) are NOT added
//! until they can be backed for real.

pub mod module;
pub mod punycode;
pub mod querystring;
pub mod tty;
