//! `rts-node` — the native Node.js API surface for RTS.
//!
//! Independent crate: it owns its OWN native Rust implementations and **never
//! mirrors `rts-std` externs**. Symbols use the `__RTS_FN_NODE_<MOD>_<NAME>`
//! convention (its own symbol space, distinct from rts-std's `__RTS_FN_NS_*`).
//!
//! Each module exposes a `register(&mut rts_engine::Engine)` that publishes its
//! surface into the codegen Registry (reached through the `rts-runtime` facade,
//! wired in `registry_build.rs`). Modules name themselves as DATA via
//! `e.module("node", "<mod>")`; scheme-aware resolution routes a `node:<mod>`
//! import to that canonical key, so rts-node OWNS `node:<mod>` distinct from any
//! `rts:<mod>` — no `node`→`rts` special-case in codegen.
//!
//! HONESTY FLOOR: only REAL implementations land here — no stubs, no mock/fixed
//! placeholder values. A member exists only when it computes the genuine,
//! Node-accurate answer (a real syscall, algorithm, or clock). Members that need
//! objects/arrays/streams/async/a runtime that does not exist yet are deferred
//! (documented per module), never faked.

pub mod os;
pub mod path;
pub mod punycode;
pub mod querystring;
pub mod string_decoder;
pub mod url;
