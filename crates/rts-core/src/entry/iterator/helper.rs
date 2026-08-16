//! An Iterator Helper: the LAZY object `it.map(f)` and its four siblings answer.
//!
//! # Why it holds an operation rather than a list
//!
//! Because the difference is observable and the fixtures are written on it.
//! `naturals().map(f).filter(p).take(3)` over an endless generator must build
//! without pulling anything, pull exactly as far as each `next()` needs, and
//! close the source the moment `take`'s budget runs out. A helper that answered
//! a materialised list — which is what this engine did until now — hangs on the
//! first of those and is wrong about the other two.
//!
//! # Why the state is beside the cell rather than in it
//!
//! The placement `Map`'s table and a generator's frame already have, for the
//! reason `generator::State` states: the object's own slots stay ordinary
//! properties, so a program that hangs a field on a helper is not fighting the
//! runtime for room. The alternative — hidden `@@` properties — would put the
//! operation somewhere `Object.getOwnPropertySymbols` can see it.

use super::drive::{self, Source, Step};
use crate::entry::{Context, class_support, functions, iterate, native, objects, symbol, throw};
use crate::entry::{with_current};
use crate::value::Value;

/// What a helper does to the sequence beneath it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::entry) enum Kind {
    /// `it.map(f)`.
    Map,
    /// `it.filter(p)`.
    Filter,
    /// `it.take(n)`.
    Take,
    /// `it.drop(n)`.
    Drop,
    /// `it.flatMap(f)`.
    FlatMap,
}

/// One helper, and everything a step of it needs.
#[derive(Clone)]
pub(in crate::entry) struct Helper {
    /// What it pulls from.
    source: Source,
    /// What it does with what it pulls.
    kind: Kind,
    /// The callback, for the three kinds that have one.
    callback: u64,
    /// What is left of a `take` or a `drop` budget.
    budget: f64,
    /// The sequence a `flatMap` is in the middle of, while it is in one.
    inner: Option<Source>,
    /// The counter the callback's second argument carries.
    index: f64,
    /// Whether it has finished. A helper that has ended stays ended.
    done: bool,
}

impl Helper {
    /// Everything it holds that is a value, for the tracer.
    pub(in crate::entry) fn trace(&self, out: &mut Vec<u64>) {
        out.push(self.source.object);
        out.push(self.source.next);
        out.push(self.callback);
        if let Some(inner) = self.inner {
            out.push(inner.object);
            out.push(inner.next);
        }
    }
}

/// The `Symbol.toStringTag` every helper carries — one string for all five,
/// which is what the specification gives them.
pub(in crate::entry) const TAG: &str = "Iterator Helper";

/// A helper over `source`, doing `kind`.
pub(in crate::entry) fn over(source: Source, kind: Kind, callback: u64, budget: f64) -> u64 {
    let state = Helper {
        source,
        kind,
        callback,
        budget,
        inner: None,
        index: 0.0,
        done: false,
    };
    with_current(|context| {
        let Some(prototype) = prototype_of(context) else {
            return objects::undefined_of(context);
        };
        let Some(cell) = native::plain(context) else {
            return objects::undefined_of(context);
        };
        context.set_prototype(cell, prototype);
        context.helpers.set(cell, state);
        Value::from_slot(cell).bits()
    })
}

/// What every helper inherits from, registering it if nothing has yet.
///
/// Lazily, the way `array_proto::cursor::over` registers its own: this class is
/// not a global name, so `entry::global` has no row that would ever reach it.
fn prototype_of(context: &mut Context) -> Option<u64> {
    if let Some(prototype) = class_support::prototype(context, "IteratorHelper") {
        return Some(prototype);
    }
    super::register_helper_prototype(context);
    class_support::prototype(context, "IteratorHelper")
}

/// The state of a helper, if `this` is one.
fn state_of(this: u64) -> Option<Helper> {
    with_current(|context| {
        let cell = Value(this).as_slot()?;
        context.helpers.get(cell).cloned()
    })
}

/// Writes a helper's state back after a step ran user code.
fn store(this: u64, state: Helper) {
    with_current(|context| {
        if let Some(cell) = Value(this).as_slot() {
            context.helpers.set(cell, state);
        }
    });
}

/// The brand check every member of the helper prototype makes first.
fn branded(this: u64) -> Option<Helper> {
    match state_of(this) {
        Some(state) => Some(state),
        None => {
            throw::type_error("the receiver is not an Iterator Helper");
            None
        }
    }
}

/// The helper prototype's two members.
#[rtse::class("IteratorHelper")]
impl IteratorHelper {
    /// `h.next()` — pull until this helper has an element, or until it ends.
    fn next(this: u64) -> u64 {
        let Some(state) = branded(this) else {
            return drive::absent();
        };
        if state.done {
            return drive::finished();
        }
        step(this, state)
    }

    /// `h.return()` — end the helper, and close everything beneath it.
    ///
    /// The argument is ignored, which is what the specification says: an
    /// Iterator Helper's `return` answers `{ undefined, true }` whatever it was
    /// handed. A generator's does not, and the difference is deliberate — a
    /// helper has no body to resume with a value.
    #[js("return")]
    fn returned(this: u64) -> u64 {
        let Some(mut state) = branded(this) else {
            return drive::absent();
        };
        if state.done {
            return drive::finished();
        }
        state.done = true;
        store(this, state.clone());
        if let Some(inner) = state.inner {
            drive::close(inner.object);
        }
        drive::close(state.source.object);
        if throw::in_flight() {
            return drive::absent();
        }
        drive::finished()
    }
}

/// `%WrapForValidIteratorPrototype%` — what `Iterator.from` answers for an
/// object that has a `next` but does not inherit `Iterator.prototype`.
///
/// It forwards `next` and `return` VERBATIM rather than through
/// [`drive::step`]: the specification's wrapper answers exactly what the source
/// answered, so a source whose result is malformed is the caller's problem
/// where it would be this object's if the result were rebuilt here.
#[rtse::class("IteratorWrapper")]
impl IteratorWrapper {
    /// `w.next()` — the source's own answer, untouched.
    fn next(this: u64) -> u64 {
        let Some(state) = branded(this) else {
            return drive::absent();
        };
        let nothing = drive::absent();
        functions::call(
            state.source.next,
            state.source.object,
            nothing,
            nothing,
            nothing,
            nothing,
        )
    }

    /// `w.return()` — the source's `return`, or `{ undefined, true }` when it
    /// has none.
    #[js("return")]
    fn returned(this: u64) -> u64 {
        let Some(state) = branded(this) else {
            return drive::absent();
        };
        let method = drive::read(state.source.object, "return");
        if throw::in_flight() {
            return drive::absent();
        }
        if !iterate::callable(method) {
            return drive::finished();
        }
        let nothing = drive::absent();
        functions::call(method, state.source.object, nothing, nothing, nothing, nothing)
    }
}

/// A wrapper over a source that is not already an `Iterator`.
pub(in crate::entry) fn wrap(source: Source) -> u64 {
    let state = Helper {
        source,
        kind: Kind::Map,
        callback: drive::absent(),
        budget: 0.0,
        inner: None,
        index: 0.0,
        done: false,
    };
    with_current(|context| {
        if class_support::prototype(context, "IteratorWrapper").is_none() {
            register_iterator_wrapper(context);
            super::adopt(context);
        }
        let Some(prototype) = class_support::prototype(context, "IteratorWrapper") else {
            return objects::undefined_of(context);
        };
        let Some(cell) = native::plain(context) else {
            return objects::undefined_of(context);
        };
        context.set_prototype(cell, prototype);
        context.helpers.set(cell, state);
        Value::from_slot(cell).bits()
    })
}

/// One element out of a helper, pulling as far as it must and no further.
fn step(this: u64, mut state: Helper) -> u64 {
    loop {
        match state.kind {
            Kind::Take if state.budget <= 0.0 => {
                return exhausted(this, state);
            }
            Kind::Drop => {
                while state.budget > 0.0 {
                    state.budget -= 1.0;
                    store(this, state.clone());
                    match drive::step(&state.source) {
                        None => return abandoned(this, state),
                        Some(Step::Done) => return ended(this, state),
                        Some(Step::Value(_)) => {}
                    }
                }
            }
            Kind::FlatMap => {
                if let Some(inner) = state.inner {
                    match drive::step(&inner) {
                        // The INNER sequence failed, so the outer one is still
                        // live and the specification closes it. That is the
                        // difference between this arm and the one below, where
                        // the source itself is what threw and has closed
                        // itself already.
                        None => return closed(this, state),
                        Some(Step::Value(value)) => {
                            store(this, state.clone());
                            return drive::result(value, false);
                        }
                        Some(Step::Done) => {
                            state.inner = None;
                            store(this, state.clone());
                            continue;
                        }
                    }
                }
            }
            _ => {}
        }

        let pulled = match drive::step(&state.source) {
            None => return abandoned(this, state),
            Some(Step::Done) => return ended(this, state),
            Some(Step::Value(value)) => value,
        };
        let index = state.index;
        state.index += 1.0;
        if state.kind == Kind::Take {
            state.budget -= 1.0;
        }
        store(this, state.clone());

        match state.kind {
            Kind::Take | Kind::Drop => return drive::result(pulled, false),
            Kind::Map => {
                let mapped = call(state.callback, pulled, index);
                if throw::in_flight() {
                    return closed(this, state);
                }
                return drive::result(mapped, false);
            }
            Kind::Filter => {
                let kept = call(state.callback, pulled, index);
                if throw::in_flight() {
                    return closed(this, state);
                }
                if crate::entry::primitives::to_boolean(kept) {
                    return drive::result(pulled, false);
                }
            }
            Kind::FlatMap => {
                let mapped = call(state.callback, pulled, index);
                if throw::in_flight() {
                    return closed(this, state);
                }
                let Some(inner) = flattenable(mapped) else {
                    return closed(this, state);
                };
                state.inner = Some(inner);
                store(this, state.clone());
            }
        }
    }
}

/// One call of a helper's callback: the element and its position.
fn call(callback: u64, element: u64, index: f64) -> u64 {
    let nothing = drive::absent();
    let counted = Value::from_f64(index).bits();
    functions::call(callback, nothing, element, counted, nothing, nothing)
}

/// The source ran out: the helper ends, and nothing is closed.
///
/// An iterator that answered `done` has already closed itself — the language
/// says so, and calling its `return` on the way past would run a generator's
/// `finally` a second time.
fn ended(this: u64, mut state: Helper) -> u64 {
    state.done = true;
    store(this, state);
    drive::finished()
}

/// A `take` budget ran out while the source is still live, so the source is
/// CLOSED — which is what makes `naturals().take(0)` run the generator's
/// `finally` without ever entering its body.
fn exhausted(this: u64, mut state: Helper) -> u64 {
    state.done = true;
    store(this, state.clone());
    drive::close(state.source.object);
    if throw::in_flight() {
        return drive::absent();
    }
    drive::finished()
}

/// A callback threw: `IfAbruptCloseIterator` — close the source, propagate.
fn closed(this: u64, mut state: Helper) -> u64 {
    state.done = true;
    store(this, state.clone());
    drive::close_abrupt(state.source.object);
    drive::absent()
}

/// The source itself threw: it is closing itself, so nothing is closed here.
fn abandoned(this: u64, mut state: Helper) -> u64 {
    state.done = true;
    store(this, state);
    drive::absent()
}

/// `GetIteratorFlattenable(v, reject-primitives)` — what `flatMap` may flatten.
///
/// A primitive is refused, string included: `[1].values().flatMap(x => "ab")`
/// is a `TypeError` and not two characters, which is the one place the helper
/// differs from `Array.prototype.flatMap`'s notion of flattening.
fn flattenable(value: u64) -> Option<Source> {
    if !drive::is_object(value) {
        throw::type_error("flatMap: the callback answered something that is not an object");
        return None;
    }
    let method = drive::read(value, &format!("{}iterator", symbol::PREFIX));
    if throw::in_flight() {
        return None;
    }
    let nothing = drive::absent();
    let iterator = match iterate::callable(method) {
        false => value,
        true => {
            let answered = functions::call(method, value, nothing, nothing, nothing, nothing);
            if throw::in_flight() {
                return None;
            }
            if !drive::is_object(answered) {
                throw::type_error("flatMap: Symbol.iterator answered something that is not an object");
                return None;
            }
            answered
        }
    };
    Some(drive::direct(iterator))
}
