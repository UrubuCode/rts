//! What an operation does besides answer.
//!
//! Every pass that moves, merges or removes an instruction reads this and nothing
//! else. It is the machinery `emit/` never had: today "may this be hoisted" is
//! decided per construct at the site that emits it, which is why the answer is
//! re-derived — and occasionally derived differently — in several files.
//!
//! # Why a set of flags rather than a lattice of levels
//!
//! A level implies an order, and these do not have one. Allocating and throwing
//! are unordered: an operation that allocates may be reordered past one that
//! throws when nothing observes the order, and one that throws may not be moved
//! past a *write* even though writing is "less" than throwing on any scale one
//! might invent. Flags say exactly what is true and leave the ordering rules to
//! the pass, which is where the question actually is.
//!
//! # Absence is a claim, and rule 5 is about this
//!
//! `PURE` says the runtime's implementation reads nothing, writes nothing,
//! allocates nothing, calls nothing and cannot fail. A primitive registered
//! wrongly does not produce an error — it produces a silently wrong program,
//! because a pure-marked operation that allocates will be hoisted out of the loop
//! whose result was keeping it alive. The language that declares the table owes a
//! test that its implementation still matches.

/// What an operation does besides answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub struct Effect(u8);

impl Effect {
    /// Reads nothing, writes nothing, allocates nothing, calls nothing, cannot
    /// fail. The only summary under which an operation may be moved freely.
    pub const PURE: Self = Self(0);
    /// Reads the heap, so it may not be moved across a write to it.
    pub const READS: Self = Self(1 << 0);
    /// Writes the heap.
    pub const WRITES: Self = Self(1 << 1);
    /// Allocates, so it may be a collection point.
    pub const ALLOCATES: Self = Self(1 << 2);
    /// Calls code the language's user wrote, so it does everything a program can.
    pub const CALLS_USER: Self = Self(1 << 3);
    /// May raise, so control does not necessarily reach the next instruction.
    pub const THROWS: Self = Self(1 << 4);
    /// Parks the frame: control leaves here and may come back.
    ///
    /// # Why this is a flag and not a primitive
    ///
    /// Because every consumer has to respect it, and a language table is exactly what a
    /// consumer is allowed not to understand. Nothing may be moved across a suspension,
    /// and everything live across one has to survive a frame that is no longer on the
    /// stack — those are facts about MOTION and about LIVENESS, which is what this set
    /// exists to carry.
    ///
    /// A `yield` and an `await` are the same fact here, and that is the finding rather
    /// than a convenience: `rts_cranelift::frame`'s own header says a generator and an
    /// asynchronous function are the same capability, which is why it owns the frame
    /// transform instead of each of them having one.
    pub const SUSPENDS: Self = Self(1 << 5);

    /// The union: what doing both amounts to.
    pub const fn and(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every flag of `what` is set here.
    pub const fn has(self, what: Self) -> bool {
        self.0 & what.0 == what.0
    }

    /// Whether this is [`Self::PURE`].
    pub const fn is_pure(self) -> bool {
        self.0 == 0
    }

    /// Whether the operation may be a collection point.
    ///
    /// Calling user code is one even without `ALLOCATES`, because what it calls
    /// may allocate. That inference is made here rather than asked of every
    /// caller, which is how a root gets missed.
    pub const fn may_collect(self) -> bool {
        self.has(Self::ALLOCATES) || self.has(Self::CALLS_USER)
    }

    /// Whether control certainly reaches the next instruction.
    pub const fn falls_through(self) -> bool {
        !self.has(Self::THROWS) && !self.has(Self::CALLS_USER) && !self.has(Self::SUSPENDS)
    }

    /// Whether the frame may be parked here.
    ///
    /// Asked apart from [`Self::may_collect`] although a parked frame is also a place a
    /// collection can happen: what a caller does about the two is different. A
    /// collection needs the roots described; a suspension needs the live set to survive
    /// a frame that has left the stack, which is a stronger requirement and a different
    /// mechanism.
    pub const fn may_suspend(self) -> bool {
        self.has(Self::SUSPENDS)
    }

    /// Whether two operations may be swapped, judged from their summaries alone.
    ///
    /// Conservative on purpose and in one direction: it answers `false` wherever
    /// it cannot tell, so a pass that trusts it refuses a legal motion rather
    /// than performing an illegal one.
    pub const fn commutes_with(self, other: Self) -> bool {
        if self.is_pure() && other.is_pure() {
            return true;
        }
        // NOTHING crosses a suspension. Not a read, not an allocation, not another
        // suspension: between the two halves of one, anything at all may run --
        // whoever resumes the frame decides when, and the program keeps going meanwhile.
        if self.has(Self::SUSPENDS) || other.has(Self::SUSPENDS) {
            return false;
        }
        // A write on either side orders everything that touches the heap, and
        // calling user code is a write of unknown extent.
        let self_writes = self.has(Self::WRITES) || self.has(Self::CALLS_USER);
        let other_writes = other.has(Self::WRITES) || other.has(Self::CALLS_USER);
        let self_touches = self_writes || self.has(Self::READS);
        let other_touches = other_writes || other.has(Self::READS);
        if (self_writes && other_touches) || (other_writes && self_touches) {
            return false;
        }
        // Raising orders against anything observable, because which of the two
        // happened is observable when only one of them runs.
        !(self.has(Self::THROWS) && other_touches) && !(other.has(Self::THROWS) && self_touches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_pure_operations_may_be_swapped() {
        assert!(Effect::PURE.commutes_with(Effect::PURE));
    }

    #[test]
    fn a_write_orders_every_read_of_the_heap() {
        assert!(!Effect::WRITES.commutes_with(Effect::READS));
        assert!(!Effect::READS.commutes_with(Effect::WRITES));
        // And a read does not order another read.
        assert!(Effect::READS.commutes_with(Effect::READS));
    }

    #[test]
    fn calling_user_code_is_a_write_of_unknown_extent() {
        assert!(!Effect::CALLS_USER.commutes_with(Effect::READS));
        assert!(!Effect::READS.commutes_with(Effect::CALLS_USER));
    }

    /// The inference rule 5 exists to keep in one place: a call may collect even
    /// when its own summary does not say it allocates.
    #[test]
    fn calling_user_code_may_collect_without_allocating() {
        assert!(Effect::CALLS_USER.may_collect());
        assert!(!Effect::CALLS_USER.has(Effect::ALLOCATES));
        assert!(!Effect::READS.may_collect());
    }

    #[test]
    fn raising_means_control_may_not_reach_the_next_instruction() {
        assert!(Effect::PURE.falls_through());
        assert!(!Effect::THROWS.falls_through());
        assert!(!Effect::CALLS_USER.falls_through());
    }

    #[test]
    fn the_union_of_two_summaries_holds_both() {
        let both = Effect::READS.and(Effect::ALLOCATES);
        assert!(both.has(Effect::READS));
        assert!(both.has(Effect::ALLOCATES));
        assert!(!both.has(Effect::WRITES));
        assert!(!both.is_pure());
    }
}
