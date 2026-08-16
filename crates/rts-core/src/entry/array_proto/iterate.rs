//! The array methods that call user code.
//!
//! # Why every one of these is written in two stages
//!
//! `with_current` holds a `RefCell` borrow for as long as its body runs, and the
//! callback is user code whose very first act may be to call the runtime. Calling
//! it from inside the borrow re-enters the cell — which is not a wrong answer but
//! a hang, and this repository has already paid for it once in a different shard.
//!
//! So each method collects what it needs inside a borrow, **lets the borrow go**,
//! calls, and re-borrows to store. That is the shape
//! `super::super::string::pattern::replaced` was written in, for the same reason,
//! and it is why these eight are in a file of their own: the eleven in
//! [`super`] read and answer inside one borrow, and the two shapes sitting
//! together is how one gets copied for the other.
//!
//! # Why the LENGTH is a snapshot and the ELEMENTS are not
//!
//! The specification fixes the visited range when the method starts — a callback
//! that pushes does not extend the loop — and then reads every element with an
//! ordinary `Get` at the moment it is visited. Those are two different decisions,
//! and this file used to make them both by copying the whole vector once: a
//! callback that wrote `src[3] = 40` was ignored, and one that filled a hole it
//! had not reached yet was ignored too.
//!
//! So [`len_of`] is taken once and [`existing`] re-reads per index. The borrow
//! that re-read OPENS AND CLOSES BETWEEN calls and never crosses one, which is
//! what makes this legal at all — it is the same rule the two-stage shape above
//! states, applied per iteration instead of once.
//!
//! The cost, named and NOT measured: a method that visits N elements now takes N
//! borrows where it took one clone of the whole vector. Which of those is dearer
//! depends on N and has not been timed, so nothing here claims a direction — what
//! is claimed is that the clone answers the mutation questions wrongly and this
//! answers them, and every element visited already costs a call into user code
//! beside either.
//!
//! # Why `this` for the callback is the second argument
//!
//! Because that is what `thisArg` is, and it used to be DROPPED: the slot was
//! read as padding and every callback ran with `undefined` as its receiver. A
//! method written as `xs.map(function (x) { return x * this.factor; }, obj)` then
//! failed inside the callback rather than at the call, which is the wrong place
//! for it to fail and a long way from the line that is actually right.

use super::super::native::Native;
use super::super::objects::undefined_of;
use super::super::rooted::Rooted;
use super::super::{functions, throw, with_current};
use super::{built, staged};
use crate::value::Value;

/// What an array's prototype holds that takes a callback.
pub(super) const NATIVES: &[(&str, Native)] = &[
    ("forEach", for_each),
    ("map", map),
    ("filter", filter),
    ("find", find),
    ("findIndex", find_index),
    ("some", some),
    ("every", every),
    ("reduce", reduce),
];

/// `a.forEach(f, thisArg)` — answers `undefined`.
extern "C" fn for_each(_e: u64, this: u64, callback: u64, receiver: u64, _a2: u64, _a3: u64) -> u64 {
    if !required(callback) {
        return nothing();
    }
    let Some(count) = len_of(this) else {
        return nothing();
    };
    for index in 0..count {
        // Absent positions are not visited: the specification defines this
        // family over the keys that EXIST, not over the range `0..length`.
        let Some(element) = existing(this, index) else {
            continue;
        };
        // A callback that throws stops the walk.
        if visit(callback, receiver, this, element, index).is_none() {
            break;
        }
    }
    nothing()
}

/// `a.map(f, thisArg)` — a new array of what `f` answered.
extern "C" fn map(_e: u64, this: u64, callback: u64, receiver: u64, _a2: u64, _a3: u64) -> u64 {
    if !required(callback) {
        return nothing();
    }
    let Some(count) = len_of(this) else {
        return nothing();
    };
    // Decided BEFORE the first callback, which is the order the specification
    // states: `ArraySpeciesCreate` reads two properties and runs a constructor,
    // all of them user code, and a program with a `Symbol.species` getter that
    // logs observes it before any element is visited. `None` is the ordinary
    // case — see [`super::species`].
    let destination = super::species::made(this, count);
    if throw::in_flight() {
        return nothing();
    }
    // ROOTED, because the callback below is user code that allocates, and an
    // allocation collects. What this has produced so far would otherwise live
    // only in a `Vec` on the Rust heap, which no scan of ours reaches — measured
    // as nine of three hundred rounds answering wrong data rather than failing.
    // See `entry::rooted`. A species destination needs none of it: each answer
    // is stored into a real object before the next call runs.
    let mut produced = Rooted::new();
    for index in 0..count {
        // `map` PRESERVES the hole at the matching position rather than skipping
        // it: the result has the same length and the same sparsity, and the
        // callback is not called. Pushing `undefined` here would give the right
        // length and the wrong sparsity. A species destination gets NO write at
        // all for one, which is the same fact — the specification's
        // `CreateDataProperty` is what makes the position exist.
        let Some(element) = existing(this, index) else {
            if destination.is_none() {
                let absent = with_current(|context| super::super::array::hole_of(context));
                produced.values().push(absent);
            }
            continue;
        };
        let Some(answered) = visit(callback, receiver, this, element, index) else {
            // What was built so far is what comes back, and the compiled call
            // site re-raises: nothing here handles the throw.
            break;
        };
        match destination {
            Some(target) => super::species::placed(target, index, answered),
            None => produced.values().push(answered),
        }
    }
    match destination {
        Some(target) => target,
        // Built after the loop, not grown during it. `built` allocates through
        // `array_new`, which takes the context — so an array grown inside the
        // loop would be one borrow taken between two calls into user code.
        None => built(produced.take()),
    }
}

/// `a.filter(f, thisArg)` — a new array of the elements `f` kept.
extern "C" fn filter(_e: u64, this: u64, callback: u64, receiver: u64, _a2: u64, _a3: u64) -> u64 {
    if !required(callback) {
        return nothing();
    }
    let Some(count) = len_of(this) else {
        return nothing();
    };
    // ZERO and not `count`, which is the specification's own asymmetry with
    // `map`: a filter does not know how many it will keep, so the species is
    // asked for an empty destination and the positions are created as they are
    // decided.
    let destination = super::species::made(this, 0);
    if throw::in_flight() {
        return nothing();
    }
    // Rooted for the same reason `map` above is.
    let mut kept = Rooted::new();
    // Its own counter, because the destination is DENSE: a kept element goes to
    // the next free position, not to the one it came from.
    let mut at = 0;
    for index in 0..count {
        // `filter` DISCARDS holes: the result is dense.
        let Some(element) = existing(this, index) else {
            continue;
        };
        match visit(callback, receiver, this, element, index) {
            Some(answered) if truthy(answered) => match destination {
                Some(target) => {
                    super::species::placed(target, at, element);
                    at += 1;
                }
                None => kept.values().push(element),
            },
            Some(_) => {}
            None => break,
        }
    }
    match destination {
        Some(target) => target,
        None => built(kept.take()),
    }
}

/// `a.find(f, thisArg)` — the first element `f` accepted, or `undefined`.
extern "C" fn find(_e: u64, this: u64, callback: u64, receiver: u64, _a2: u64, _a3: u64) -> u64 {
    match sought(this, callback, receiver, false) {
        Some((element, _)) => element,
        None => nothing(),
    }
}

/// `a.findIndex(f, thisArg)` — where that element was, or -1.
///
/// Shares the scan with [`find`] rather than repeating it, because the two must
/// agree about which element matched — and a callback with a side effect makes
/// "run it twice and compare" a different program.
extern "C" fn find_index(
    _e: u64,
    this: u64,
    callback: u64,
    receiver: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let at = sought(this, callback, receiver, false).map_or(-1.0, |(_, index)| index as f64);
    Value::from_f64(at).bits()
}

/// `a.some(f, thisArg)`.
///
/// Stops at the first acceptance, which is observable rather than an
/// optimisation: a callback with a side effect must not run for the rest of the
/// array once the answer is settled.
extern "C" fn some(_e: u64, this: u64, callback: u64, receiver: u64, _a2: u64, _a3: u64) -> u64 {
    let found = sought(this, callback, receiver, true).is_some();
    Value::from_bool(found).bits()
}

/// `a.every(f, thisArg)`.
///
/// True for an empty array, which is the language and the corner an
/// implementation written as "found one that failed" gets right by construction.
extern "C" fn every(_e: u64, this: u64, callback: u64, receiver: u64, _a2: u64, _a3: u64) -> u64 {
    if !required(callback) {
        return Value::from_bool(false).bits();
    }
    let Some(count) = len_of(this) else {
        return Value::from_bool(false).bits();
    };
    for index in 0..count {
        // An absent position is not visited and does not make `every` answer
        // false: the specification defines it over the keys that exist, so
        // `[1,,3].every(x => x > 0)` is `true`.
        let Some(element) = existing(this, index) else {
            continue;
        };
        match visit(callback, receiver, this, element, index) {
            Some(answered) if truthy(answered) => {}
            // A throw and a false answer both stop the scan; only the false
            // one means the predicate said no.
            _ => return Value::from_bool(false).bits(),
        }
    }
    Value::from_bool(true).bits()
}

/// `a.reduce(f, initial)`.
///
/// Without an initial value the first element that EXISTS is it and the loop
/// starts after it — not `undefined`, which would make `[1,2].reduce((a,b) =>
/// a+b)` answer `NaN`.
///
/// An empty array with no initial value is a `TypeError`, which is the one
/// answer a reduction has no honest value for: there is nothing to fold and
/// nothing to fold it into. It answered `undefined` until a native could raise,
/// and `undefined` then flowed into whatever the program did next — a wrong
/// number rather than a stopped program.
extern "C" fn reduce(_e: u64, this: u64, callback: u64, initial: u64, _a2: u64, _a3: u64) -> u64 {
    if !required(callback) {
        return nothing();
    }
    let Some(count) = len_of(this) else {
        return nothing();
    };
    let seeded = !with_current(|context| initial == undefined_of(context));
    let (mut carried, from) = if seeded {
        (initial, 0)
    } else {
        // The seed is the first element that EXISTS, not position zero: a hole
        // at the head is not an accumulator, and seeding with it made
        // `[,1,2].reduce((a,b) => a+b)` answer `NaN`.
        match (0..count).find_map(|index| existing(this, index).map(|held| (held, index))) {
            Some((held, index)) => (held, index + 1),
            None => {
                throw::type_error("Reduce of empty array with no initial value");
                return nothing();
            }
        }
    };
    for index in from..count {
        let Some(element) = existing(this, index) else {
            continue;
        };
        // The accumulator takes the slot `thisArg` would have, which is the
        // arity being spent where the language spends it: `reduce` is the one
        // of these whose callback genuinely needs four arguments, and the one
        // the language gives no `thisArg` at all.
        carried = functions::call(
            callback,
            nothing(),
            carried,
            element,
            Value::from_f64(index as f64).bits(),
            this,
        );
        // A callback that threw leaves `call` answering `undefined`, and the
        // next pass would fold that into the accumulator. What comes back is
        // what had been folded before the throw; the compiled site re-raises.
        if throw::in_flight() {
            break;
        }
    }
    carried
}

/// How many positions the walk covers, read once before the first call.
///
/// `None` for a receiver that is not an array, which is what every method here
/// answers `undefined` for rather than aborting.
pub(super) fn len_of(this: u64) -> Option<usize> {
    with_current(|context| staged(context, this).map(|(_, elements)| elements.len()))
}

/// The element at a position RIGHT NOW, or `None` if there is none there.
///
/// The borrow opens and closes inside this function, so a caller may hold the
/// answer across a call into user code but never the borrow — which is the rule
/// the module documentation states, applied per iteration.
///
/// Absent covers both ways a position can have no key: past the end, because a
/// callback shortened the array, and a hole, because it was never written or was
/// `delete`d. The two are one answer here because every caller treats them
/// alike — the specification asks `HasProperty` and gets `false` for both.
pub(super) fn existing(this: u64, index: usize) -> Option<u64> {
    with_current(|context| {
        let cell = Value(this).as_slot()?;
        let held = *context.elements_at(cell)?.get(index)?;
        (!super::super::array::is_hole(context, held)).then_some(held)
    })
}

/// The same, as the program SEES it: an absent position reads `undefined`.
///
/// For `find` and `findIndex`, which the specification deliberately defines over
/// the range rather than over the keys — they visit a hole with `undefined`
/// where `forEach` and its relatives skip it.
pub(super) fn present(this: u64, index: usize) -> u64 {
    match existing(this, index) {
        Some(held) => held,
        None => nothing(),
    }
}

/// One call of a callback, outside every borrow.
///
/// The three arguments the specification passes — element, index, and the array
/// — with the method's `thisArg` as the receiver. Written once because seven
/// methods make exactly this call, and seven copies is where one of them would
/// pass the index and the element the wrong way round.
///
/// # Why the answer is an `Option`
///
/// `None` means the callback THREW. A throw leaves `call` answering `undefined`,
/// which is a value — so without this every one of those seven would keep
/// calling the callback over the remaining elements, producing effects the
/// language says never happen. The absence is in the type so that a caller has
/// to decide rather than inherit the wrong answer.
pub(super) fn visit(
    callback: u64,
    receiver: u64,
    array: u64,
    element: u64,
    index: usize,
) -> Option<u64> {
    let (absent, at) = with_current(|context| {
        (
            undefined_of(context),
            Value::from_f64(index as f64).bits(),
        )
    });
    let answered = functions::call(callback, receiver, element, at, array, absent);
    match throw::in_flight() {
        true => None,
        false => Some(answered),
    }
}

/// The first element a predicate accepted, and where it was.
///
/// Shared by `find`, `findIndex` and `some`, which differ only in what they
/// report about the same scan.
fn sought(this: u64, callback: u64, receiver: u64, skip_absent: bool) -> Option<(u64, usize)> {
    if !required(callback) {
        return None;
    }
    let count = len_of(this)?;
    for index in 0..count {
        // `some` is the caller that SKIPS, and that is why the parameter exists:
        // it belongs to the `forEach`/`map`/`filter` family, defined over the
        // keys that exist, and shares this scan with `find`/`findIndex`, which
        // belong to the other. One scan and two contracts has to say which.
        let seen = match (existing(this, index), skip_absent) {
            (Some(held), _) => held,
            (None, true) => continue,
            (None, false) => present(this, index),
        };
        match visit(callback, receiver, this, seen, index) {
            Some(answered) if truthy(answered) => return Some((seen, index)),
            Some(_) => {}
            None => return None,
        }
    }
    None
}

/// Whether a callback's answer counts as yes.
///
/// Through the language's falsy set rather than a comparison against `true`: a
/// predicate answering `1` or `"a"` accepts, and one answering `0` or `""` does
/// not. Comparing to `true` is the version that looks right and rejects every
/// idiomatic predicate.
fn truthy(value: u64) -> bool {
    super::super::primitives::to_boolean(value)
}

/// The `undefined` a method answers when there is nothing to answer.
fn nothing() -> u64 {
    with_current(|context| undefined_of(context))
}

/// Whether the callback is one, refused with a `TypeError` when it is not.
///
/// Asked BEFORE the array is touched, which is the specification's order and is
/// observable: a receiver whose index reads are a proxy trap must see none of
/// them. It silently walked zero elements and answered instead — so
/// `[1,2].map(undefined)` was an empty array where every other engine stops the
/// program, and a program written against that is correct here and broken
/// everywhere else.
fn required(callback: u64) -> bool {
    if crate::entry::iterate::callable(callback) {
        return true;
    }
    throw::type_error("the array method was given something that is not a function");
    false
}
