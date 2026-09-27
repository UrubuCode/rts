//! `Math`, as instructions.
//!
//! # Why a module of its own, and not a corner of `call.rs`
//!
//! Every member a program reaches through `Math` is decided HERE, in the crate
//! that knows what the name means, and in one of three shapes:
//!
//! - **an instruction**, where the hardware has one — `sqrt`, `floor`, `ceil`,
//!   `trunc`, `abs`, `fround`, `min`, `max`;
//! - **a sequence** of those, stated once in `sequence.rs` for both emitters —
//!   `round`, `sign`, `imul`, `clz32`, each around the corner the obvious
//!   spelling gets wrong;
//! - **a direct call**, operand and answer unboxed, by a number the runtime's
//!   table is indexed with — the transcendentals, `pow`, `hypot`, `atan2`,
//!   which are library calls on every machine and stop being a property read
//!   plus a dispatch here. `runtime/math_direct.rs` holds the numbering.
//!
//! And the eight constants fold to the number under the same proof.
//!
//! Each was a property read through the chain cache plus a dispatch — 40 ns for
//! `round`, 75 for `sin`, 126 for `hypot`, against 2, 6 and 4.6 after
//! (2026-09-26, release, `bench/analytic.ts`'s shape). The rule this module
//! carries, and the reason it is separate from the call emitter: **anything
//! low-level a program reaches by a well-known name is decided in this crate,
//! not left to the runtime as a call.** `call.rs` is the generic path and was
//! accumulating the exceptions to itself.
//!
//! What admits a member is in [`emit`]'s documentation, and it is a proof rather
//! than a guess: the program leaves `Math` untouched, nothing in scope shadows
//! the name, and every operand is already a proven double. The machine knows
//! none of this — `NumOp::Min` says nothing about `Math` — which is rule 2 of
//! its README and why the decision is taken here.

mod body;
pub(crate) mod sequence;

pub(super) use body::{constant, emit, fixed};
pub(crate) use body::constant_named;
