//! `Array.from(items, mapFn, thisArg)`.
//!
//! # Why it left [`super`]
//!
//! Because it is the only method here that reads something which is **not** an
//! array, and doing that correctly is three separate rules: which values the
//! iteration protocol already answers, `ToLength` of a `length` an object merely
//! claims, and `Get` of every index below it. Those had been written as one
//! function that walked the shape directly, and each rule was wrong in its own
//! way — `{length: "3"}` was refused for not being a number, and an index defined
//! by a getter answered `undefined` without the getter ever running.
//!
//! The reader itself is [`super::super::like`], shared with `concat`, because
//! spreading a `Symbol.isConcatSpreadable` object is the same operation. Two
//! copies is what there were, and they disagreed about both halves.
//!
//! # Why this walks an iterator itself, when [`crate::entry::iterate`] exists
//!
//! Because they are different operations rather than two answers to one
//! question. That module answers "the whole sequence, as an array", which is
//! exactly what a spread and a `yield*` need and is why it materialises. This
//! function with a mapper needs the opposite: the mapper runs BETWEEN two pulls,
//! so a mapper that throws must stop `next()` from being called again and must
//! tell the iterator the walk ended — `IteratorClose`, which is only expressible
//! while the cursor is still open.
//!
//! Draining first and mapping afterwards is what this did, and it is observable
//! in two ways at once: the iterator ran to exhaustion no matter where the
//! mapper failed, and its `return()` was never called. So the walk is here, and
//! [`Source`] is what keeps it from becoming a second answer to "what is
//! iterable" — only a value whose iterator is the PROGRAM'S is stepped, and
//! everything this engine walks itself still goes through the one module that
//! decides what those are.

use super::super::super::objects::undefined_of;
use super::super::super::{Context, functions, throw, with_current};
use super::super::{built, iterate as array_iterate, like};
use super::{calls, nothing, snapshot};
use crate::text::Str;
use crate::value::Value;

/// `Array.from(items, mapFn, thisArg)`.
///
/// Through the runtime's own iteration, so anything `for-of` walks this accepts
/// — including a string, which becomes one element per code point rather than
/// per unit.
///
/// # Why the array-like fallback is here and not in `iterate`
///
/// `Array.from({length: 2})` is `[undefined, undefined]` and
/// `for (const x of {length: 2})` is a `TypeError`: the language reads indices
/// off a `length` **only for this function**. Putting the fallback in `iterate`
/// would make `for-of` accept what the language refuses, which is a wrong
/// program that runs. So `iterate` still decides what is *iterable*, and this
/// decides what it additionally accepts when nothing was iterated.
pub(in crate::entry) extern "C" fn from(
    _e: u64,
    _this: u64,
    items: u64,
    mapper: u64,
    receiver: u64,
    _a3: u64,
) -> u64 {
    // Outside every borrow: each branch below runs user code — a `length`
    // getter, a `Symbol.iterator` method, or the mapper.
    match source(items) {
        Source::Like => {
            let produced = built(like::values_of(items));
            if throw::in_flight() {
                return nothing();
            }
            match calls(mapper) {
                true => mapped(produced, mapper, receiver),
                // Already a fresh array, so there is nothing to build twice.
                false => produced,
            }
        }
        // Interleaved ONLY when there is both a mapper to fail and an iterator
        // of the program's to close. Without a mapper nothing runs between two
        // pulls, so draining in one step is the operation rather than a
        // divergence — and it costs one allocation instead of two calls per
        // element.
        Source::Stepped if calls(mapper) => walked(items, mapper, receiver),
        Source::Stepped | Source::Primordial => {
            let produced = super::super::super::iterate::iterate(items);
            if throw::in_flight() {
                return nothing();
            }
            match calls(mapper) {
                true => mapped(produced, mapper, receiver),
                false => produced,
            }
        }
    }
}

/// How `Array.from` reads its argument.
enum Source {
    /// By its `length`, then `Get` of every index below it.
    Like,
    /// By an iterator of this engine's own — an array, a string, a `Map` or a
    /// `Set` — which runs no user code between two pulls, so there is nothing
    /// for a mapper to interleave with and nothing to close.
    Primordial,
    /// By the PROGRAM's `Symbol.iterator`, one pull at a time.
    Stepped,
}

/// Which of the three this value is.
///
/// One function and not three tests, because they partition: a value that is
/// read by `length` is exactly one that neither this engine nor the program
/// knows how to iterate, and two separate spellings of that is where one of them
/// learns about `Symbol.iterator` and the other does not.
///
/// The `length` itself is not read here. `ToLength` and `Get` are both
/// operations that may run user code, so both live in [`like::values_of`] —
/// which means a non-iterable object always takes the array-like path and an
/// absent `length` answers zero elements there, exactly as `ToLength(undefined)`
/// says.
fn source(items: u64) -> Source {
    with_current(|context| {
        let Some(cell) = Value(items).as_slot() else {
            return Source::Like;
        };
        if context.elements_at(cell).is_some()
            || context.text_at(cell).is_some()
            || super::super::super::collections::iterated(context, cell).is_some()
        {
            return Source::Primordial;
        }
        match declares_iterator(context, cell) {
            true => Source::Stepped,
            false => Source::Like,
        }
    })
}

/// Whether this value is read by its `length` rather than by iterating it.
///
/// Shared with [`super::from_async`] rather than copied, which is what the
/// visibility is for: `Array.from({length: 2})` and `Array.fromAsync({length:
/// 2})` must agree about what an array-like is, and two spellings of this test is
/// where one of them learns about `Symbol.iterator` and the other does not. A
/// thin reading of [`source`] and not a second test, for that same reason.
pub(super) fn array_like(items: u64) -> bool {
    matches!(source(items), Source::Like)
}

/// Whether an object declares how to be iterated, anywhere along its chain.
///
/// A data read: a getter on `Symbol.iterator` would be user code and this is
/// inside a borrow — the same boundary [`super::super::super::iterate`] draws
/// for `next` and `done`.
fn declares_iterator(context: &mut Context, cell: u32) -> bool {
    let key = context.well_known(&format!(
        "{}iterator",
        super::super::super::symbol::PREFIX
    ));
    match super::super::super::objects::read_property(context, cell, key) {
        // Present but `undefined` is the language's own way of saying "not
        // iterable after all", and it has to read as absent here or an object
        // that deliberately erased an inherited iterator would be refused
        // rather than read by its `length`.
        Some(found) => found.bits() != undefined_of(context),
        None => false,
    }
}

/// The mapper over an array already in hand.
///
/// ROOTED, and written as a loop rather than a `collect` for that reason: the
/// mapper is user code that allocates, an allocation collects, and what it has
/// already answered would otherwise live only in a `Vec` on the Rust heap, which
/// no scan of ours reaches. Same hole as `map`'s, same mechanism — see
/// `entry::rooted`.
fn mapped(produced: u64, mapper: u64, receiver: u64) -> u64 {
    let Some(elements) = snapshot(produced) else {
        return nothing();
    };
    let mut mapped = super::super::super::rooted::Rooted::new();
    for (index, element) in elements.iter().enumerate() {
        // `visit` is `map`'s call, and it is shared rather than repeated so that
        // the receiver, the argument order and the rule-8 check are decided in
        // one place. `thisArg` is this function's THIRD argument, which used to
        // be read as padding and thrown away.
        let Some(answered) = array_iterate::visit(mapper, receiver, produced, *element, index)
        else {
            break;
        };
        mapped.values().push(answered);
    }
    built(mapped.take())
}

/// One pull, one mapper call, until the iterator says it is done.
///
/// The array the mapper's third argument names does not exist yet, and that is
/// the language rather than an omission: the specification calls the mapper with
/// the value and its position only. It is passed as absent for that reason,
/// through the same [`array_iterate::visit`] the other path uses so the rule-8
/// check has one spelling.
fn walked(items: u64, mapper: u64, receiver: u64) -> u64 {
    let Some((iterator, next)) = opened(items) else {
        // Either the method is not callable or it threw. `iterate` owns the
        // `TypeError` for the first and re-derives it from a data read that
        // runs nothing twice; for the second the throw is already in flight and
        // raising a second would replace the program's own error.
        return match throw::in_flight() {
            true => nothing(),
            false => super::super::super::iterate::iterate(items),
        };
    };
    let absent = nothing();
    let mut produced = super::super::super::rooted::Rooted::new();
    let mut index = 0usize;
    loop {
        let step = functions::call(next, iterator, absent, absent, absent, absent);
        // Rule 8. Without it `done` reads `undefined`, which is never true, and
        // this loop fills a vector until the process dies.
        if throw::in_flight() {
            return nothing();
        }
        // `done` before `value`, which is the order the specification states —
        // an iterator whose `done` getter has a side effect observes it first.
        if super::super::super::primitives::to_boolean(read(step, "done")) {
            break;
        }
        let element = read(step, "value");
        if throw::in_flight() {
            return nothing();
        }
        let Some(answered) = array_iterate::visit(mapper, receiver, absent, element, index) else {
            // The mapper threw, and the iterator is still open. This is the
            // whole reason the walk is here rather than in a drain: it can be
            // told the sequence ended early.
            closed(iterator);
            return nothing();
        };
        produced.values().push(answered);
        index += 1;
    }
    built(produced.take())
}

/// The iterator a value's own `Symbol.iterator` answers, and its `next`.
///
/// `None` when there is nothing to step — which [`walked`] separates from a
/// throw by asking, because the two need opposite answers.
fn opened(items: u64) -> Option<(u64, u64)> {
    let method = read(items, &format!("{}iterator", super::super::super::symbol::PREFIX));
    if !calls(method) {
        return None;
    }
    let absent = nothing();
    let iterator = functions::call(method, items, absent, absent, absent, absent);
    if throw::in_flight() {
        return None;
    }
    let next = read(iterator, "next");
    match calls(next) {
        true => Some((iterator, next)),
        false => None,
    }
}

/// `IteratorClose` — tells an iterator that the walk stopped before the end.
///
/// The throw that STOPPED it is set aside first. The specification says that
/// completion wins over anything `return()` itself throws, and this engine holds
/// one thrown value — so without the save a `return` that failed would replace
/// the program's own error with its own, and the `catch` written for the first
/// would see the second.
fn closed(iterator: u64) {
    let held = super::super::super::current::take_thrown_slot();
    let method = read(iterator, "return");
    if calls(method) {
        let absent = nothing();
        functions::call(method, iterator, absent, absent, absent, absent);
    }
    super::super::super::current::set_thrown(held);
}

/// One property, as the specification's `Get`.
///
/// Through `get_indexed` rather than a shape read, because `IteratorComplete`
/// and `IteratorValue` are both defined as `Get`: an iterator whose `done` is an
/// accessor is unusual and legal, and a shape read answers `undefined` for it
/// without the getter ever running.
fn read(value: u64, name: &str) -> u64 {
    let key = with_current(|context| context.intern_value(Str::from_str(name)).bits());
    super::super::super::computed::get_indexed(value, key)
}
