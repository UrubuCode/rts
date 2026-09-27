//! `for await (x of xs)`, as the running emitter expands it.
//!
//! # The expansion, and what it leaves out on purpose
//!
//! `emit/for_await.rs` is the definition this follows, step for step: the iterator
//! is `xs[Symbol.asyncIterator]()`, or `xs[Symbol.iterator]()` where there is none --
//! and then each VALUE is awaited, which is what `CreateAsyncFromSyncIterator` adds
//! and what an async iterator must not get a second time. Each pass awaits one
//! `next()`, and a `break` awaits the iterator's `return()`.
//!
//! No protected region, and that is the running emitter's shape rather than a
//! shortcut: a `return` or a throw out of the body does not close the iterator
//! there either, and its module states that remainder. It is also why this is not
//! `iterate.rs` with an `await` added -- that loop's region owes a close on every
//! exit, and a suspension inside a region that owes one is the frame transform's
//! measured bug class.

use rts_mir::Domain as _;
use rts_mir::cfg::{Terminator, ValueId};

use super::{FrameKind, LoopFrame, Lowering, Unsupported};
use crate::domain::{JsPrim, WellKnown};
use crate::syntax::{Expr, Pattern, Stmt};
use crate::values::Singleton;

impl Lowering<'_> {
    /// A `for await` over a target in the head's scope, which the caller has entered.
    pub(super) fn for_await(
        &mut self,
        pattern: &Pattern,
        subject: &Expr,
        body: &Stmt,
    ) -> Result<bool, Unsupported> {
        let source = self.expression(subject)?;

        // THE ITERATOR: the async one, or the sync one -- and which, kept, because it
        // decides whether each value is awaited.
        let method = self.well_known(WellKnown::AsyncIteratorSymbol, source, subject);
        let undefined = self.singleton_at(Singleton::Undefined, subject);
        let synced = self.prim(JsPrim::StrictEquals, vec![method, undefined], subject);
        let synced = self.prim(JsPrim::Truthy, vec![synced], subject);
        let from_async = self.builder.block();
        let from_sync = self.builder.block();
        let opened = self.builder.block();
        self.builder.end(Terminator::Branch {
            condition: synced,
            then_block: from_sync,
            then_args: Vec::new(),
            else_block: from_async,
            else_args: Vec::new(),
        });
        self.builder.switch_to(from_async);
        let made = self.call_method(method, source, subject);
        self.builder.end(Terminator::Jump {
            target: opened,
            args: vec![made],
        });
        self.builder.switch_to(from_sync);
        let method = self.well_known(WellKnown::IteratorSymbol, source, subject);
        let made = self.call_method(method, source, subject);
        self.builder.end(Terminator::Jump {
            target: opened,
            args: vec![made],
        });
        self.builder.switch_to(opened);
        let iterator = self.builder.param(opened);
        self.types.insert(iterator, self.domain.top());

        let mut written = self.assigned_in(body)?;
        written.extend(self.assigned_by_pattern(pattern));
        let carried = self.carried_now(written);
        let header = self.builder.block();
        let into_body = self.builder.block();
        let closing = self.builder.block();
        let exit = self.builder.block();
        let entering: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
        self.builder.end(Terminator::Jump {
            target: header,
            args: entering,
        });

        // THE HEADER awaits one step and tests `done`.
        self.builder.switch_to(header);
        let mut params = Vec::with_capacity(carried.len());
        for held in &carried {
            let param = self.builder.param(header);
            self.types.insert(param, self.domain.top());
            self.values.insert(*held, param);
            params.push(param);
        }
        let next = self.well_known(WellKnown::Next, iterator, subject);
        let pending = self.call_method(next, iterator, subject);
        let step = self.awaited(pending, subject);
        let done = self.well_known(WellKnown::Done, step, subject);
        let ended = self.prim(JsPrim::Truthy, vec![done], subject);
        self.builder.end(Terminator::Branch {
            condition: ended,
            then_block: exit,
            then_args: params,
            else_block: into_body,
            else_args: Vec::new(),
        });

        // THE ELEMENT, awaited only off a sync iterator.
        self.builder.switch_to(into_body);
        let value = self.well_known(WellKnown::Element, step, subject);
        let awaiting = self.builder.block();
        let bound = self.builder.block();
        self.builder.end(Terminator::Branch {
            condition: synced,
            then_block: awaiting,
            then_args: Vec::new(),
            else_block: bound,
            else_args: vec![value],
        });
        self.builder.switch_to(awaiting);
        let settled = self.awaited(value, subject);
        self.builder.end(Terminator::Jump {
            target: bound,
            args: vec![settled],
        });
        self.builder.switch_to(bound);
        let element = self.builder.param(bound);
        self.types.insert(element, self.domain.top());

        let pass = self.open_pass(self.scope, subject);
        self.destructure(pattern, element, subject)?;
        self.loops.push(LoopFrame {
            labels: std::mem::take(&mut self.pending_labels),
            kind: FrameKind::Loop,
            header,
            exit: closing,
            carried: carried.clone(),
        });
        let left = self.statement(body);
        self.loops.pop();
        if let Some((restored, _)) = pass {
            self.close_pass(restored);
        }
        if !left? {
            let back: Vec<ValueId> = carried.iter().map(|held| self.values[held]).collect();
            self.builder.end(Terminator::Jump {
                target: header,
                args: back,
            });
        }

        // `break`: the iterator's `return()`, AWAITED -- `AsyncIteratorClose`.
        self.builder.switch_to(closing);
        let broke: Vec<ValueId> = carried
            .iter()
            .map(|_| {
                let param = self.builder.param(closing);
                self.types.insert(param, self.domain.top());
                param
            })
            .collect();
        let method = self.well_known(WellKnown::Return, iterator, subject);
        let absent = self.prim(JsPrim::IsNullish, vec![method], subject);
        let calling = self.builder.block();
        let closed = self.builder.block();
        self.builder.end(Terminator::Branch {
            condition: absent,
            then_block: closed,
            then_args: Vec::new(),
            else_block: calling,
            else_args: Vec::new(),
        });
        self.builder.switch_to(calling);
        let answered = self.call_method(method, iterator, subject);
        self.awaited(answered, subject);
        self.builder.end(Terminator::Jump {
            target: closed,
            args: Vec::new(),
        });
        self.builder.switch_to(closed);
        self.builder.end(Terminator::Jump {
            target: exit,
            args: broke,
        });

        self.builder.switch_to(exit);
        for held in &carried {
            let param = self.builder.param(exit);
            self.types.insert(param, self.domain.top());
            self.values.insert(*held, param);
        }
        Ok(false)
    }
}
