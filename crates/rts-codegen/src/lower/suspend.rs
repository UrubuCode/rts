//! `yield` and `await`, which are one thing here.
//!
//! # Why they are the same instruction
//!
//! Because the fact the graph has to carry about both is the same fact: the frame is
//! parked, something outside decides when it comes back, and what comes back is a value
//! this function did not compute. `rts_cranelift::frame`'s own header says it first — a
//! generator and an asynchronous function are one machine capability, and owning it
//! there is *"what makes those the same feature rather than three implementations of
//! varying honesty"*.
//!
//! So neither is a primitive of this language. `Op::Suspend` is neutral, and what the
//! two differ about is not in the body at all:
//!
//! - **Who resumes.** A generator is resumed by its consumer calling `next`; an
//!   asynchronous function by the microtask queue. That is the scheduler's choice, and
//!   `rts_cranelift::sched` is where a `Delivery` is decided.
//! - **What a call to one does.** Calling either runs no body — one answers a generator
//!   object, the other a promise. That is the CALLER's sequence and it follows from
//!   `Func::may_suspend`, which is why the flag is on the function: rule 2 says a
//!   lowering does not decide how one reaches a callee.
//!
//! Neither of those is asked here, and neither of them is guessed.
//!
//! # What the effect claims, and why each flag is there rather than conservative
//!
//! `SUSPENDS`, `THROWS` and `ALLOCATES`, and the middle one is the one worth stating:
//! `gen.throw(e)` and a rejected promise both resume the frame BY RAISING at exactly
//! this point, so control genuinely may not reach the next instruction. A suspension
//! marked as falling through would let a pass place an instruction after it that the
//! rejection path never runs.
//!
//! `ALLOCATES` because handing the value out builds something — a result object for a
//! generator, a reaction record for an await. Where is the resumer's side and not this
//! one's, which is why the flag is set and no allocation is emitted.
//!
//! # What `await` is NOT lowered to, and this is the reuse-check finding
//!
//! `rts-core` already has the entry point a compiled `await` calls:
//! `entry::promise::promise_await`, `RtEntry::PromiseAwait`. It is deliberately not
//! named here, and its own callers say why — `array_proto/more/from_async.rs` records
//! that *"`await` here DRAINS rather than suspending"* and that *"when `Inst::Suspend`
//! lands, this changes with every other `await`"*.
//!
//! Draining is today's shape and it is the wrong one: it keeps the awaiting frame on the
//! stack and runs the loop from inside it. Calling that entry point from this graph would
//! bake it into the new IR as the meaning of `await`, and the whole reason this stage
//! exists is that the meaning is a suspension. So the suspension is what is emitted, and
//! the machine refuses it by name — `Unlowerable::NeedsFrameTransform` — until
//! `frame::resumable_form` is wired. An honest refusal, in the place that owns it.

use rts_mir::cfg::{Op, ValueId};
use rts_mir::{Domain, Effect};

use super::{Lowering, Unsupported};
use crate::syntax::Expr;

impl Lowering<'_> {
    /// `yield e`, `yield`, or `await e` — all three park the frame.
    ///
    /// `delegate` is refused by the caller rather than here: `yield*` is not a
    /// suspension, it is a loop around one.
    pub(super) fn suspend(&mut self, value: Option<ValueId>, at: &Expr) -> ValueId {
        let held = self.builder.push(
            Op::Suspend { value },
            Effect::SUSPENDS.and(Effect::THROWS).and(Effect::ALLOCATES),
            at.at,
        );
        // WHAT COMES BACK IS UNKNOWN, and stating that is the point rather than a
        // shortfall. `next(x)` chooses it, and so does a promise settling: this
        // function computed neither. Narrowing it from the operand -- the natural
        // mistake, because the operand is right there -- would be unsound in the one
        // direction that matters, since a pass would then trust a type nothing checks.
        self.types.insert(held, self.domain.top());
        held
    }

    /// `await e`.
    pub(super) fn await_on(&mut self, operand: &Expr) -> Result<ValueId, Unsupported> {
        let value = self.expression(operand)?;
        Ok(self.suspend(Some(value), operand))
    }

    /// `yield e`, `yield`, or `yield* e`.
    pub(super) fn yield_from(
        &mut self,
        value: Option<&Expr>,
        delegate: bool,
        at: &Expr,
    ) -> Result<ValueId, Unsupported> {
        if delegate {
            // A LOOP AROUND A SUSPENSION, not one -- `delegate.rs`.
            let Some(subject) = value else {
                return Err(Unsupported::Expression("`yield*` with nothing to delegate to"));
            };
            return self.delegate(subject, at);
        }
        let value = match value {
            Some(operand) => Some(self.expression(operand)?),
            // A BARE `yield` HANDS OUT NOTHING, and that is not the same as handing out
            // `undefined`: the operand is absent in the graph, so nothing has to decide
            // which singleton stands for absence. The resumer's side substitutes
            // `undefined` when it builds the result, where the language already says so.
            None => None,
        };
        Ok(self.suspend(value, at))
    }
}
