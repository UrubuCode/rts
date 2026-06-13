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
//! Increment 1 (this commit): [`value`] is real and exhaustively tested (pure
//! model + Cranelift JIT roundtrip). Everything else is a documented skeleton
//! with `todo!()` bodies — buildable, not yet wired into the pipeline.

// Scaffold phase: stub modules intentionally carry unused items until their
// implementation lands. Removed module-by-module as each pillar is built out.
#![allow(dead_code)]

pub mod value;
pub mod repr;
pub mod shape;
pub mod ic;
pub mod dispatch;
pub mod abi_gen;
pub mod lower;
pub mod pipeline;
