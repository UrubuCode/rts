//! `yield*` — producing everything another iterable produces, in the graph.
//!
//! It is a LOOP whose body is an ordinary `yield`, and `emit/delegate.rs` is the
//! definition this follows path for path, because the running engine and this stage
//! must hand out the same values in the same order:
//!
//! - the source's `[Symbol.iterator]()`, where it declares one, else the source itself;
//! - where that has a callable `next`, the protocol: `DelegateStep(next, iterator, sent)`
//!   until the step is `done`, yielding each `value` and sending what the resumption
//!   delivered into the next step -- the finished step's `value` is what the whole
//!   expression answers. `DelegateStep` and not a call, because the runtime records the
//!   iterator this generator stands in front of while it is parked; forwarding
//!   `outer.throw(e)` and `outer.return(v)` happens there, not here;
//! - otherwise what `Iterate` materialises, element by element, answering `undefined`.
//!
//! Nothing here writes a binding of the program, so the loops carry only their own
//! values as block parameters and no binding crosses a back edge.

use rts_mir::Domain as _;
use rts_mir::Effect;
use rts_mir::cfg::{Const, Op, Terminator, ValueId};

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim, WellKnown};
use crate::runtime::RuntimeOp;
use crate::syntax::Expr;
use crate::values::Singleton;

impl Lowering<'_> {
    /// `yield* subject`, answering what the expression evaluates to.
    pub(super) fn delegate(&mut self, subject: &Expr, at: &Expr) -> Result<ValueId, Unsupported> {
        let source = self.expression(subject)?;

        // THE ITERATOR: `source[Symbol.iterator]()` where declared, else the source.
        let method = self.well_known(WellKnown::IteratorSymbol, source, subject);
        let declares = self.is_function(method, subject);
        let asked = self.builder.block();
        let itself = self.builder.block();
        let join = self.builder.block();
        self.builder.end(Terminator::Branch {
            condition: declares,
            then_block: asked,
            then_args: Vec::new(),
            else_block: itself,
            else_args: Vec::new(),
        });
        self.builder.switch_to(asked);
        let made = self.call_method(method, source, subject);
        self.builder.end(Terminator::Jump {
            target: join,
            args: vec![made],
        });
        self.builder.switch_to(itself);
        self.builder.end(Terminator::Jump {
            target: join,
            args: vec![source],
        });
        self.builder.switch_to(join);
        let iterator = self.top_param(join);

        // WHICH LOOP: a callable `next` is the protocol, anything else is materialised.
        let step = self.well_known(WellKnown::Next, iterator, subject);
        let steppable = self.is_function(step, subject);
        let stepwise = self.builder.block();
        let listwise = self.builder.block();
        let done = self.builder.block();
        let first = self.singleton_at(Singleton::Undefined, at);
        self.builder.end(Terminator::Branch {
            condition: steppable,
            then_block: stepwise,
            then_args: vec![first],
            else_block: listwise,
            else_args: Vec::new(),
        });

        // THE PROTOCOL, with what was sent in as the header's parameter.
        self.builder.switch_to(stepwise);
        let sent = self.top_param(stepwise);
        let answered = self.entry(RuntimeOp::DelegateStep, vec![step, iterator, sent], at);
        let element = self.well_known(WellKnown::Element, answered, at);
        let finished = self.well_known(WellKnown::Done, answered, at);
        let finished = self.prim(JsPrim::Truthy, vec![finished], at);
        let body = self.builder.block();
        self.builder.end(Terminator::Branch {
            condition: finished,
            then_block: done,
            then_args: vec![element],
            else_block: body,
            else_args: Vec::new(),
        });
        self.builder.switch_to(body);
        let received = self.suspend(Some(element), at);
        self.builder.end(Terminator::Jump {
            target: stepwise,
            args: vec![received],
        });

        // THE LIST, indexed.
        self.builder.switch_to(listwise);
        let values = self.entry(RuntimeOp::Iterate, vec![iterator], at);
        let count = self.entry(RuntimeOp::ArrayLength, vec![values], at);
        let zero = self.number(0, at);
        let head = self.builder.block();
        self.builder.end(Terminator::Jump {
            target: head,
            args: vec![zero],
        });
        self.builder.switch_to(head);
        let index = self.top_param(head);
        let more = self.prim(JsPrim::LessThan, vec![index, count], at);
        let more = self.prim(JsPrim::Truthy, vec![more], at);
        let each = self.builder.block();
        let finished = self.singleton_at(Singleton::Undefined, at);
        self.builder.end(Terminator::Branch {
            condition: more,
            then_block: each,
            then_args: Vec::new(),
            else_block: done,
            else_args: vec![finished],
        });
        self.builder.switch_to(each);
        let item = self.prim(JsPrim::IndexRead, vec![values, index], at);
        self.suspend(Some(item), at);
        let one = self.number(1, at);
        let next = self.prim(JsPrim::Add, vec![index, one], at);
        self.builder.end(Terminator::Jump {
            target: head,
            args: vec![next],
        });

        self.builder.switch_to(done);
        Ok(self.top_param(done))
    }

    /// `typeof value === "function"`, as a truth value a branch takes.
    fn is_function(&mut self, value: ValueId, at: &Expr) -> ValueId {
        let kind = self.prim(JsPrim::TypeOf, vec![value], at);
        let text = crate::syntax::Text::from_units("function".encode_utf16().collect());
        let spelled = self.domain.constant(JsConst::Text(text));
        let spelled = self.declared(spelled, at);
        let same = self.prim(JsPrim::StrictEquals, vec![kind, spelled], at);
        self.prim(JsPrim::Truthy, vec![same], at)
    }

    /// A parameter of `block` nothing is known about.
    fn top_param(&mut self, block: rts_mir::BlockId) -> ValueId {
        let param = self.builder.param(block);
        self.types.insert(param, self.domain.top());
        param
    }

    /// A number the lowering itself writes.
    fn number(&mut self, value: i64, at: &Expr) -> ValueId {
        let held = Const::Int(value);
        let of = self.domain.of_const(&held);
        let pushed = self.builder.push(Op::Const(held), Effect::PURE, at.at);
        self.types.insert(pushed, of);
        pushed
    }
}
