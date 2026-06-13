//! Auto-generated JIT symbol table — kills the 1113 hand-written `add_fn!`.
//!
//! In the old engine, `jit.rs` registered ~1113 runtime symbols by hand; a
//! rename produced a link-OK binary that SIGILL'd at runtime on an ABI mismatch,
//! with no build-time check. Here the symbol table is DERIVED from the same
//! `SPECS` metadata codegen reads, and a build-time assertion verifies every
//! codegen-referenced symbol exists with a matching lowered signature.

/// One registrable runtime symbol derived from a spec member.
pub struct SymbolEntry {
    pub name: &'static str,
    pub ptr: *const u8,
}

/// Build the full JIT symbol set from the ABI specs (no manual list).
pub fn jit_symbols() -> Vec<SymbolEntry> {
    todo!("phase: abi — iterate SPECS, emit (symbol, fn_ptr) pairs; assert coverage")
}
