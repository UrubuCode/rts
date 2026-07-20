//! Engine-direct JIT symbols + the Registry-harvest composition for the
//! `JITBuilder`.
//!
//! [`jit_symbols`] assembles the full JIT symbol table from THREE sources:
//!  1. **Registry harvest** (`front::run::registry::all_jit_symbols`) — every
//!     genuine namespace/class MEMBER that carries a real `fn_ptr` (e.g. the
//!     `string` ns via `fp_for`, io, math, Date/URL class methods). The bulk; it
//!     stays in sync with the Registry automatically, no hand list.
//!  2. **`__rtsadp_*` adapter trampolines** — the engine's OWN codegen-owned
//!     symbols (`crate::value::*`): the generic `+`, shape/IC obj access, function
//!     values, dynamic dispatch. Not runtime, not in any Registry — listed here.
//!  3. **Engine-internal runtime primitives** — `__RTS_FN_NS_GC_*` env / generator
//!     / GEN_SM / string-pool ops the engine emits DIRECTLY as codegen sentinels
//!     (NOT exposed as `rts:gc` members; the `gc` ns deliberately surfaces only
//!     `collect`/`live_count`). The harvest cannot supply them (no member to hold
//!     the address), so they are listed here too.
//!
//! Sources 2+3 are the irreducible "engine-direct" set — symbols the engine NAMES
//! itself rather than resolving via the Registry. Source 1 used to be hand-listed
//! here too; those entries drain out as each namespace's `register` is converted
//! to carry real `fn_ptr`s (the `dataview`/`string` `fp_for` pattern).
//!
//! This replaces the fake `crate::runtime::symbols()` table. It is the new
//! engine's analogue of the old engine's `register_runtime_symbols` /
//! `runtime_jit_symbols` bridge (`rts-codegen-old/src/abi/mod.rs` +
//! `codegen/jit.rs`) — but built against the `rts-runtime` FACADE, NOT by
//! depending on the frozen old crate.
//!
//! ## Why sources 2+3 are listed by hand (the harvest cannot supply them)
//!
//! The harvest (`registry().jit_symbols()`) only yields members with a real,
//! non-null `fn_ptr` (the **null-skip** invariant: alias/external members carry a
//! null fn_ptr). The engine-internal `__RTS_FN_NS_GC_*` primitives (string-pool /
//! env / generator / GEN_SM / gcell / poly-bridge) and a few directly-emitted
//! class methods (`__RTS_FN_GL_STRING_*` slice/substr/codePointAt/localeCompare,
//! emitted by `try_string_special`) are NOT registered as harvestable members —
//! the engine NAMES them itself in its lowering — so there is no member to attach
//! an address to. We take each one's address directly through the facade re-export
//! and list it here. A genuine namespace/class MEMBER, by contrast, carries its
//! real address at registration (`fp_for`) and is installed by the harvest with no
//! hand entry (string ns / Date / URL already converted).

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

/// The full JIT symbol table: the engine-direct lists (`__rtsadp_*` adapters +
/// the Cat-3 internal `__RTS_FN_NS_GC_*` primitives) split across `list_a`/`list_b`
/// for the <500-line module rule, plus (inside `list_b`) the Registry harvest.
pub fn jit_symbols() -> Vec<JitSymbol> {
    let mut syms = list_a::symbols();
    syms.extend(list_b::symbols());
    syms
}

mod list_a;
mod list_b;

#[cfg(test)]
mod tests {
    use super::jit_symbols;
    use std::collections::HashMap;

    /// Drift/sanity guard for the JIT symbol table. NOTE: this checks the INSTALLED
    /// set, not full emittable-symbol coverage — it cannot catch a symbol the
    /// lowering can emit that is neither harvested nor hand-listed (that surfaces
    /// only at runtime / in the TS suite as a "can't resolve symbol"). What it DOES
    /// guarantee: every installed symbol has a REAL address (no null slipped through
    /// from an `external` member whose `fp_for` forgot it — the link-OK/runtime-
    /// SIGILL class), and no name is installed with TWO different addresses (a
    /// harvest entry disagreeing with a hand entry; same-address dupes are harmless).
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
                    "JIT symbol `{}` installed with two different addresses",
                    s.name
                ),
                None => {
                    seen.insert(s.name, s.ptr);
                }
            }
        }
    }
}
