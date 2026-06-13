//! # rts-codegen-new — lean native JS/TS engine (redesign)
//!
//! This crate is the ground-up redesign of the RTS codegen. It exists alongside
//! the frozen `rts-codegen-old` during the strangler-fig migration; the bin/cli
//! keep using `rts-codegen-old` until this crate reaches parity and the cutover
//! happens. See `docs/specs/rts-codegen-new-design.md` for the full rationale.
//!
//! ## Why the old engine is being replaced
//!
//! RTS reached 100% cross-runtime parity (372/372, tag `v0.0-202606072107`) on
//! the OLD engine — but did so by grinding hardcoded special-cases into giant
//! files (`calls/mod.rs` ~4.6k LOC) on top of a value model whose own
//! `MAINTENANCE.md` admitted was the wall: a single `i64` ABI slot overloaded to
//! mean `{int, handle, boxed-float, string, undefined/null/bool sentinel}`, with
//! the type "tag" smeared into four `HashSet<ir::Value>` compile-time side-tables
//! plus AST-shape heuristics plus a runtime BOX/UNBOX/EQ/ARITH helper zoo
//! (`Entry::FloatPrim`). That model is **unsound by construction** (a container
//! accessor that forgets to register a value silently mis-coerces) and does not
//! scale (each new container-storable type needs its own helper quadruple).
//!
//! ## The new thesis
//!
//! 1. **One value.** [`value::PolyValue`] is a NaN-boxed 64-bit word: an inline
//!    `f64`, a small `i32`, a singleton (undefined/null/true/false/the-hole), or
//!    a GC **handle slot** (GC-safe because heap refs are already HandleTable
//!    indices, never raw pointers). The tag lives **in the value**, not in a
//!    side-table. `typeof` is a tag inspection; box/unbox are single pure
//!    Cranelift ops (`bitcast`/`band`/`bor`/`icmp`/`select`) the egraph can fold.
//! 2. **Prove-and-unbox.** [`repr`] is a representation lattice. A value is kept
//!    UNBOXED (`raw i64`/`f64` in registers — the existing winning numeric path)
//!    only where the front-end PROVES a monomorphic representation; otherwise it
//!    is a `PolyValue`. box/unbox are **explicit IR nodes** inserted at proven
//!    boundaries — a TOTAL function of the IR, never "tracked elsewhere".
//! 3. **Shapes, not hashmaps.** [`shape`] gives objects hidden classes + inline
//!    slot arrays; [`ic`] gives AOT-safe **data** inline caches (a runtime cache
//!    cell compared by shape-id, no code patching) replacing both the default
//!    `HashMap<String,i64>` property-bag and the O(N) string-compare vtable.
//! 4. **One lowering path.** [`lower`] lowers HIR straight to Cranelift. There is
//!    **no second optimizer tier** (the old `rts-mir` re-did Cranelift's egraph).
//! 5. **No builtins; Registry-driven dispatch.** [`dispatch`] resolves every
//!    non-primordial method through the Registry/`SPECS` metadata via ONE generic
//!    path; [`abi_gen`] derives the JIT symbol table from `SPECS` (killing the
//!    1113 hand-written `add_fn!` and the link-OK/ABI-mismatch SIGILL class).
//!
//! ## Status
//!
//! Increment 1: [`value`] is real and exhaustively tested (pure model + Cranelift
//! JIT roundtrip). Increment 3 ([`front::hir_lower`]): REAL TypeScript runs
//! end-to-end — swc parse → [`rts_hir`] typed HIR → Cranelift JIT → native
//! execution — for the proven-monomorphic NUMERIC subset (number/i32/bool
//! arithmetic, comparisons, `let`/assignment, `if`/`while`/`return`).
//!
//! Increment 4 ([`front::run`]): a REAL `.ts` PROGRAM runs and PRODUCES OUTPUT.
//! Top-level statements + function defs + cross-function calls + the
//! Tagged/string/polymorphic path + `console.log` lower through a whole-module
//! JIT and execute against the REAL runtime ([`front::run::run_source`]). The
//! numeric fast path stays unboxed; a value becomes a [`value::PolyValue`] only
//! at a Tagged boundary (a `console.log` arg, a string/mixed `+`, a Tagged
//! local/call), boxed via the pure `value::emit_*` IR. String BYTES live in the
//! REAL string pool (`__RTS_FN_NS_GC_STRING_*`), reached through the
//! [`value::abi_adapter`] handle indirection table; `console.log` prints via the
//! REAL `__RTS_FN_NS_IO_PRINT`. The generic JS operators are codegen-owned
//! `__rtsadp_*` trampolines (no real symbol exists for tag-dispatched `+`) that
//! call the real pool for the heap parts. The JIT symbol set comes from
//! [`runtime_link`] (the real runtime surface + the adapter trampolines). The
//! first honest measurement on the real cross-runtime fixtures lives in
//! `front::run::fixture_check` (bun-gated, `#[ignore]`d).
//!
//! Increment P3 ([`shape`] + [`front::run::obj`]): OBJECTS and ARRAYS via
//! COMPILE-TIME shapes. [`shape::ShapeTable`] interns object-literal key-sequences
//! to a `ShapeId` (the compile-time key→slot map). An object/array value is a REAL
//! `Entry::Vec` of `PolyValue` words (the inline slot array), boxed as a
//! `TAG_OBJECT` [`value::PolyValue`] — exactly the strings/handles bridge with the
//! object tag. A property read `obj.a` lowers to `VEC_GET(obj, const slot)` (the
//! slot resolved at compile time from the literal's shape), `obj.a = v` to
//! `VEC_SET`; an array index `arr[i]` to `VEC_GET(obj, i)`, `arr[i] = v` to
//! `VEC_SET`, `arr.length` to `VEC_LEN`; a missing static key reads `undefined`;
//! `typeof {}`/`typeof []` is `"object"` (one tag inspection). What BAILS (no
//! guess): a property/index access on an object whose shape is not statically
//! proven (param/return/reassigned — the dynamic inline cache is later), a
//! computed/dynamic object key, adding a NEW key (the transition tree is later),
//! and a console.log / `+` of a WHOLE object/array value (faithful pretty-print /
//! ToPrimitive is later — the runtime's `[object Object]` would diverge from Bun).
//!
//! Anything outside the implemented subset is an EXPLICIT `Unsupported` bail,
//! never a silent miscompile — including the two HIR-ambiguity cases the engine
//! REFUSES rather than guess: equality on operands of differing/unknown kind
//! (swc conflates `==`/`===`) and unary `!`/`+` (swc conflates them). Whole-value
//! object/array printing, string methods, classes, closures, dynamic property
//! ICs, and 128-bit ints are later increments. The remaining pillars are
//! documented skeletons not yet wired into the pipeline.

// Scaffold phase: stub modules intentionally carry unused items until their
// implementation lands. Removed module-by-module as each pillar is built out.
#![allow(dead_code)]

pub mod value;
pub mod repr;
pub mod shape;
pub mod ic;
pub mod dispatch;
pub mod abi_gen;
pub mod runtime_link;
pub mod front;
pub mod lower;
pub mod pipeline;

/// End-to-end proof suite (P1): reproduces the EXACT old-engine value-model
/// failures and shows the new `PolyValue` representation solving each one
/// through real Cranelift JIT execution. See the module for the per-proof
/// citations of the old-engine bug each refutes.
#[cfg(test)]
mod proof_tests;
