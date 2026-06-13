//! The JIT symbol table the harness installs into the `JITBuilder`.
//!
//! In the old engine, `jit.rs` registered ~1113 runtime symbols by hand; a
//! rename produced a link-OK binary that SIGILL'd at runtime on an ABI mismatch,
//! with no build-time check. Here the table is the REAL runtime surface the new
//! lowering actually calls — sourced from [`crate::runtime_link`], which takes
//! the address of each real `__RTS_FN_*` function through the `rts-runtime`
//! facade plus the codegen-owned `__rtsadp_*` adapter trampolines. The set is
//! small and explicit because the new engine emits a tiny, known surface today;
//! it grows as the lowering grows, never by a hand-maintained parallel list that
//! can drift from the call sites.

pub use crate::runtime_link::JitSymbol as SymbolEntry;

/// The full JIT symbol set: the real runtime symbols + adapter trampolines the
/// new lowering emits. See [`crate::runtime_link::jit_symbols`].
pub fn jit_symbols() -> Vec<SymbolEntry> {
    crate::runtime_link::jit_symbols()
}
