//! `Math`, as instructions.
//!
//! # Why a module of its own, and not a corner of `call.rs`
//!
//! `Math.sqrt`, `floor`, `ceil`, `trunc`, `abs`, `min` and `max` are the seven
//! members a program reaches by name that the hardware answers in ONE
//! instruction, and each was a property read through the chain cache plus a
//! dispatch — 48 ns for a square root and 131 ns for a minimum, against one or
//! two for the instruction (`bench/analytic.ts`, 2026-08 and 2026-09-26).
//!
//! The rule this module carries, and the reason it is separate from the call
//! emitter: **anything low-level a program reaches by a well-known name is
//! decided in this crate as an instruction, not left to the runtime as a call.**
//! `call.rs` is the generic path and was accumulating the exceptions to itself;
//! the next member (`Math.hypot`, `Math.imul`, `Math.fround`, `Math.clz32` all
//! measure between 40 and 140 ns today) lands here beside its siblings, where the
//! proof that admits it is stated once.
//!
//! What admits a member is in [`emit`]'s documentation, and it is a proof rather
//! than a guess: the program leaves `Math` untouched, nothing in scope shadows
//! the name, and every operand is already a proven double. The machine knows
//! none of this — `NumOp::Min` says nothing about `Math` — which is rule 2 of
//! its README and why the decision is taken here.

mod body;

pub(super) use body::emit;
