//! Whether a destructuring pattern may read its source directly.
//!
//! # The one question, and why the runtime answers it rather than the emitter
//!
//! `const [x, y] = source` must, by the specification, call
//! `source[Symbol.iterator]()` and then step it. Stepping is what
//! `emit/destructure/array.rs` emits, and it is expensive in a way the emitted
//! code cannot see: three property reads, a call and a `{ value, done }`
//! ALLOCATION per position, plus a prologue that fetches the method, asks
//! whether it is callable, and invokes it.
//!
//! For a source that is an ordinary array whose iterator is the primordial one,
//! reading `source[0]`, `source[1]` … is indistinguishable from that — in this
//! engine more plainly than in the specification, because `array_proto::cursor`
//! already reads `elements.len()` and `elements[i]` straight from the vector.
//! The two forms already execute the same two reads; what differs is the
//! protocol wrapped around them.
//!
//! "Indistinguishable" is a conjunction of four facts, and every one of them is
//! about state a program can change. So the question is asked HERE, where the
//! context is already in hand, rather than emitted as four comparisons the
//! generated code would pay for separately — `emit/foreach.rs` asks its own
//! version the other way, and pays a `Symbol.iterator` read, an `ArrayNew` and
//! an identity comparison at every loop it guards. Measured on this tree, that
//! shape costs 203 ns, of which 66 ns is the `[]` allocated only to read a
//! method off it. Once per loop that is nothing; once per destructuring it is
//! most of what the fast path was going to save.
//!
//! # Why this is not a protector cell
//!
//! The obvious cheaper design remembers "nothing has been patched" in one bit
//! and invalidates it on write. That bit needs a hook at every point that can
//! write `Array.prototype[Symbol.iterator]`, `%ArrayIteratorPrototype%.next`,
//! or a `return` onto either prototype — and a point that is forgotten is not a
//! slow answer, it is a WRONG one, silently, in a construct that appears in
//! almost every file. So nothing is cached: the two primordials are recorded
//! where they are installed, which is the only moment they are knowably
//! primordial, and every question below reads the CURRENT state.
//!
//! What that costs is a handful of shape-slot lookups inside a single crossing,
//! against the four crossings and the allocation the emitted form would need.


use crate::value::Value;

use super::current::with_current;
use super::{Context, class_support, objects, symbol};

/// Whether an array pattern may read `source` by index instead of stepping it.
///
/// Answers a boolean value, never a machine boolean: the emitter branches on it
/// through the ordinary truthiness path, so it is an ordinary operand.
///
/// Conservative in one direction only. A `false` costs the stepping this exists
/// to avoid and is always correct; a `true` is a claim that the protocol has no
/// observable effect left to have, and every clause below is one way that claim
/// could fail.
#[rtse::entry]
pub fn array_pattern_direct(source: u64) -> u64 {
    with_current(|context| Value::from_bool(direct(context, source)).bits())
}

/// The four facts, cheapest first.
fn direct(context: &mut Context, source: u64) -> bool {
    // 1. A reference that holds its OWN elements.
    //
    // This one clause excludes most of what the other three would have to: an
    // array-like with a `length` property and no vector, `Object.create([1,2])`
    // — which iterates through the INHERITED method over the child, and whose
    // elements this engine cannot reach by index anyway — a string, a typed
    // array, a `Map`, and anything that is not a reference at all.
    let Some(cell) = Value(source).as_slot() else {
        return false;
    };
    if context.elements_at(cell).is_none() {
        return false;
    }

    // 2. Not a proxy.
    //
    // A proxy over an array holds no elements of its own, so clause 1 already
    // refuses it — this is here because that is a coincidence of how proxies are
    // represented and not a thing to depend on, and because the refusal has to
    // survive a proxy that ever comes to carry them.
    //
    // `Context::proxy_at` and not `proxy::is_proxy`, and the difference is not
    // style: the free function takes no context and borrows one itself, so
    // calling it from inside this borrow is a reentrant `RefCell` borrow, which
    // in an `extern "C"` frame cannot unwind and aborts. The falsifier caught it
    // on the first run.
    if context.proxy_at(cell).is_some() {
        return false;
    }

    // 3. Its `Symbol.iterator` is the one the array prototype was built with.
    //
    // Both halves are needed and neither implies the other: an own property on
    // the receiver replaces the method for that array alone, and a write to
    // `Array.prototype` replaces it for every array that has no own one.
    let iterator = context.well_known(symbol::ITERATOR);
    if objects::own_property(context, cell, iterator).is_some() {
        return false;
    }
    let Some(installed) = context.array_iterator_method else {
        // The prototype has never been built, so no array has an iterator to
        // step and nothing could have taken this path anyway.
        return false;
    };
    let Some(prototype) = context.array_prototype else {
        return false;
    };
    match objects::own_property(context, prototype, iterator) {
        Some(found) if found.bits() == installed => {}
        _ => return false,
    }

    // 4. The step this skips is still the step the specification would run.
    //
    // `%ArrayIteratorPrototype%.next` replaced, or a `return` added to it or to
    // the `%IteratorPrototype%` above it, both make the protocol observable
    // again: the first because the pattern would have called the replacement,
    // the second because abandoning a pattern early performs `IteratorClose`,
    // which is a no-op ONLY while there is no `return` to find. Primordially
    // there is none — `%ArrayIteratorPrototype%` carries `next` and a tag, and
    // nothing else.
    cursor_step_is_primordial(context)
}

/// Whether the array cursor still steps and closes the way it was built to.
fn cursor_step_is_primordial(context: &mut Context) -> bool {
    let Some(cell) = context.array_cursor_prototype else {
        // Never registered, so no cursor has ever existed and there has been
        // nothing to replace. This is the ordinary case for a program that
        // destructures without ever asking an array for an iterator by name,
        // and answering it by NOT LOOKING is the point: the question used to go
        // to `class_support::prototype`, which walks `classes` comparing a
        // string per entry, so the program that never makes a cursor paid the
        // whole walk on every pattern to be told the class is absent.
        return true;
    };

    let next = context.well_known("next");
    match (
        objects::own_property(context, cell, next),
        context.array_cursor_next,
    ) {
        (Some(found), Some(installed)) if found.bits() == installed => {}
        _ => return false,
    }

    if carries_return(context, cell) {
        return false;
    }
    match class_support::prototype(context, "Iterator") {
        Some(above) => match Value(above).as_slot() {
            Some(above) => !carries_return(context, above),
            None => false,
        },
        None => true,
    }
}

/// Whether an object has an own `return`, which `IteratorClose` would find.
fn carries_return(context: &mut Context, cell: u32) -> bool {
    let key = context.well_known("return");
    objects::own_property(context, cell, key).is_some()
}
