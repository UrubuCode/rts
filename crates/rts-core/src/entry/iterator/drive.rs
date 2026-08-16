//! Stepping an iterator from Rust, one element at a time.
//!
//! # Why this exists beside [`crate::entry::iterate`]
//!
//! `iterate` answers the WHOLE sequence as an array, and says in its own first
//! page why: every caller it has is going to hold every element anyway. The
//! ES2025 iterator helpers are the opposite requirement — `naturals().take(3)`
//! over an endless generator must pull exactly three times — so they cannot be
//! written over a function whose contract is "drain it".
//!
//! What is NOT duplicated is the protocol's meaning. `iterate::member` and
//! `iterate::callable` are reused; the one thing added here is a read that runs
//! a GETTER, because `{ get done() { … } }` is a shape the helper fixtures use
//! and a data read answers `undefined` for it — which is never `true`, which is
//! a loop that does not end.

use crate::entry::{functions, iterate, objects, primitives, throw, with_current};
use crate::value::Value;

/// An iterator and the `next` that was cached when it was adopted.
///
/// The specification's `GetIteratorDirect` records the method once, at the
/// moment the helper is built, and every later step calls THAT — not whatever
/// `next` reads as by then. A program that replaces `it.next` after building a
/// helper sees the original, and holding the pair is what says so.
#[derive(Clone, Copy)]
pub(in crate::entry) struct Source {
    /// The iterator object itself, which is the receiver of every step.
    pub(in crate::entry) object: u64,
    /// The `next` it had when it was adopted.
    pub(in crate::entry) next: u64,
}

/// What one step answered.
pub(in crate::entry) enum Step {
    /// The sequence ended.
    Done,
    /// One element.
    Value(u64),
}

/// The pair the specification's `GetIteratorDirect` records.
pub(in crate::entry) fn direct(object: u64) -> Source {
    Source {
        object,
        next: read(object, "next"),
    }
}

/// `undefined`, for an argument nothing was given for.
pub(in crate::entry) fn absent() -> u64 {
    with_current(|context| objects::undefined_of(context))
}

/// One property of a value, **getters run**.
///
/// Through `objects::get_property`, which is the crate's one property read —
/// so a `done` behind an inherited getter or a proxy trap answers here the way
/// it answers everywhere else in the program.
pub(in crate::entry) fn read(object: u64, name: &str) -> u64 {
    let key = with_current(|context| {
        let key = context.well_known(name);
        objects::machine_key(key).map(|key| key.index() as i64)
    });
    match key {
        Some(key) => objects::get_property(object, key),
        None => absent(),
    }
}

/// Whether a value is an OBJECT — not a primitive, and not a string.
///
/// A string is a cell here, so `as_slot` alone answers `true` for one. The
/// distinction is program-visible in two places this module cares about: an
/// iterator result must be an object, and `flatMap` refuses a primitive.
pub(in crate::entry) fn is_object(value: u64) -> bool {
    with_current(|context| match Value(value).as_slot() {
        Some(cell) => context.text_at(cell).is_none(),
        None => false,
    })
}

/// One step of an iterator.
///
/// `None` means a throw is in flight — either the callee's or this function's
/// own — and the caller returns without looking at anything, which is rule 8 of
/// the crate's README applied at the one place every helper passes through.
pub(in crate::entry) fn step(source: &Source) -> Option<Step> {
    if !iterate::callable(source.next) {
        throw::type_error("the iterator has no callable next()");
        return None;
    }
    let nothing = absent();
    let result = functions::call(source.next, source.object, nothing, nothing, nothing, nothing);
    if throw::in_flight() {
        return None;
    }
    if !is_object(result) {
        throw::type_error("the iterator answered a result that is not an object");
        return None;
    }
    // `done` before `value`, which is the order the specification states: an
    // iterator whose `done` getter has a side effect observes it first.
    let done = read(result, "done");
    if throw::in_flight() {
        return None;
    }
    if primitives::to_boolean(done) {
        return Some(Step::Done);
    }
    let value = read(result, "value");
    if throw::in_flight() {
        return None;
    }
    Some(Step::Value(value))
}

/// Closes an iterator: its `return`, if it has a callable one.
///
/// An iterator with none is closed by doing nothing, which is what the
/// specification says and what a `for`-`of` over a bare `{ next }` relies on.
pub(in crate::entry) fn close(object: u64) {
    let method = read(object, "return");
    if throw::in_flight() {
        return;
    }
    if !iterate::callable(method) {
        return;
    }
    let nothing = absent();
    functions::call(method, object, nothing, nothing, nothing, nothing);
}

/// `IfAbruptCloseIterator` — close the iterator, and keep the ORIGINAL throw.
///
/// The order matters and so does the taking. A throw is already in flight when
/// this is reached, and every read [`close`] makes returns early while one is —
/// so closing without setting the throw aside does nothing at all, which is
/// exactly what a generator's `finally` NOT running looked like in four
/// fixtures.
///
/// A `return` that throws while closing is discarded: the specification says
/// the original completion wins, and swapping them would replace the program's
/// own error with whatever cleanup found.
pub(in crate::entry) fn close_abrupt(object: u64) {
    let Some(thrown) = throw::caught() else {
        close(object);
        return;
    };
    close(object);
    let _ = throw::caught();
    throw::throw_value(thrown);
}

/// The `{ value, done }` a step answers.
pub(in crate::entry) fn result(value: u64, done: bool) -> u64 {
    with_current(|context| crate::entry::generator::result(context, value, done))
}

/// `{ undefined, done: true }` — what an exhausted iterator answers forever.
pub(in crate::entry) fn finished() -> u64 {
    with_current(|context| {
        let absent = objects::undefined_of(context);
        crate::entry::generator::result(context, absent, true)
    })
}
