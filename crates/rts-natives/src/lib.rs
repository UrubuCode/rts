//! `rts-natives` — **how RTS works inside**: the extent of Cranelift.
//!
//! The partition rule (owner directive, 2026-07-31) is one sentence:
//!
//! > **`rts-engine` manages. `rts-natives` is how it works inside.**
//!
//! An item belongs here when EITHER clause holds:
//!
//! - **(a) the Cranelift IR cannot express it**, so generated code has to call
//!   out: coroutines, exceptions, a garbage collector, a mutable shared cell, a
//!   trace stack;
//! - **(b) it IS the runtime value representation, or it has to know that
//!   representation from the inside**: the heap, [`heap::handles::Entry`], the
//!   HandleTable, hidden classes ([`heap::shapes`]), the NaN-box
//!   ([`heap::poly`]) — and anything that pattern-matches `Entry` exhaustively
//!   (which is why [`heap::pickle`] is here and not in a layer above).
//!
//! It belongs in `rts-engine` instead when it **decides dispatch**: the
//! Registry, the builder, the member/spec vocabulary.
//!
//! Clause (b) is what places the heap, the shapes and the NaN-box here. Forcing
//! them under clause (a) would be a stretch — the IR can express a struct load;
//! what it cannot express is *which* struct.
//!
//! ## Position in the graph
//!
//! ```text
//! rts-abi        the CONTRACT (AbiType / SymbolDesc / the naming rule)
//!    ▲
//! rts-natives    HOW IT WORKS INSIDE  ← this crate; every `__rtsn_` lives here
//!    ▲
//! rts-engine     MANAGES — Registry + builder + member + sig
//!    ▲
//! rts-primitives → rts-shared → rts-std (the real backend: io/net/tokio)
//!    ▲
//! rts-runtime    facade + adapters (value model, `__rtsadp_*`)
//! ```
//!
//! `rts-engine` re-exports [`heap`], [`collector`] and [`numfmt`] verbatim, so
//! every existing `rts_engine::heap::…` path above keeps resolving unchanged.
//!
//! Plan of record: `RTS_ORGANIZATION.md`.

// The `#[rtse::*]` macros emit absolute `::rts_engine::*` paths (the authoring
// surface predates this crate). Aliasing ourselves to that name makes those
// paths resolve HERE — to `abi` and `heap` below — without a dependency edge
// back to `rts-engine`, which would be a cycle. `rts-engine` does the same
// trick for its own macro users, and `rts-runtime` for `adapters/`.
extern crate self as rts_engine;

pub mod abi;
pub mod collector;
/// The ONE declaration site for symbols whose bodies live ABOVE this crate and
/// are bound by the linker. Replaces the four `gc_surface.rs` files (N2b).
pub mod externs;
pub mod heap;
pub mod numfmt;

pub use collector::{GcPayload, Traceable};
