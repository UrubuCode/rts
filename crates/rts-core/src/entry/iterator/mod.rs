//! `Iterator` — the ES2025 helper base, and the prototype every iterator here
//! reaches.
//!
//! # What changed, and why the old shape could not stay
//!
//! It was a NAMESPACE carrying `from`, and the helpers were copied onto each
//! kind of iterator this engine builds: `ListIterator` had eleven, `ArrayCursor`
//! had eleven more that forwarded to them, and a generator had none at all — so
//! `g().map(f)` was `TypeError: (intermediate value).map is not a function` in
//! every program that reached for it. Two copies and a hole is what a shared
//! prototype exists to prevent, and the language already says which one:
//! `%IteratorPrototype%` owns the eleven, `%IteratorHelperPrototype%` sits on
//! it, and every built-in iterator's prototype sits on it too.
//!
//! So this is the object, and [`register`] is where the three other prototypes
//! are pointed at it. A generator reaches `map` by INHERITING it, which is both
//! the shape a program can observe with `Object.getPrototypeOf` and the reason
//! there is no longer a place for a twelfth copy to be written.
//!
//! # Lazy, which the copies were not
//!
//! The five that answer another iterator — `map`, `filter`, `take`, `drop`,
//! `flatMap` — build a [`helper::Helper`] over the receiver and pull nothing.
//! [`helper`] says what that costs and what it buys; the short version is that
//! `naturals().take(3)` over an endless generator terminates here and did not
//! before.
//!
//! The six that answer a value — `reduce`, `toArray`, `forEach`, `some`,
//! `every`, `find` — drive the receiver through its own `next`, so they work on
//! anything with the protocol rather than only on what this crate built.

mod drive;
mod helper;

use crate::entry::{
    Context, class_support, functions, iterate, native, objects, symbol, throw, with_current,
};
use crate::value::Value;

pub(in crate::entry) use self::drive::{Source, Step};
pub(in crate::entry) use self::helper::Helper;

/// `Iterator`, and the eleven helpers its prototype owns.
#[rtse::class("Iterator")]
impl Iterator {
    /// `Iterator.from(x)` — an iterator over `x` with the helpers on it.
    ///
    /// Answers `x`'s own iterator UNCHANGED when that already inherits
    /// `Iterator.prototype`, which every iterator this engine builds now does.
    /// Wrapping one would give a program two objects where the language gives
    /// it one, and `Iterator.from(it) === it` is what a fixture asks.
    /// `new Iterator()` — refused, and `Iterator()` with it.
    ///
    /// The language makes it an ABSTRACT class: it exists to be the prototype
    /// every iterator inherits and to be extended, never to be built. A default
    /// constructor would answer a bare object that passes `instanceof Iterator`
    /// and has no `next`, which is worse than the refusal.
    #[construct]
    fn build(this: u64) -> u64 {
        throw::type_error("Iterator is abstract and cannot be constructed directly");
        let _ = this;
        drive::absent()
    }

    #[stat]
    fn from(value: u64) -> u64 {
        let Some(source) = flattenable(value) else {
            return drive::absent();
        };
        match inherits_iterator_prototype(source.object) {
            true => source.object,
            false => helper::wrap(source),
        }
    }

    /// `it.map(f)` — the same elements, each through `f`.
    fn map(this: u64, callback: u64) -> u64 {
        built(this, callback, helper::Kind::Map)
    }

    /// `it.filter(p)` — the elements `p` answers truthy for.
    fn filter(this: u64, callback: u64) -> u64 {
        built(this, callback, helper::Kind::Filter)
    }

    /// `it.take(n)` — at most the first `n`.
    fn take(this: u64, count: u64) -> u64 {
        counted(this, count, helper::Kind::Take)
    }

    /// `it.drop(n)` — everything after the first `n`.
    fn drop(this: u64, count: u64) -> u64 {
        counted(this, count, helper::Kind::Drop)
    }

    /// `it.flatMap(f)` — `f`'s answers, one level flattened.
    fn flat_map(this: u64, callback: u64) -> u64 {
        built(this, callback, helper::Kind::FlatMap)
    }

    /// `it.toArray()` — everything left, as an array.
    fn to_array(this: u64) -> u64 {
        let Some(source) = receiver(this) else {
            return drive::absent();
        };
        let mut produced = crate::entry::rooted::Rooted::new();
        loop {
            match drive::step(&source) {
                None => return drive::absent(),
                Some(Step::Done) => break,
                Some(Step::Value(value)) => produced.values().push(value),
            }
        }
        crate::entry::array_proto::built(produced.take())
    }

    /// `it.forEach(f)` — `f` over each, answering nothing.
    fn for_each(this: u64, callback: u64) -> u64 {
        let Some(source) = walked(this, callback) else {
            return drive::absent();
        };
        let mut index = 0.0;
        loop {
            match drive::step(&source) {
                None => return drive::absent(),
                Some(Step::Done) => return drive::absent(),
                Some(Step::Value(value)) => {
                    call(callback, value, index);
                    index += 1.0;
                    if throw::in_flight() {
                        drive::close_abrupt(source.object);
                        return drive::absent();
                    }
                }
            }
        }
    }

    /// `it.reduce(f, initial)`.
    ///
    /// An EMPTY iterator with no initial value is a `TypeError`, which is what
    /// the specification says and what this answered `undefined` for while a
    /// native could not raise.
    #[arity(1)]
    fn reduce(this: u64, callback: u64, initial: u64) -> u64 {
        let Some(source) = walked(this, callback) else {
            return drive::absent();
        };
        let absent = drive::absent();
        let mut index = 0.0;
        let mut carried = initial;
        if initial == absent {
            match drive::step(&source) {
                None => return absent,
                Some(Step::Done) => {
                    throw::type_error("reduce of an empty iterator with no initial value");
                    return absent;
                }
                Some(Step::Value(value)) => {
                    carried = value;
                    index = 1.0;
                }
            }
        }
        loop {
            match drive::step(&source) {
                None => return absent,
                Some(Step::Done) => return carried,
                Some(Step::Value(value)) => {
                    let counted = Value::from_f64(index).bits();
                    carried = functions::call(callback, absent, carried, value, counted, absent);
                    index += 1.0;
                    if throw::in_flight() {
                        drive::close_abrupt(source.object);
                        return absent;
                    }
                }
            }
        }
    }

    /// `it.some(p)` — whether any element satisfies `p`. Stops at the first.
    fn some(this: u64, callback: u64) -> u64 {
        searched(this, callback, Answer::Some)
    }

    /// `it.every(p)` — whether all of them do. Stops at the first that does not.
    fn every(this: u64, callback: u64) -> u64 {
        searched(this, callback, Answer::Every)
    }

    /// `it.find(p)` — the first that satisfies `p`, or `undefined`.
    fn find(this: u64, callback: u64) -> u64 {
        searched(this, callback, Answer::Find)
    }
}

/// What a short-circuiting walk answers when it stops.
enum Answer {
    /// `some` — `true` at the first truthy, `false` at the end.
    Some,
    /// `every` — `false` at the first falsy, `true` at the end.
    Every,
    /// `find` — the element itself, or `undefined`.
    Find,
}

/// The three that stop early, which differ only in what stopping means.
fn searched(this: u64, callback: u64, answer: Answer) -> u64 {
    let Some(source) = walked(this, callback) else {
        return drive::absent();
    };
    let mut index = 0.0;
    loop {
        let value = match drive::step(&source) {
            None => return drive::absent(),
            Some(Step::Done) => {
                return match answer {
                    Answer::Some => truth(false),
                    Answer::Every => truth(true),
                    Answer::Find => drive::absent(),
                };
            }
            Some(Step::Value(value)) => value,
        };
        let decided = call(callback, value, index);
        index += 1.0;
        if throw::in_flight() {
            drive::close_abrupt(source.object);
            return drive::absent();
        }
        let held = crate::entry::primitives::to_boolean(decided);
        let stop = match answer {
            Answer::Some | Answer::Find => held,
            Answer::Every => !held,
        };
        if stop {
            drive::close(source.object);
            if throw::in_flight() {
                return drive::absent();
            }
            return match answer {
                Answer::Some => truth(true),
                Answer::Every => truth(false),
                Answer::Find => value,
            };
        }
    }
}

/// A boolean, as the value a member answers.
fn truth(held: bool) -> u64 {
    Value::from_bool(held).bits()
}

/// One call of a helper's callback: the element and its position.
fn call(callback: u64, element: u64, index: f64) -> u64 {
    let nothing = drive::absent();
    let counted = Value::from_f64(index).bits();
    functions::call(callback, nothing, element, counted, nothing, nothing)
}

/// The receiver of a helper, as the pair a step needs.
///
/// A member of `Iterator.prototype` requires an OBJECT receiver — the
/// specification's first step in every one of the eleven — and says so with a
/// `TypeError` rather than answering `undefined`.
fn receiver(this: u64) -> Option<Source> {
    if !drive::is_object(this) {
        throw::type_error("the receiver of an iterator helper is not an object");
        return None;
    }
    let source = drive::direct(this);
    match throw::in_flight() {
        true => None,
        false => Some(source),
    }
}

/// The receiver, and a callback that has to be callable.
fn walked(this: u64, callback: u64) -> Option<Source> {
    let source = receiver(this)?;
    if !iterate::callable(callback) {
        // The receiver is closed before the refusal, which is the order the
        // specification states: an argument that is not a function still leaves
        // the iterator it was handed to closed.
        drive::close(source.object);
        throw::type_error("the iterator helper was given something that is not a function");
        return None;
    }
    Some(source)
}

/// One of the three helpers that take a callback.
fn built(this: u64, callback: u64, kind: helper::Kind) -> u64 {
    let Some(source) = walked(this, callback) else {
        return drive::absent();
    };
    helper::over(source, kind, callback, 0.0)
}

/// One of the two that take a count.
///
/// `NaN` and a negative are a `RangeError` rather than a clamp: the language
/// chose to refuse them, and clamping would make `it.take(-1)` an empty
/// iterator here and an error everywhere else.
fn counted(this: u64, count: u64, kind: helper::Kind) -> u64 {
    let Some(source) = receiver(this) else {
        return drive::absent();
    };
    // A symbol does not convert, and the language says so with a `TypeError`
    // rather than with `NaN`. Reaching `to_number` first would answer `NaN` and
    // this would report a `RangeError` — the right shape of refusal for the
    // wrong reason, which is the kind of near-miss a fixture catches and a
    // program does not.
    if with_current(|context| symbol::is_symbol(context, count)) {
        throw::type_error("an iterator helper's count cannot be a symbol");
        return drive::absent();
    }
    let limit = class_support::to_number(count);
    if throw::in_flight() {
        return drive::absent();
    }
    if limit.is_nan() || limit < 0.0 {
        drive::close(source.object);
        throw::range_error("an iterator helper's count is not a non-negative number");
        return drive::absent();
    }
    let nothing = drive::absent();
    helper::over(source, kind, nothing, limit.trunc())
}

/// `GetIteratorFlattenable(x, iterate-strings)` — what `Iterator.from` adopts.
///
/// A string IS accepted here, unlike in `flatMap`: `Iterator.from("abc")` walks
/// three characters, and the two operations differ by exactly that flag in the
/// specification.
fn flattenable(value: u64) -> Option<Source> {
    let text = with_current(|context| {
        Value(value)
            .as_slot()
            .is_some_and(|cell| context.text_at(cell).is_some())
    });
    if !drive::is_object(value) && !text {
        throw::type_error("Iterator.from: the value is neither an object nor a string");
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
                throw::type_error("Iterator.from: Symbol.iterator answered a primitive");
                return None;
            }
            answered
        }
    };
    Some(drive::direct(iterator))
}

/// Whether a value's prototype chain reaches `Iterator.prototype`.
fn inherits_iterator_prototype(value: u64) -> bool {
    with_current(|context| {
        let Some(prototype) = class_support::prototype(context, "Iterator") else {
            return false;
        };
        let mut walking = Value(value).as_slot();
        while let Some(cell) = walking {
            let Some(parent) = context.prototype_at(cell) else {
                return false;
            };
            if parent == prototype {
                return true;
            }
            walking = Value(parent).as_slot();
        }
        false
    })
}

/// Installs `Iterator`, and points every other iterator prototype at it.
///
/// The three that are pointed here are the three this crate builds:
/// `ListIterator` (a `Map`'s, a `Set`'s, a string's), `ArrayCursor` (an array's)
/// and `Generator`. Each is registered lazily by its own module, so this asks
/// for the prototype it already has rather than forcing a registration order —
/// and [`adopt`] is called again from each of those registrations, because
/// whichever runs second is the one that finds both.
pub(in crate::entry) fn register(context: &mut Context) -> u64 {
    adopt(context);
    class_support::made(context, "Iterator").unwrap_or_else(|| objects::undefined_of(context))
}

/// The one member the attribute cannot name: a symbol-keyed key.
fn install_symbol_iterator(context: &mut Context) {
    let Some(prototype) = class_support::prototype(context, "Iterator") else {
        return;
    };
    let Some(cell) = Value(prototype).as_slot() else {
        return;
    };
    // `Symbol.iterator` answering the receiver, which is what makes every
    // iterator its own iterable — and what the four prototypes below used to
    // install one copy of each, on every instance they made.
    let key = context.well_known(&format!("{}iterator", symbol::PREFIX));
    let itself = native::callable(context, itself as native::Native);
    native::name_of(context, itself, "[Symbol.iterator]");
    objects::put(context, cell, key, itself);
    native::hidden(context, cell, key);
}

/// Points a built-in iterator's prototype at `Iterator.prototype`.
///
/// Idempotent, and called from four places for that reason: registration order
/// here is decided by which kind of iterator a program reaches first, and a
/// single wiring point would have to force one.
pub(in crate::entry) fn adopt(context: &mut Context) {
    // Registering it here rather than waiting for a program to name `Iterator`:
    // a generator reaches the helpers by INHERITING them, so the object has to
    // exist the moment the first generator prototype does, whether or not the
    // global was ever read. `register_iterator` answers immediately once it has
    // run, and `record` happens before any install — so the call this one is
    // reached from cannot recurse into itself.
    let prototype = match class_support::prototype(context, "Iterator") {
        Some(prototype) => prototype,
        None => {
            register_iterator(context);
            install_symbol_iterator(context);
            match class_support::prototype(context, "Iterator") {
                Some(prototype) => prototype,
                None => return,
            }
        }
    };
    for name in [
        "ListIterator",
        "ArrayCursor",
        "Generator",
        "IteratorHelper",
        "IteratorWrapper",
    ] {
        if let Some(found) = class_support::prototype(context, name)
            && let Some(cell) = Value(found).as_slot()
        {
            context.set_prototype(cell, prototype);
        }
    }
}

/// Registers `%IteratorHelperPrototype%`, with the tag the five share.
fn register_helper_prototype(context: &mut Context) -> u64 {
    let made = self::helper::register_iterator_helper(context);
    if let Some(prototype) = class_support::prototype(context, "IteratorHelper")
        && let Some(cell) = Value(prototype).as_slot()
    {
        let key = context.well_known(&format!("{}toStringTag", symbol::PREFIX));
        let tag = context.intern_value(crate::text::Str::from_str(helper::TAG)).bits();
        objects::put(context, cell, key, tag);
        native::hidden(context, cell, key);
    }
    register(context);
    adopt(context);
    made
}

/// `it[Symbol.iterator]()` — the iterator itself.
extern "C" fn itself(_environment: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    this
}
