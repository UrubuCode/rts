//! Which entry points can leave a throw behind, and which provably cannot.
//!
//! # What this decides
//!
//! A throw does not unwind here. It is **recorded** in a slot the runtime owns,
//! the throwing function returns normally, and the caller asks whether the slot
//! filled — `rts-core`'s `entry/throw.rs` states the whole design and why it is
//! a load and a branch rather than an exception table and a personality routine.
//!
//! The consequence is that `emit/expr.rs` puts that ask after **every** call it
//! makes. This module is the exception list: the operations that cannot fill the
//! slot, so the ask is dead code at their sites.
//!
//! # Why it is a list and not an inference
//!
//! There is no way to derive it here. This crate depends on `rts-cranelift` and
//! SWC and nothing else (rule 1 of the crate README), so it cannot see a single
//! line of `rts-core` — and even if it could, "can this Rust function reach
//! `set_thrown`" is a whole-program call-graph question over a crate that calls
//! back into compiled code.
//!
//! So it is stated, verified by reading, and **asserted against the runtime in
//! `rts-host`**, which is the one crate that may name both (`entries.rs`, and
//! rule 2 of that crate's README: make the agreements between the three
//! explicit).
//!
//! # The cost of being wrong, in the two directions
//!
//! They are not symmetric, and that decides the default.
//!
//! A false `true` — claiming an operation can raise when it cannot — costs a
//! load, a compare, a branch and two basic blocks that never execute. It is
//! measurable and it is correct.
//!
//! A false `false` costs a **swallowed throw**: a `catch` that never runs, a
//! program that carries on with a wrong value instead of failing. That is the
//! hardest class of bug this engine can ship, so [`CANNOT_RAISE`] holds only
//! operations whose runtime body was read in full and found closed — every
//! callee in it either pure computation or a `Context` field access.
//!
//! Machine layer rule 12, the same shape: unproven behaviour fails safely, and
//! the conservative form is the default. Absence from this list is the default.
//!
//! # What it is worth
//!
//! Two claims, and they are different sizes.
//!
//! At the **site**, between 0.1 and 0.6 ns per removed check — `emit/expr.rs`'s
//! own measurement of what moving the flag to a load collected, which is the
//! part that is left once the call is gone. Small, and that is the claim the
//! plan document ranked this by.
//!
//! At **compile time** it is larger and nobody had counted it. Measured
//! 2026-08-22 over `bench/analytic.ts` (`rts ir`, 26 839 lines): the check
//! accounts for **1 423 of 6 164 basic blocks directly and roughly 46% of them
//! once the continuation block each one splits off is counted**, 1 423 of 7 484
//! calls, and about 32% of every instruction emitted. `RTS_TIMING=1` on the same
//! file puts 135.8 ms of a 50.4 ms placement phase in the machine compiler
//! against 19.7 ms in this layer's lowering — so what the code generator is
//! handed is what compilation costs, and blocks are the unit it is handed.
//!
//! This list does not collect most of that; the operations on it are not the
//! common ones. It is the mechanism the collection would use.

use super::RuntimeOp;

/// Every operation whose runtime body provably cannot record a throw.
///
/// Each entry names the `rts-core` function it was verified against. "Closed"
/// below means the body was read in full and every call in it is either pure
/// computation or a `Context` field access — nothing that can reach
/// `entry::throw`, `entry::throw_value` or `entry::current::set_thrown`, and
/// nothing that can call back into user code.
///
/// Verified 2026-08-22. Re-verify an entry when its runtime body changes; the
/// assertion in `rts-host` checks the symbol exists, which is what keeps a
/// renamed operation from silently keeping an exemption, but it cannot read a
/// body.
pub const CANNOT_RAISE: &[RuntimeOp] = &[
    // `left % right` on two unboxed doubles. `entry/operators.rs`, whose own
    // documentation already claimed this exemption — "because it cannot run
    // user code and therefore cannot throw — none of the `__rts_take_thrown`
    // check that follows every generic operator" — while the emitter emitted it
    // anyway. Closed.
    RuntimeOp::NumberRemainder,
    // A thread-local xorshift and no context borrow. `entry/math.rs`, `draw`.
    // Closed.
    RuntimeOp::MathRandom,
    // `context.literals.get(which)`, or `undefined` when the table is short.
    // `entry/text.rs`. Closed — and this is the one that removes a check from a
    // string LITERAL in expression position, which is why the strings
    // investigation reached it independently.
    RuntimeOp::StringConst,
    // `context.callees.last()`, or `undefined`. `entry/function_proto.rs`.
    // Closed.
    RuntimeOp::RunningFunction,
    // Writes `context.pending_call_name` from the literal table. `entry/
    // functions.rs`. Closed.
    RuntimeOp::SetCallName,
    // `context.derived.set(cell, true)`. `entry/functions.rs`. Closed.
    RuntimeOp::MarkDerived,
    // `context.class_constructors.set(cell, true)`. `entry/functions.rs`.
    // Closed.
    RuntimeOp::MarkClassConstructor,
    // `context.elements_at(cell)`, answering the base address or 0. `entry/
    // array.rs`. Closed — the bounds question belongs to the caller, and
    // `array.rs:651` records that the bound is established before the address
    // is used.
    RuntimeOp::ElementsBase,
];

/// The two operations that ARE the check, which are exempt for a different
/// reason and must not be confused with [`CANNOT_RAISE`].
///
/// `Thrown` reads the slot and `TakeThrown` empties it. Checking after either
/// would ask the question the call was asking, forever. They are excluded
/// because including them **recurses**, not because their bodies are closed —
/// `TakeThrown`'s is not, it clears a slot that was filled by a throw.
///
/// Kept apart so that a future audit of [`CANNOT_RAISE`] cannot conclude
/// anything about these two from their presence in one list.
pub const IS_THE_CHECK: &[RuntimeOp] = &[RuntimeOp::Thrown, RuntimeOp::TakeThrown];

impl RuntimeOp {
    /// Whether a call to this operation may leave a throw for the caller to
    /// find.
    ///
    /// `true` is the conservative answer and the default for anything not
    /// listed — see the module documentation for why the two directions of
    /// error are not the same size.
    ///
    /// This does **not** answer for [`IS_THE_CHECK`]: those two are `true` here,
    /// because a throw genuinely can be in flight when they are called. What is
    /// wrong at their sites is asking again, which is a different question and
    /// `emit::expr::check_for_throw` answers it separately.
    pub fn can_raise(self) -> bool {
        !CANNOT_RAISE.contains(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_operation_that_coerces_its_operands_can_always_raise() {
        // Every one of these routes through `ToPrimitive`, which runs a user
        // `valueOf` or `toString`. Exempting one swallows what that throws.
        for op in [
            RuntimeOp::Add,
            RuntimeOp::Subtract,
            RuntimeOp::Remainder,
            RuntimeOp::Less,
            RuntimeOp::LooseEquals,
            RuntimeOp::GetProperty,
            RuntimeOp::SetProperty,
            RuntimeOp::Call,
            RuntimeOp::KeyNumber,
        ] {
            assert!(
                op.can_raise(),
                "{op:?} reaches user code and must keep its throw check"
            );
        }
    }

    #[test]
    fn the_exempt_set_is_exactly_what_was_verified() {
        // Pinned as a count so that adding one without reading its runtime body
        // fails here rather than silently in a program that catches nothing.
        assert_eq!(
            CANNOT_RAISE.len(),
            8,
            "CANNOT_RAISE changed — each entry must name the rts-core body it \
             was read against, and rts-host asserts the symbol still exists"
        );
    }

    #[test]
    fn the_check_itself_is_not_in_the_exempt_set() {
        for op in IS_THE_CHECK {
            assert!(
                op.can_raise(),
                "{op:?} is exempt from asking, not from raising — see the module doc"
            );
        }
    }
}
