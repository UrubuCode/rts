//! The JIT symbol table handed to `JITBuilder` — now TWO generated sources, no
//! hand list.
//!
//! # What this module used to be
//!
//! It assembled the table from THREE sources: the Registry harvest plus two
//! hand-written lists (`list_a.rs` + `list_b.rs`, ~1390 lines of
//! `sym("__RTS_…", addr)`). Its own doc-comment argued the hand lists were
//! irreducible — that the harvest "cannot supply" engine-internal primitives
//! because no Registry member holds their address. That was true of the
//! HARVEST, and false of the problem: a symbol's address does not have to come
//! from a Registry member, it can come from the DECLARATION. Both lists were
//! DELETED (2026-07-28) once the baked table proved it carried all 355 rows.
//!
//! # The two sources now
//!
//! 1. **The BAKED TABLE** (`rts_runtime::symbol_table::symbols()`) —
//!    authoritative, and installed first. `rts-symbol-baker` scans the
//!    `#[rtse::*]` declarations across the runtime crates and emits a static,
//!    strictly name-ordered table; `rts-runtime` compiles it (it is the crate
//!    that links every scanned crate). A symbol is in it because it EXISTS in
//!    the source, not because someone remembered to list it — which is exactly
//!    what the hand lists could not guarantee.
//! 2. **The Registry harvest** (`front::run::registry::all_jit_symbols`) — the
//!    members whose address is registered at RUNTIME via `fp_for` rather than
//!    declared. Also generated (from the Registry, not by hand), so it is not a
//!    drain target; it is the second half of the same picture. It overlaps the
//!    baked table heavily and that is fine: same name, same address.
//!
//! Ordering is deliberate — baked first, harvest second. `JITBuilder::symbol` is
//! last-wins, and the two never disagree on an address (the test below asserts
//! it), so the order is about intent, not correctness: the declaration is the
//! authority, the runtime registration confirms it.
//!
//! See docs/specs/rts-macro-single-source.md.

/// One installable JIT symbol: an extern "C" name and its function pointer. The
/// pointer is to a `#[no_mangle] extern "C"` function with static lifetime.
#[derive(Clone, Copy)]
pub struct JitSymbol {
    pub name: &'static str,
    pub ptr: *const u8,
}

// SAFETY: every `ptr` is to static `extern "C"` code (the runtime binary or this
// crate), never dereferenced as data — sound to share across threads.
unsafe impl Send for JitSymbol {}
unsafe impl Sync for JitSymbol {}

#[inline]
fn sym(name: &'static str, ptr: *const u8) -> JitSymbol {
    JitSymbol { name, ptr }
}

/// The full JIT symbol table: the baked table, then the Registry harvest.
pub fn jit_symbols() -> Vec<JitSymbol> {
    let mut syms: Vec<JitSymbol> = rts_runtime::symbol_table::symbols()
        .into_iter()
        .map(|e| sym(e.name, e.ptr))
        .collect();
    syms.extend(
        crate::front::run::registry::all_jit_symbols()
            .into_iter()
            .map(|(name, ptr)| sym(name, ptr)),
    );
    syms
}

/// The names the BAKED table provides. Exposed so a drain guard elsewhere can
/// assert that nothing re-grows a hand-written mirror of it.
pub fn baked_names() -> std::collections::HashSet<&'static str> {
    rts_runtime::symbol_table::symbols()
        .into_iter()
        .map(|e| e.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{baked_names, jit_symbols};
    use std::collections::HashMap;

    /// Drift/sanity guard for the JIT symbol table. NOTE: this checks the INSTALLED
    /// set, not full emittable-symbol coverage — it cannot catch a symbol the
    /// lowering can emit that neither source provides (that surfaces at runtime as
    /// an unresolved symbol). What it DOES guarantee: every installed symbol has a
    /// REAL address (no null slipped through from an `external` member whose
    /// `fp_for` forgot it — the link-OK/runtime-SIGILL class), and no name is
    /// installed with TWO different addresses (the baked table and the harvest
    /// disagreeing; same-address dupes are expected and harmless).
    #[test]
    fn jit_symbols_have_no_null_and_no_conflicting_dupes() {
        let syms = jit_symbols();
        assert!(!syms.is_empty(), "the JIT symbol table is empty");
        let mut seen: HashMap<&str, *const u8> = HashMap::new();
        for s in &syms {
            assert!(
                !s.ptr.is_null(),
                "JIT symbol `{}` has a NULL fn_ptr",
                s.name
            );
            match seen.get(s.name) {
                Some(&prev) => assert_eq!(
                    prev, s.ptr,
                    "JIT symbol `{}` installed with two different addresses \
                     (the baked table and the Registry harvest disagree)",
                    s.name
                ),
                None => {
                    seen.insert(s.name, s.ptr);
                }
            }
        }
    }

    /// THE DRAIN GUARD (F10, holding at zero). The hand lists are gone; this keeps
    /// them gone. If a new `sym("…", addr)` list ever reappears in this module,
    /// wire it in here so the assertion catches the duplication — a hand row is
    /// redundant the moment the baked table carries the same name, and only the
    /// hand row can drift from the definition it mirrors.
    #[test]
    fn no_hand_written_symbol_list_exists() {
        let baked = baked_names();
        assert!(
            !baked.is_empty(),
            "the baked symbol table is empty — `rts_runtime::symbol_table` did not \
             compile in, or the baker emitted nothing. Run `cargo run -p rts-symbol-baker`."
        );
        // The whole installed set must come from the two GENERATED sources.
        let harvest: std::collections::HashSet<&str> =
            crate::front::run::registry::all_jit_symbols()
                .into_iter()
                .map(|(n, _)| n)
                .collect();
        let unaccounted: Vec<&str> = jit_symbols()
            .into_iter()
            .map(|s| s.name)
            .filter(|n| !baked.contains(n) && !harvest.contains(n))
            .collect();
        assert!(
            unaccounted.is_empty(),
            "{} installed JIT symbol(s) come from neither the baked table nor the \
             Registry harvest — i.e. a hand list came back:\n{}",
            unaccounted.len(),
            unaccounted.join("\n")
        );
    }

    /// Reports the drain state (`--nocapture`): how the installed symbols split
    /// across the two generated sources. Measurement, not assertion.
    #[test]
    fn drain_report() {
        let baked = baked_names();
        let harvest: std::collections::HashSet<&str> =
            crate::front::run::registry::all_jit_symbols()
                .into_iter()
                .map(|(n, _)| n)
                .collect();
        let installed = jit_symbols().len();
        eprintln!(
            "installed={installed} baked={} harvest={} hand=0",
            baked.len(),
            harvest.len(),
        );
    }
}
