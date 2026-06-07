//! Standalone crate root for the AOT runtime-support archive.
//!
//! The root crate's `build.rs` compiles THIS file as a separate `staticlib`
//! (`crate-name = rts_rt`, `panic=abort`, `--cfg rt_all_archive`) to produce
//! `runtime_support.a`, which the AOT linker pulls in for the `__RTS_*`
//! `extern "C"` runtime symbols.
//!
//! It mirrors `rts-runtime`'s `lib.rs` exactly so every `crate::*` path inside
//! the namespace implementations resolves identically whether the file is
//! compiled as part of the `rts-runtime` rlib or here in the archive crate.
//! (Future: this archive becomes its own `rts-namespaces` crate; for now the
//! mirror keeps a single source of truth under `crates/rts-runtime`.)

pub mod abi {
    pub use rts_abi::*;
}

#[path = "../runtime/mod.rs"]
pub mod runtime;

#[path = "mod.rs"]
pub mod namespaces;
