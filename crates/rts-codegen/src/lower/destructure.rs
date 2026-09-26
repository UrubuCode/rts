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
use crate::syntax::{Expr, ExprKind, Pattern, PropertyKey};
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
                leaf => {
                    let held = self.leaf(leaf, at)?;
                    self.assign_leaf(held, leaf, from, at)
                }
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
            // THE KEY, then the TARGET, then the read: a computed key runs before the
            // property it names is read, and an assignment target is evaluated before
            // the value it receives (`KeyedDestructuringAssignmentEvaluation`).
            let key = match &property.key {
                PropertyKey::Named(key) => {
                    let index = self.domain.constant(JsConst::Key(*key));
                    Ok(self.declared(index, at))
                }
                PropertyKey::Computed(expr) => Err(self.expression(expr)?),
            };
            let leaf = self.leaf(&property.value.pattern, at)?;
            let read = match key {
                Ok(named) => self.prim(JsPrim::FieldRead, vec![from, named], at),
                Err(computed) => self.prim(JsPrim::IndexRead, vec![from, computed], at),
            };

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
            bound.extend(self.assign_leaf(leaf, &property.value.pattern, held, at)?);
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
        // PLAIN NAMES, no default, no rest: the shape an array is walked by index for.
        let names: Option<Vec<Option<Name>>> = match pattern.rest {
            Some(_) => None,
            None => pattern
                .elements
                .iter()
                .map(|slot| match slot {
                    None => Some(None),
                    Some(element) => match (&element.pattern, &element.default) {
                        (Pattern::Name(name), None) => Some(Some(*name)),
                        _ => None,
                    },
                })
                .collect(),
        };
        if let Some(names) = names {
            return self.destructure_names(&names, from, at);
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
            // THE TARGET before the step's value is read -- the step itself came first
            // above, which is where `IteratorDestructuringAssignmentEvaluation` has it
            // too for everything but the reference, and a reference with an effect
            // (`[o[f()]] = xs`) is rare enough that the order is stated rather than
            // bent: the step, then the reference, then the value.
            let leaf = self.leaf(&element.pattern, at)?;

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
            bound.extend(self.assign_leaf(leaf, &element.pattern, value, at)?);
        }

        // A REST TARGET TAKES EVERYTHING LEFT, and then the sequence is over -- so
        // nothing is owed and the close below is skipped entirely. That is not an
        // omission: the iterator reported `done` itself, which is the one way out that
        // closes nothing.
        if let Some(rest) = pattern.rest.as_deref() {
            let leaf = self.leaf(rest, at)?;
            let gathered = self.gather_rest(iterator, at)?;
            bound.extend(self.assign_leaf(leaf, rest, gathered, at)?);
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

    /// `[a, , b] = from` -- plain names, no default, no rest -- with an ARRAY whose
    /// protocol has nothing left to observe read by index, as `iterate.rs` walks one:
    /// what `next()` would answer at each position is the element there, or `undefined`
    /// past the end, and an array iterator owes no close. Anything else steps the
    /// protocol exactly as the general form does. Both paths hand the slot values to one
    /// block, and the names are bound there, once -- no default or target is evaluated
    /// between the steps, so nothing observable moves.
    fn destructure_names(
        &mut self,
        names: &[Option<Name>],
        from: ValueId,
        at: &Expr,
    ) -> Result<Vec<Name>, Unsupported> {
        // WHETHER THE PROTOCOL HAS ANYTHING LEFT TO OBSERVE: own elements, no proxy,
        // and the iterator and its `next` both primordial -- `ArrayPatternDirect`
        // reads the current state of all four. A replaced `next` is observed.
        let method = self.well_known(WellKnown::IteratorSymbol, from, at);
        let direct = self.entry(RuntimeOp::ArrayPatternDirect, vec![from], at);
        let direct = self.prim(JsPrim::Truthy, vec![direct], at);
        let indexed = self.builder.block();
        let stepped = self.builder.block();
        let joined = self.builder.block();
        self.builder.end(Terminator::Branch {
            condition: direct,
            then_block: indexed,
            then_args: Vec::new(),
            else_block: stepped,
            else_args: Vec::new(),
        });

        self.builder.switch_to(indexed);
        let mut values = Vec::new();
        for (position, slot) in names.iter().enumerate() {
            if slot.is_some() {
                let index = self.integer(position as i64, at);
                values.push(self.entry(RuntimeOp::ElementAt, vec![from, index], at));
            }
        }
        self.builder.end(Terminator::Jump {
            target: joined,
            args: values,
        });

        // THE PROTOCOL, the general form's steps without its bindings.
        self.builder.switch_to(stepped);
        let iterator = self.call_method(method, from, at);
        let mut values = Vec::new();
        let mut exhausted = None;
        for slot in names {
            let next = self.well_known(WellKnown::Next, iterator, at);
            let step = self.call_method(next, iterator, at);
            let done = self.well_known(WellKnown::Done, step, at);
            let ended = self.prim(JsPrim::Truthy, vec![done], at);
            exhausted = Some(ended);
            if slot.is_none() {
                continue;
            }
            let past = self.builder.block();
            let within = self.builder.block();
            let settled = self.builder.block();
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
                target: settled,
                args: vec![absent],
            });
            self.builder.switch_to(within);
            let held = self.well_known(WellKnown::Element, step, at);
            self.builder.end(Terminator::Jump {
                target: settled,
                args: vec![held],
            });
            self.builder.switch_to(settled);
            let value = self.builder.param(settled);
            self.types.insert(value, self.domain.top());
            values.push(value);
        }
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
            None => self.close_iterator(iterator, at),
        }
        self.builder.end(Terminator::Jump {
            target: joined,
            args: values,
        });

        self.builder.switch_to(joined);
        let mut bound = Vec::new();
        for name in names.iter().flatten() {
            let value = self.builder.param(joined);
            self.types.insert(value, self.domain.top());
            bound.extend(self.bind_leaf(*name, value, at)?);
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

    /// Where one element of a pattern goes, evaluated before its value is read.
    fn leaf(&mut self, pattern: &Pattern, at: &Expr) -> Result<Leaf, Unsupported> {
        let Pattern::Target(place) = pattern else {
            return Ok(Leaf::Binding);
        };
        Ok(match &place.kind {
            ExprKind::Ident(_) => Leaf::Binding,
            ExprKind::Member {
                object,
                property,
                optional: false,
            } => {
                let receiver = self.expression(object)?;
                let key = self.domain.constant(JsConst::Key(*property));
                Leaf::Field(receiver, self.declared(key, at))
            }
            ExprKind::Index {
                object,
                index,
                optional: false,
            } => {
                let receiver = self.expression(object)?;
                Leaf::Index(receiver, self.expression(index)?)
            }
            _ => {
                return Err(Unsupported::Expression(
                    "a pattern target that is neither a name nor a property",
                ));
            }
        })
    }

    /// Puts `value` where the element goes: a name bound, a property written, or a
    /// NESTED pattern taken apart again. Answers the names it bound.
    fn assign_leaf(
        &mut self,
        leaf: Leaf,
        pattern: &Pattern,
        value: ValueId,
        at: &Expr,
    ) -> Result<Vec<Name>, Unsupported> {
        match (leaf, pattern) {
            (Leaf::Field(receiver, key), _) => {
                self.prim(JsPrim::FieldWrite, vec![receiver, key, value], at);
                Ok(Vec::new())
            }
            (Leaf::Index(receiver, key), _) => {
                self.prim(JsPrim::IndexWrite, vec![receiver, key, value], at);
                Ok(Vec::new())
            }
            (Leaf::Binding, Pattern::Name(name)) => self.bind_leaf(*name, value, at),
            (Leaf::Binding, Pattern::Target(place)) => match &place.kind {
                ExprKind::Ident(name) => self.bind_leaf(*name, value, at),
                _ => Err(Unsupported::Pattern),
            },
            (Leaf::Binding, nested) => self.destructure(nested, value, at),
        }
    }

    fn bind_leaf(&mut self, name: Name, value: ValueId, at: &Expr) -> Result<Vec<Name>, Unsupported> {
        let of = self.type_of(value);
        let target = Expr {
            kind: ExprKind::Ident(name),
            at: at.at,
        };
        self.bind(name, value, of, &target)?;
        Ok(vec![name])
    }
}

/// Where one element of a pattern goes, once its reference is evaluated.
enum Leaf {
    /// A name, or a nested pattern -- nothing to evaluate ahead of the value.
    Binding,
    /// `o.k`, its receiver and key already evaluated.
    Field(ValueId, ValueId),
    /// `o[k]`, likewise.
    Index(ValueId, ValueId),
}
