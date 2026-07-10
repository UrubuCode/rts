//! `rts-node` — the native Node.js API surface for RTS.
//!
//! Independent crate: it owns its OWN native Rust implementations and **never
//! mirrors `rts-std` externs**. Symbols use the `__RTS_FN_NODE_<MOD>_<NAME>`
//! convention (its own symbol space, distinct from rts-std's `__RTS_FN_NS_*`).
//!
//! Each module exposes a `register(&mut rts_engine::Engine)` that publishes its
//! surface into the codegen Registry (reached through the `rts-runtime` facade,
//! wired in `registry_build.rs`). A `node:<mod>` import resolves to it exactly
//! like the `rts:<mod>` namespaces — the engine names no module, resolution is
//! data-driven. See `docs/node-implementation/` for the full plan.
//!
//! This is the rebuilt crate (the previous name→symbol scaffold that borrowed
//! rts-std symbols is deleted). Modules land here incrementally, mature-pure
//! first (`docs/node-implementation/implementation-plan.md`).

pub mod punycode;
pub mod querystring;
