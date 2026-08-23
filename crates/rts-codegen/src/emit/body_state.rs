//! What the emitter knows about the body it is in, and why it is one field.
//!
//! # The hazard this type exists to make unrepresentable
//!
//! Everything here names something that belongs to **one** `FuncBuilder`: SSA
//! values and blocks. A nested function is emitted in the middle of an outer
//! one, with its own builder, so any of them read across that boundary names
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

use rts_cranelift::RegionId;
use rts_cranelift::ir::{BlockId, ValueId};

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
    /// The integer zero every throw check compares against, materialized once
    /// at this body's entry.
    ///
    /// # Why it is worth a field
    ///
    /// The check is a load, a compare against zero, and a branch, and it is
    /// emitted after every call that can raise. The zero was declared and
    /// materialized at each of them: **1 066 `Inst::Const` in
    /// `bench/analytic.ts`**, a third of every constant the file emits, all of
    /// them the same number. `ir::Function::push_const` already collapses the
    /// pool to one row, but each site still emits its own instruction — a value
    /// has to dominate its uses, so the pool cannot share the materialization.
    ///
    /// The entry block dominates every block in the function, so one value
    /// there does.
    ///
    /// # Why this costs no more than what is already accepted
    ///
    /// It is one more value live across the whole body, and `flag` — asked at
    /// the same place, for the same checks — already is. Nothing new is being
    /// traded: the same sites read both, so wherever the register allocator
    /// keeps one it can keep the other, and an integer zero is the cheapest
    /// thing it can ever have to spill and reload.
    ///
    /// # Why it is gated on the same condition as `flag`
    ///
    /// `None` for a body that PARKS. `frame::resumable_form` rewrites a
    /// suspending function around every suspension, so a value defined at entry
    /// and read after a `yield` is not the value it was — which cost 37
    /// generator files in one run when `flag` learned it. A constant is no
    /// different from an address here: both are SSA values of the pre-rewrite
    /// function.
    pub(super) zero: Option<ValueId>,
    /// The block that re-raises, once per protected region that asked for one.
    ///
    /// See [`BodyState::reraise_in`] for why one per region rather than one per
    /// body or one per site.
    reraise: Vec<(Option<RegionId>, BlockId)>,
}

impl BodyState {
    /// The re-raise block already built for this region, if there is one.
    ///
    /// # Why one per region and not one per body
    ///
    /// The block's terminator is a `Throw`, and where a throw lands is decided
    /// by the region the throwing block is in — that is the whole reason
    /// `raise_if_thrown` creates its block while the region is open. Two sites
    /// inside the same `try` re-raise into the same handler; a site inside it
    /// and a site outside it do not. So the region is exactly the key, and
    /// `None` — no region open — is a key like any other, because every block
    /// outside every region routes alike.
    ///
    /// A `RegionId` is minted fresh by each `open_region`, so a region that
    /// closes and reopens is a different key and cannot collide with itself.
    ///
    /// # Why sharing is sound at all
    ///
    /// The block takes no parameters and reads nothing from the site that
    /// branches to it: it calls `__rts_take_thrown` and throws what came back.
    /// So there is no value that has to dominate anything, and two sites
    /// branching to one copy cannot disagree about what it computes — which is
    /// what made 1 069 identical copies of it in `bench/analytic.ts` pure
    /// duplication rather than specialisation.
    pub(super) fn reraise_in(&self, region: Option<RegionId>) -> Option<BlockId> {
        self.reraise
            .iter()
            .find(|(held, _)| *held == region)
            .map(|(_, block)| *block)
    }

    /// Records the re-raise block built for a region.
    pub(super) fn remember_reraise(&mut self, region: Option<RegionId>, block: BlockId) {
        self.reraise.push((region, block));
    }

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

