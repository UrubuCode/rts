//! `for`-`of`: stepping the protocol, and the three ways out that owe a close.
//!
//! # Why it steps rather than draining, although draining already exists
//!
//! `rts_core::entry::iterate` turns an iterable into an array and is reached by
//! `CoreEntry::Iterate`. Using it here would have been one call and no loop, and it is
//! wrong — its own header says for whom it is right: *"what still arrives here is
//! everything that must consume the WHOLE sequence to answer at all"*. A `for`-`of` does
//! not. Draining one changes three answers, and the old emitter measured all three
//! against Bun before rewriting itself:
//!
//! - a `break` never reaches `return()` on the iterator,
//! - a `Map` or `Set` the body mutates is walked as it was BEFORE the body ran,
//! - a source that never reports `done` is drained forever instead of ending the pass it
//!   was told to.
//!
//! The first is a leak, the second a wrong answer, and the third turns a program that
//! terminates into one that does not. So: step.
//!
//! # What this deliberately does NOT do, and why it is not the same omission
//!
//! `emit/foreach.rs` carries TWO arms in one loop — an indexed walk for an array or a
//! string, the stepped protocol for everything else — because stepping costs a
//! `{ value, done }` allocation per element, including for `for (const x of anArray)`,
//! which is the common case the indexed walk exists to avoid.
//!
//! That specialisation is not reproduced here, and the reason is where it belongs rather
//! than whether it is worth having. The old emitter had to decide it while emitting,
//! from syntax, which is why it emits both arms and lets one be dead. Here the question
//! is *"is this value an array"*, which is a type and a guard — `rts_mir`'s own
//! machinery, applied by a pass over a graph that already says what it is doing. Writing
//! the dual arm into the lowering would spend the thing this stage exists to provide.
//!
//! So this emits one honest loop, and the specialisation is a pass. What it must not do
//! is emit one honest loop and call it fast.
//!
//! # The three ways out, and why only one of them is a jump
//!
//! | leaving by | closes? | how |
//! |---|---|---|
//! | `done` | no | the sequence ended itself; there is nothing to close |
//! | `break` | yes | a block between the loop and the exit, which closes and falls through |
//! | `return` or a raise | yes | the region's CLEANUP, copied into each path by the machine |
//!
//! `break` is a jump and gets a block. `return` and a raise are not jumps out of the
//! loop — they leave the function — so no block this lowering writes could be on their
//! path. That is precisely what a cleanup is for: `unwind::plan_unwind` and
//! `plan_normal_exit` find those paths from the region tree and copy the piece into
//! them.
//!
//! **The `done` path must not run it**, which is the reason the close is NOT simply the
//! cleanup for all three. A cleanup runs on every way out of a region, and the ordinary
//! end of a sequence is a way out that owes nothing — `it.return()` called after `done`
//! is an observable extra call on a user's iterator.
//!
//! So the loop's region is entered for the BODY and left before the exit, and `break`
//! carries its own closing block. The obligation itself is not decided here:
//! `ForEachSource::owes_iterator_close` is in the tree and answers it.
//!
//! # `return` on the iterator is optional, and the cleanup branches
//!
//! An iterator need not have one, and calling `undefined` would raise where the
//! specification says do nothing. So the cleanup reads the key, asks whether it is
//! nullish, and calls only if it is not — a piece that branches inside itself, which is
//! exactly what `Terminator::CleanupDone` allows and says it allows.

use rts_mir::Domain;
use rts_mir::cfg::{Callee, Op, Terminator, ValueId};

use super::{FrameKind, LoopFrame, Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim, WellKnown};
use crate::runtime::RuntimeOp;
use crate::syntax::{Expr, ForEachSource, ForEachTarget, Spreadable, Stmt};

impl Lowering<'_> {
    /// Lowers a `for`-each loop.
    pub(super) fn for_each(
        &mut self,
        source: ForEachSource,
        target: &ForEachTarget,
        subject: &Expr,
        body: &Stmt,
        at: &Stmt,
    ) -> Result<bool, Unsupported> {
        // THE HEAD HAS ITS OWN SCOPE, which is where the per-pass binding lives. Without
        // entering it the target is not found at all and reads as a GLOBAL -- the same
        // thing the `catch` clause reported the first time it was run, and the same fix.
        let Some(head) = self.resolution.head_scope(at.at) else {
            return Err(Unsupported::NoScope);
        };
        let outer = std::mem::replace(&mut self.scope, head);
        let lowered = self.for_each_in_head(source, target, subject, body);
        self.scope = outer;
        lowered
    }

    fn for_each_in_head(
        &mut self,
        source: ForEachSource,
        target: &ForEachTarget,
        subject: &Expr,
        body: &Stmt,
    ) -> Result<bool, Unsupported> {
        match source {
            ForEachSource::Of => {}
            // `for`-`in` walks enumerable string keys INCLUDING inherited ones, which is
            // a walk of the prototype chain and not a protocol at all -- the tree's own
            // comment on the variant calls it the trap. Nothing here is reusable for it.
            ForEachSource::In => {
                let (ForEachTarget::Declare { target: pattern, .. }
                | ForEachTarget::Assign(pattern)) = target
                else {
                    return Err(Unsupported::Statement(
                        "a for-in target that is disposed at the end of each pass",
                    ));
                };
                return self.for_in(pattern, subject, body);
            }
            // `for await` asks for `Symbol.asyncIterator` and awaits each step -- with no
            // region owing a close, which is `for_await.rs`'s reason for being apart.
            ForEachSource::AwaitOf => {
                let (ForEachTarget::Declare { target: pattern, .. }
                | ForEachTarget::Assign(pattern)) = target
                else {
                    return Err(Unsupported::Statement(
                        "a for-await target that is disposed at the end of each pass",
                    ));
                };
                return self.for_await(pattern, subject, body);
            }
        }

        // A DECLARED target is fresh per pass, in the head's scope; an ASSIGNED one
        // writes bindings outside the loop, which the carried set below takes from
        // the target as well as from the body.
        let pattern = match target {
            ForEachTarget::Declare { target, .. } | ForEachTarget::Assign(target) => target,
            _ => {
                return Err(Unsupported::Statement(
                    "a for-of target that is disposed at the end of each pass",
                ));
            }
        };

        // THE ITERATOR, once, before the loop. `xs[Symbol.iterator]()` with `xs` as the
        // receiver -- a method read and called, which is what the specification says and
        // what makes a source declaring one on its prototype work.
        let subject_value = self.expression(subject)?;
        let method = self.well_known(WellKnown::IteratorSymbol, subject_value, subject);
        let iterator = self.call_method(method, subject_value, subject);

        let mut written = self.assigned_in(body)?;
        written.extend(self.assigned_by_pattern(pattern));
        let carried = self.carried_now(written);
        let header = self.builder.block();
        let into_body = self.builder.block();
        // THE CLOSING BLOCK, which is where `break` goes. Created before the region opens
        // so that it belongs outside it: closing an iterator is not itself protected by
        // the loop's own cleanup, or a `return()` that threw would close twice.
        let closing = self.builder.block();
        let cleanup = self.builder.block();
        let exit = self.builder.block();

        let entering: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
        self.builder.end(Terminator::Jump {
            target: header,
            args: entering,
        });

        // THE HEADER steps the protocol and tests `done`, which is what makes the test
        // happen before each pass including the first.
        self.builder.switch_to(header);
        let mut params = Vec::with_capacity(carried.len());
        for binding in &carried {
            let param = self.builder.param(header);
            self.types.insert(param, self.domain.top());
            self.values.insert(*binding, param);
            params.push(param);
        }
        let next = self.well_known(WellKnown::Next, iterator, subject);
        let step = self.call_method(next, iterator, subject);
        let done = self.well_known(WellKnown::Done, step, subject);
        // ToBoolean and not a comparison with `true`: an iterator answering `done: 1`
        // ends the loop, which is what the specification says.
        let ended = self.prim(JsPrim::Truthy, vec![done], subject);
        self.builder.end(Terminator::Branch {
            condition: ended,
            // The sequence ended ITSELF, so nothing is owed. Straight to the exit, past
            // the closing block.
            then_block: exit,
            then_args: params.clone(),
            else_block: into_body,
            else_args: Vec::new(),
        });

        // THE BODY, inside the region whose cleanup closes the iterator.
        self.builder.switch_to(into_body);
        self.builder.open_region(None, Some(cleanup));
        let element = self.well_known(WellKnown::Element, step, subject);
        // A HEAD A CLOSURE CAPTURES is a fresh environment per pass, bound before the
        // body runs -- no copy, since nothing of one pass is the next one's.
        let pass = self.open_pass(self.scope, subject);
        self.destructure(pattern, element, subject)?;
        self.loops.push(LoopFrame {
            labels: std::mem::take(&mut self.pending_labels),
            kind: FrameKind::Loop,
            header,
            // `break` leaves through the CLOSE and not through the exit, which is the
            // whole of what makes this loop different from a `while`.
            exit: closing,
            carried: carried.clone(),
        });
        let left = self.statement(body);
        self.loops.pop();
        if let Some((restored, _)) = pass {
            self.close_pass(restored);
        }
        let left = left?;
        if !left {
            let back: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
            self.builder.end(Terminator::Jump {
                target: header,
                args: back,
            });
        }
        self.builder.close_region();

        // THE CLEANUP PIECE, for the paths no block here is on: a `return` out of the
        // body and a raise from it.
        self.builder.switch_to(cleanup);
        self.close_iterator(iterator, subject);
        self.builder.end(Terminator::CleanupDone);

        // THE CLOSING BLOCK, for `break`. Its parameters are what the breaking edge
        // carries, and it hands the same values on to the exit.
        self.builder.switch_to(closing);
        let broke: Vec<ValueId> = carried
            .iter()
            .map(|_| {
                let param = self.builder.param(closing);
                self.types.insert(param, self.domain.top());
                param
            })
            .collect();
        self.close_iterator(iterator, subject);
        self.builder.end(Terminator::Jump {
            target: exit,
            args: broke,
        });

        self.builder.switch_to(exit);
        let leaving: Vec<ValueId> = carried
            .iter()
            .map(|_| {
                let param = self.builder.param(exit);
                self.types.insert(param, self.domain.top());
                param
            })
            .collect();
        for (binding, param) in carried.iter().zip(&leaving) {
            self.values.insert(*binding, *param);
        }
        Ok(false)
    }

    /// Reads a key this language fixes, off `object`.
    pub(super) fn well_known(&mut self, which: WellKnown, object: ValueId, at: &Expr) -> ValueId {
        let index = self.domain.constant(JsConst::WellKnown(which));
        let key = self.declared(index, at);
        self.prim(JsPrim::FieldRead, vec![object, key], at)
    }

    /// Calls what `method` holds, with `receiver` as the receiver.
    ///
    /// The receiver is the `Op::Call` field and not argument zero, which is the whole
    /// reason that field exists: how one reaches a callee is the machine's convention.
    pub(super) fn call_method(&mut self, method: ValueId, receiver: ValueId, at: &Expr) -> ValueId {
        let effect = rts_mir::Effect::CALLS_USER
            .and(rts_mir::Effect::THROWS)
            .and(rts_mir::Effect::ALLOCATES);
        let held = self.builder.push(
            Op::Call {
                callee: Callee::Dynamic(method),
                receiver: Some(receiver),
                args: Vec::new(),
            },
            effect,
            at.at,
        );
        self.types.insert(held, self.domain.top());
        held
    }

    /// `it.return?.()` — the close, written once and used from both paths that owe it.
    ///
    /// Branches on whether the key holds anything, because an iterator need not have a
    /// `return` and calling `undefined` would raise where the specification says do
    /// nothing. Leaves the builder in the block that follows the call, which is what
    /// lets a caller terminate it however its own path requires.
    pub(super) fn close_iterator(&mut self, iterator: ValueId, at: &Expr) {
        let method = self.well_known(WellKnown::Return, iterator, at);
        let absent = self.prim(JsPrim::IsNullish, vec![method], at);
        let calling = self.builder.block();
        let after = self.builder.block();
        self.builder.end(Terminator::Branch {
            condition: absent,
            then_block: after,
            then_args: Vec::new(),
            else_block: calling,
            else_args: Vec::new(),
        });
        self.builder.switch_to(calling);
        self.call_method(method, iterator, at);
        self.builder.end(Terminator::Jump {
            target: after,
            args: Vec::new(),
        });
        self.builder.switch_to(after);
    }

    /// An array literal, whether or not a spread widens it.
    ///
    /// # Two shapes, and the cheap one is not an optimisation
    ///
    /// With no spread, the elements are known and `NewArray` takes them — one operation,
    /// no calls, and the array arrives full. That is what this emitted before a spread
    /// was expressible and it is unchanged, which matters: `[a, b, c]` must not start
    /// paying for a feature it does not use.
    ///
    /// With a spread, the count is not a number this stage has, so the array is BUILT:
    /// one empty array and an append per element. `ArrayAppend` for a single value and
    /// `ArrayAppendAll` for a spread, both answering the array so the calls chain.
    ///
    /// Keeping both is not two answers to one question — it is one answer whose input
    /// differs. A literal either has a spread in it or does not, and that is settled by
    /// the syntax rather than by anything the graph would have to guess.
    ///
    /// # Why the spread DRAINS, when `for`-`of` must not
    ///
    /// `[...xs]` cannot answer at all until the sequence ends, so draining it is the
    /// operation rather than a divergence from it — which is the line
    /// `rts_core::entry::iterate`'s own header draws, and the opposite side of the one
    /// `lower/iterate.rs` opens with. The same runtime, asked two different questions.
    pub(super) fn array_literal(
        &mut self,
        elements: &[Option<Spreadable>],
        at: &Expr,
    ) -> Result<ValueId, Unsupported> {
        // A HOLE is not `undefined`: `[, 1]` has a position some operations skip and
        // others read as `undefined`. So a literal with one is APPENDED, like one with a
        // spread, and the hole is the runtime's own marker for an unwritten position --
        // `emit/expr.rs`'s way, which leaves `0 in [, 1]` false.
        let appended = elements
            .iter()
            .any(|held| !matches!(held, Some(Spreadable::Single(_))));
        if !appended {
            let mut values = Vec::with_capacity(elements.len());
            for element in elements {
                if let Some(Spreadable::Single(held)) = element {
                    values.push(self.expression(held)?);
                }
            }
            return Ok(self.prim(JsPrim::NewArray, values, at));
        }

        let mut array = self.prim(JsPrim::NewArray, Vec::new(), at);
        for element in elements {
            match element {
                Some(Spreadable::Single(held)) => {
                    let value = self.expression(held)?;
                    array = self.entry(RuntimeOp::ArrayAppend, vec![array, value], at);
                }
                Some(Spreadable::Spread(held)) => {
                    let value = self.expression(held)?;
                    array = self.entry(RuntimeOp::ArrayAppendAll, vec![array, value], at);
                }
                None => {
                    let hole = self.domain.constant(crate::domain::JsConst::Hole);
                    let hole = self.declared(hole, at);
                    array = self.entry(RuntimeOp::ArrayAppend, vec![array, hole], at);
                }
            }
        }
        Ok(array)
    }

    /// Calls an entry point of this language's table.
    ///
    /// Here rather than at each site because the effect is the entry's and not the
    /// caller's: what an entry point does is a row of `ENTRIES`, and a caller writing
    /// its own summary is how an effect table stops being the one source.
    pub(super) fn entry(&mut self, which: RuntimeOp, args: Vec<ValueId>, at: &Expr) -> ValueId {
        let entry = self.domain.entry_point(which);
        // ALLOCATES and CALLS_USER and THROWS, for the honest reason: appending grows an
        // array, a spread runs the source's `next`, and both raise on something that is
        // not iterable. Narrowing this per entry is what `ENTRIES` grows a column for,
        // and claiming PURE here would be rule 5's silent wrong program.
        let effect = rts_mir::Effect::ALLOCATES
            .and(rts_mir::Effect::CALLS_USER)
            .and(rts_mir::Effect::THROWS);
        let held = self.builder.push(
            Op::Call {
                callee: Callee::Entry(entry),
                receiver: None,
                args,
            },
            effect,
            at.at,
        );
        let answered = self.domain.of_entry(entry);
        self.types.insert(held, answered);
        held
    }
}
