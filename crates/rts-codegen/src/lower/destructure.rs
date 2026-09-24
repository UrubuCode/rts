//! An object pattern, read as the property reads it is.
//!
//! # Why the array form is refused and this one is not
//!
//! `const { a } = o` reads the property `a` of `o`. That is all it is, and it is what
//! this file lowers.
//!
//! `const [a] = xs` is **not** `a = xs[0]`. Array destructuring steps the ITERATOR
//! protocol: it reads `xs[Symbol.iterator]`, calls it, and calls `next()` once per
//! element — so it works on a `Set`, on a generator and on anything with a `next`,
//! and it does not work on an object with numeric keys and no iterator. A lowering
//! that indexed would be wrong in both directions at once: it would accept what the
//! language refuses and refuse what the language accepts.
//!
//! So the array form keeps its refusal under the same name as `for`-`of` — an
//! iteration protocol — which is the piece that answers both.
//!
//! # What a default is, exactly
//!
//! `undefined` specifically, and not absent and not falsy. The tree's own comment
//! says it: `[a = 1] = [null]` binds `null`, because `null` is a value that was
//! there. And the default is evaluated only when it is needed, so `{ a = f() }` over
//! an object that has `a` never calls `f` — which is why it is a branch and not an
//! argument to a coalescing operation.

use super::choice::Arm;
use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim, WellKnown};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{Expr, Pattern, PropertyKey};
use rts_mir::Domain;
use rts_mir::cfg::{Terminator, ValueId};

impl Lowering<'_> {
    /// Binds every name an object pattern names, from a value already lowered.
    ///
    /// Answers the names it bound, so a caller that needs to know what a declaration
    /// introduced does not have to walk the pattern a second time.
    pub(super) fn destructure(
        &mut self,
        pattern: &Pattern,
        from: ValueId,
        at: &Expr,
    ) -> Result<Vec<Name>, Unsupported> {
        let Pattern::Object(object) = pattern else {
            return match pattern {
                Pattern::Array(array) => self.destructure_array(array, from, at),
                _ => Err(Unsupported::Pattern),
            };
        };
        if object.rest.is_some() {
            // A rest target collects the own enumerable properties NOT already named,
            // which needs the key set of the object at run time -- an operation this
            // table does not have, and not one an ordinary read can stand in for.
            return Err(Unsupported::Expression(
                "an object rest target needs the own keys at run time",
            ));
        }

        let mut bound = Vec::with_capacity(object.properties.len());
        for property in &object.properties {
            let PropertyKey::Named(key) = &property.key else {
                return Err(Unsupported::Expression(
                    "a computed key in a pattern is a value, so which property is read is not written",
                ));
            };
            let index = self.domain.constant(JsConst::Key(*key));
            let named = self.declared(index, at);
            let read = self.prim(JsPrim::FieldRead, vec![from, named], at);

            // THE DEFAULT IS A BRANCH, because it runs only when the value read was
            // `undefined` -- so `{ a = f() }` over an object that has `a` must not
            // call `f`. A coalescing operation would also be wrong for `null`, which
            // takes no default.
            let held = match &property.value.default {
                None => read,
                Some(default) => {
                    let undefined = self.singleton_at(crate::values::Singleton::Undefined, at);
                    let absent = self.prim(JsPrim::StrictEquals, vec![read, undefined], at);
                    self.choice(absent, Arm::Eval(default), Arm::Subject(read))?
                }
            };

            let Pattern::Name(name) = &property.value.pattern else {
                return Err(Unsupported::Expression(
                    "a nested pattern needs the value read to be destructured again",
                ));
            };
            let of = self.type_of(held);
            let target = Expr {
                kind: crate::syntax::ExprKind::Ident(*name),
                at: at.at,
            };
            self.bind(*name, held, of, &target)?;
            bound.push(*name);
        }
        Ok(bound)
    }

    /// Binds every name an ARRAY pattern names, by stepping the iterator.
    ///
    /// # Why each slot needs a branch and not just the step's `value`
    ///
    /// Because a slot past the end of the sequence is `undefined`, and the step's own
    /// `value` is not reliably that. `{ done: true, value: 42 }` is a legal answer from
    /// a hand-written iterator, and the specification says a slot reached after `done`
    /// binds `undefined` -- not 42. Reading `value` unconditionally is correct for every
    /// well-behaved iterator and silently wrong for one that is not, which is the shape
    /// of defect this stage exists to stop shipping.
    ///
    /// So: one step, one truth test on `done`, and a join whose two arms are the value
    /// and the singleton.
    ///
    /// # A HOLE still takes a step
    ///
    /// `[, b] = xs` reads two elements and binds one. The tree says so by keeping the
    /// hole as an absent element rather than omitting it -- *"dropping holes would shift
    /// every element after them onto the wrong value"* -- and this steps for one exactly
    /// as it does for a named slot.
    ///
    /// # The close, and when it is owed
    ///
    /// If the pattern stopped before the sequence did, the iterator is owed its
    /// `return()`: `const [a] = endless()` takes one element and closes. So the close is
    /// under the LAST slot's `done` test -- not done means there is more, which means the
    /// pattern is the one that stopped.
    ///
    /// A pattern with no elements at all closes unconditionally, because it stepped
    /// nothing and therefore never reached `done`.
    pub(super) fn destructure_array(
        &mut self,
        pattern: &crate::syntax::ArrayPattern,
        from: ValueId,
        at: &Expr,
    ) -> Result<Vec<Name>, Unsupported> {
        // A REST TARGET IS A LOOP AND NOT A DRAIN, which is the distinction the refusal
        // that stood here drew and then could not act on. `ArrayAppendAll` drains an
        // iterable from the START; a rest target gathers what this iterator has LEFT,
        // and those differ by however many slots came before it. So the loop is built
        // below, out of the same step the slots use and `ArrayAppend` per element.
        //
        // Handing the ITERATOR to `ArrayAppendAll` would have compiled and would be
        // right for every built-in iterator, because those answer themselves from
        // `Symbol.iterator`. A hand-written one need not, and then the drain restarts
        // the source or raises -- the same class of defect as reading `value` past
        // `done`, correct until someone writes their own iterator.
        let nested_rest =
            matches!(pattern.rest.as_deref(), Some(other) if !matches!(other, Pattern::Name(_)));
        if nested_rest {
            return Err(Unsupported::Expression(
                "a nested pattern needs the value read to be destructured again",
            ));
        }

        let method = self.well_known(WellKnown::IteratorSymbol, from, at);
        let iterator = self.call_method(method, from, at);

        let mut bound = Vec::new();
        let mut exhausted = None;
        // WITH A REST, THE LAST SLOT IS ITS PLACEHOLDER and not a hole: the tree keeps
        // the rest's position in `elements` so that both roles of a pattern have one
        // shape, and `emit/destructure` drops it for the same reason. Stepping it as a
        // hole took one element away from the rest -- `const [h, ...t] = [1, 2, 3]`
        // gathered `[3]` -- which a program that ran through this stage showed.
        let slots = match pattern.rest {
            Some(_) => &pattern.elements[..pattern.elements.len().saturating_sub(1)],
            None => &pattern.elements[..],
        };
        for slot in slots {
            let next = self.well_known(WellKnown::Next, iterator, at);
            let step = self.call_method(next, iterator, at);
            let done = self.well_known(WellKnown::Done, step, at);
            let ended = self.prim(JsPrim::Truthy, vec![done], at);
            exhausted = Some(ended);

            // A HOLE binds nothing and the step above is the whole of its effect.
            let Some(element) = slot else {
                continue;
            };

            let past = self.builder.block();
            let within = self.builder.block();
            let joined = self.builder.block();
            self.builder.end(Terminator::Branch {
                condition: ended,
                then_block: past,
                then_args: Vec::new(),
                else_block: within,
                else_args: Vec::new(),
            });
            self.builder.switch_to(past);
            let absent = self.singleton_at(crate::values::Singleton::Undefined, at);
            self.builder.end(Terminator::Jump {
                target: joined,
                args: vec![absent],
            });
            self.builder.switch_to(within);
            let held = self.well_known(WellKnown::Element, step, at);
            self.builder.end(Terminator::Jump {
                target: joined,
                args: vec![held],
            });
            self.builder.switch_to(joined);
            let value = self.builder.param(joined);
            self.types.insert(value, self.domain.top());

            let value = match &element.default {
                // `undefined` SPECIFICALLY, and not absent and not falsy: `[a = 1] =
                // [null]` binds `null`, because `null` is a value that was there. And
                // the default is evaluated only when it is needed, which is why this is
                // a branch rather than an argument to a coalescing operation.
                Some(default) => {
                    let absent = self.singleton_at(crate::values::Singleton::Undefined, at);
                    let missing = self.prim(JsPrim::StrictEquals, vec![value, absent], at);
                    self.choice(missing, Arm::Eval(default), Arm::Subject(value))?
                }
                None => value,
            };
            // A NESTED pattern needs the value destructured again, which is the same
            // refusal the object form gives for the same reason and under the same name.
            let Pattern::Name(name) = &element.pattern else {
                return Err(Unsupported::Expression(
                    "a nested pattern needs the value read to be destructured again",
                ));
            };
            let of = self.type_of(value);
            let target = Expr {
                kind: crate::syntax::ExprKind::Ident(*name),
                at: at.at,
            };
            self.bind(*name, value, of, &target)?;
            bound.push(*name);
        }

        // A REST TARGET TAKES EVERYTHING LEFT, and then the sequence is over -- so
        // nothing is owed and the close below is skipped entirely. That is not an
        // omission: the iterator reported `done` itself, which is the one way out that
        // closes nothing.
        if let Some(Pattern::Name(name)) = pattern.rest.as_deref() {
            let gathered = self.gather_rest(iterator, at)?;
            let of = self.type_of(gathered);
            let target = Expr {
                kind: crate::syntax::ExprKind::Ident(*name),
                at: at.at,
            };
            self.bind(*name, gathered, of, &target)?;
            bound.push(*name);
            return Ok(bound);
        }

        // THE CLOSE, owed only if the PATTERN stopped first. Not done after the last
        // step means the sequence has more, so `const [a] = endless()` closes and
        // `const [a, b] = [1, 2]` does not.
        match exhausted {
            Some(ended) => {
                let closing = self.builder.block();
                let after = self.builder.block();
                self.builder.end(Terminator::Branch {
                    condition: ended,
                    then_block: after,
                    then_args: Vec::new(),
                    else_block: closing,
                    else_args: Vec::new(),
                });
                self.builder.switch_to(closing);
                self.close_iterator(iterator, at);
                self.builder.end(Terminator::Jump {
                    target: after,
                    args: Vec::new(),
                });
                self.builder.switch_to(after);
            }
            // NO ELEMENTS: `const [] = xs` asks for the iterator and steps nothing, so
            // it never reached `done` and owes the close unconditionally.
            None => self.close_iterator(iterator, at),
        }
        Ok(bound)
    }

    /// Everything the iterator has left, as an array.
    ///
    /// A loop rather than a call, and the loop carries ONE value across its back edge:
    /// the array. Everything else it needs -- the iterator, the keys -- is constant
    /// through it, which is why this needs none of the carried-binding machinery a
    /// written loop does.
    ///
    /// It cannot close, and that is not an omission. Gathering runs until the iterator
    /// reports `done`, which is the one way out that owes nothing.
    fn gather_rest(&mut self, iterator: ValueId, at: &Expr) -> Result<ValueId, Unsupported> {
        let empty = self.prim(JsPrim::NewArray, Vec::new(), at);
        let header = self.builder.block();
        let appending = self.builder.block();
        let exit = self.builder.block();
        self.builder.end(Terminator::Jump {
            target: header,
            args: vec![empty],
        });

        self.builder.switch_to(header);
        let holding = self.builder.param(header);
        self.types.insert(holding, self.domain.top());
        let next = self.well_known(WellKnown::Next, iterator, at);
        let step = self.call_method(next, iterator, at);
        let done = self.well_known(WellKnown::Done, step, at);
        let ended = self.prim(JsPrim::Truthy, vec![done], at);
        self.builder.end(Terminator::Branch {
            condition: ended,
            then_block: exit,
            then_args: vec![holding],
            else_block: appending,
            else_args: Vec::new(),
        });

        self.builder.switch_to(appending);
        let element = self.well_known(WellKnown::Element, step, at);
        let grown = self.entry(RuntimeOp::ArrayAppend, vec![holding, element], at);
        // THE APPEND'S ANSWER goes round the back edge and not the array it was given.
        // They are the same object today and the entry point answers one deliberately,
        // so carrying the operand instead would be reading a value whose definition
        // does not dominate the next pass.
        self.builder.end(Terminator::Jump {
            target: header,
            args: vec![grown],
        });

        self.builder.switch_to(exit);
        let gathered = self.builder.param(exit);
        self.types.insert(gathered, self.domain.top());
        Ok(gathered)
    }
}
