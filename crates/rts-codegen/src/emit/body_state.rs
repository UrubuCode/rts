//! What one function body knows about throws, and why the two facts are one
//! field.
//!
//! # The hazard this type exists to make unrepresentable
//!
//! Both things here name something that belongs to **one** `FuncBuilder`: an
//! SSA value and a block. A nested function is emitted in the middle of an outer
//! one, with its own builder, so either of them read across that boundary names
//! something from a function the emitter is not in.
//!
//! Neither failure is caught by a type. The value one has already happened and
//! is recorded at `emit/wrap.rs`: a wrapper read the enclosing body's throw flag
//! and got a `FuncAddr` instead, so **every generator in the suite loaded the
//! callee's address as if it were the flag**. The block one is sharper and is
//! recorded at `emit/function.rs` for `finally_jumps` — the builder panics with
//! "block belongs to this function", reached by `try { return (() => 2)(); }
//! finally { … }`, which is ordinary code.
//!
//! They were two `Ctx` fields, and only one of them was ever saved. Keeping them
//! apart meant keeping four save-and-restore sites in step by hand, which is a
//! discipline that had already failed twice; one field is what stops a fifth
//! site from saving one and forgetting the other, because
//! [`BodyState::enter_nested`] takes all of it or none of it.
//!
//! Anything else this crate learns about a body — a value, a block, a cache
//! whose key is either — belongs here for the same reason, and gets the scoping
//! for free rather than getting its own site to forget.

use rts_cranelift::ir::ValueId;

/// The facts that belong to the body currently being emitted.
#[derive(Default)]
pub(crate) struct BodyState {
    /// The captured binding written last, and what was written into it.
    ///
    /// # This was a `Ctx` field, and it was the third instance of the bug above
    ///
    /// It holds a `ValueId` **and** a `BlockId`, and it was never saved or
    /// restored around a nested function — so a `s = s + x` inside an arrow left
    /// a memo naming the arrow's block and the arrow's value, and emission
    /// carried on in the enclosing body still holding it. Its guard is "same
    /// block, nothing emitted here", and `BlockId`s are per function, so a
    /// collision is not exotic: it is one number matching another.
    ///
    /// What comes out is a read answering a value from a function it is not in.
    /// The symptom is `Place(Lower(CannotWiden { from: I64 }))` — the same one
    /// `emit/binding.rs` already records for a different mistake with this
    /// field, and the reason it is recognisable is that the value it lands on is
    /// usually a raw integer where a JavaScript value was required.
    ///
    /// Found 2026-08-22, and not by looking for it: sharing the re-raise block
    /// shifted block numbering, which moved WHICH programs collide.
    /// `['a','b'].forEach(x => { s = s + x; … })` inside a `try` started
    /// failing while two programs that failed at `HEAD` started working. None
    /// of those three was evidence about the change; all three were this.
    ///
    /// Read [`super::CapturedWrite`] for what the memo is for and why its
    /// window is as narrow as it is. The window was never the problem — the
    /// window is per block, and nothing said which function's blocks.
    pub(super) last_captured_write: Option<super::CapturedWrite>,
    /// Where this body's throw flag lives, asked once at its entry.
    ///
    /// `None` means the check falls back to calling `__rts_thrown`, which is
    /// what a body with no entry block of its own gets. See
    /// `expr::raise_if_thrown`.
    pub(super) flag: Option<ValueId>,
}

impl BodyState {
    /// Takes both facts away for the duration of a nested function, answering
    /// what to hand back to [`BodyState::leave_nested`].
    ///
    /// Taken rather than replaced, and both rather than either: a nested body
    /// defines its own flag at its own entry and builds its own re-raise blocks,
    /// and reading either of the outer body's names something from another
    /// function. See this module's header for what each of those did the last
    /// time it happened.
    pub(super) fn enter_nested(&mut self) -> BodyState {
        std::mem::take(self)
    }

    /// Puts back what [`BodyState::enter_nested`] took.
    pub(super) fn leave_nested(&mut self, outer: BodyState) {
        *self = outer;
    }
}

