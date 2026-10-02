//! `Error` and the family that inherits from it.
//!
//! # Why this is first
//!
//! `throw new Error("…")` is how every program raises, and until this existed
//! the name did not resolve — so the one statement a failing program is written
//! with did not compile. Nothing else in the queue is reached by a program that
//! cannot express failure.
//!
//! # What an error object is here
//!
//! An ordinary object with a `message` property, inheriting from a prototype
//! that holds `name` and `toString`. Nothing beside the cell, no reserved
//! layout, no capture of anything — which is why `class MyError extends Error`
//! works with nothing added: the instance `construct` allocates already inherits
//! from `MyError.prototype`, and this only writes a property onto it.
//!
//! # What is deliberately absent
//!
//! **`stack`.** It is not in the specification, every engine spells it
//! differently, and producing one means walking native frames Cranelift emitted
//! — the same machinery an uncaught throw needs and does not have. A `stack`
//! that answered `""` would be a property programs branch on, answering the
//! wrong thing quietly.
//!
//! # Why every constructor here takes the options bag
//!
//! The ES2022 bag is `Error`'s, and the six subclasses inherit their constructor
//! behaviour from it rather than declaring their own — so a family where only
//! `Error` read `{ cause }` was not "half done", it was **inconsistent in the one
//! direction a program notices**: `new Error(m, { cause })` carried the cause and
//! `new TypeError(m, { cause })` dropped it silently, which is the shape almost
//! every re-throw in real code is written in.
//!
//! The alternative was one shared constructor the six delegate to by name. It was
//! rejected because `#[rtse::class]` derives the wrapper from the Rust signature:
//! a subclass whose `build` takes two arguments cannot be handed a third, so the
//! arity has to be stated where the wrapper is generated. What is shared is the
//! BODY — [`written_with_cause`] — and each declaration is the one line that says
//! which name it is.
//!
//! # Why `throw` still ends the program
//!
//! Making the value is this module's half. Finding a handler in a *caller* is
//! [`super::throw`]'s, and that one needs an exception table and a personality
//! routine — a campaign rather than a branch. So `throw new Error("x")` now
//! reports the error's own text rather than "an object", which is the visible
//! half of the improvement, and a `try` around a call is still refused by name.

use super::error_describe::described;
use super::error_stack::install_stack_accessor;
use super::objects::undefined_of;
use super::{Context, with_current};
use crate::value::Value;

/// `Error`.
#[rtse::class("Error")]
impl Error {
    /// What `toString` reads when the instance has no `name` of its own, and
    /// what `err.name` answers.
    const name: &str = "Error";

    /// What `err.message` answers when the constructor was given nothing.
    ///
    /// The specification does not store `undefined` for `new Error()` — it omits
    /// the own property entirely, so the read reaches the prototype and finds
    /// the empty string. Without this it fell off the chain and answered
    /// `undefined`, which prints as the word wherever a message is shown.
    ///
    /// Stated on `Error` alone: every other class in this file extends it and
    /// inherits the same default rather than restating it seven times.
    const message: &str = "";

    /// `new Error(message, options)` — `options.cause`, ES2022.
    // `length` is 1. `SetFunctionLength` counts the arguments the LANGUAGE
    // pins, and the options bag is not one of them — every runtime answers 1
    // for all seven. The derived arity counts the Rust signature, which has to
    // carry the bag as a slot, so the two differ and the override says which.
    #[arity(1)]
    #[construct]
    fn build(this: u64, message: u64, options: u64) -> u64 {
        written_with_cause(this, message, options, "Error")
    }

    /// `err.toString()` — `"Error: boom"`, or just the name without a message.
    fn to_string(this: u64) -> u64 {
        described(this)
    }

    /// How many frames a capture keeps. V8's, not the specification's.
    ///
    /// `#[settable]` because a program WRITES this — `Error.stackTraceLimit = 0`
    /// is how a library turns traces off, and the pinned attributes every other
    /// numeric constant gets would have made that a silent no-op. 10 is Node's
    /// default and the number its own descriptor reports.
    #[stat]
    #[settable]
    const stackTraceLimit: f64 = 10.0;

    /// `Error.captureStackTrace(target, hideFrom)` — V8's, which Node exposes.
    ///
    /// The body is [`super::error_stack`]'s: this file declares the eight error
    /// classes and the one thing it must not also be is 620 lines long.
    #[stat]
    fn capture_stack_trace(target: u64, hide_from: u64) -> u64 {
        super::error_stack::capture(target, hide_from)
    }
}

/// `TypeError` — what an operation on the wrong kind of value raises.
#[rtse::class("TypeError", extends = register_error)]
impl TypeError {
    /// The name every instance answers.
    const name: &str = "TypeError";

    /// `new TypeError(message, options)` — `options.cause`, ES2022.
    // `length` is 1. `SetFunctionLength` counts the arguments the LANGUAGE
    // pins, and the options bag is not one of them — every runtime answers 1
    // for all seven. The derived arity counts the Rust signature, which has to
    // carry the bag as a slot, so the two differ and the override says which.
    #[arity(1)]
    #[construct]
    fn build(this: u64, message: u64, options: u64) -> u64 {
        written_with_cause(this, message, options, "TypeError")
    }
}

/// `RangeError` — a value outside the set an operation accepts.
#[rtse::class("RangeError", extends = register_error)]
impl RangeError {
    /// The name every instance answers.
    const name: &str = "RangeError";

    /// `new RangeError(message, options)` — `options.cause`, ES2022.
    // `length` is 1. `SetFunctionLength` counts the arguments the LANGUAGE
    // pins, and the options bag is not one of them — every runtime answers 1
    // for all seven. The derived arity counts the Rust signature, which has to
    // carry the bag as a slot, so the two differ and the override says which.
    #[arity(1)]
    #[construct]
    fn build(this: u64, message: u64, options: u64) -> u64 {
        written_with_cause(this, message, options, "RangeError")
    }
}

/// `SyntaxError`.
#[rtse::class("SyntaxError", extends = register_error)]
impl SyntaxError {
    /// The name every instance answers.
    const name: &str = "SyntaxError";

    /// `new SyntaxError(message, options)` — `options.cause`, ES2022.
    // `length` is 1. `SetFunctionLength` counts the arguments the LANGUAGE
    // pins, and the options bag is not one of them — every runtime answers 1
    // for all seven. The derived arity counts the Rust signature, which has to
    // carry the bag as a slot, so the two differ and the override says which.
    #[arity(1)]
    #[construct]
    fn build(this: u64, message: u64, options: u64) -> u64 {
        written_with_cause(this, message, options, "SyntaxError")
    }
}

/// `ReferenceError`.
#[rtse::class("ReferenceError", extends = register_error)]
impl ReferenceError {
    /// The name every instance answers.
    const name: &str = "ReferenceError";

    /// `new ReferenceError(message, options)` — `options.cause`, ES2022.
    // `length` is 1. `SetFunctionLength` counts the arguments the LANGUAGE
    // pins, and the options bag is not one of them — every runtime answers 1
    // for all seven. The derived arity counts the Rust signature, which has to
    // carry the bag as a slot, so the two differ and the override says which.
    #[arity(1)]
    #[construct]
    fn build(this: u64, message: u64, options: u64) -> u64 {
        written_with_cause(this, message, options, "ReferenceError")
    }
}

/// `EvalError`, which nothing raises and every program may still catch.
#[rtse::class("EvalError", extends = register_error)]
impl EvalError {
    /// The name every instance answers.
    const name: &str = "EvalError";

    /// `new EvalError(message, options)` — `options.cause`, ES2022.
    // `length` is 1. `SetFunctionLength` counts the arguments the LANGUAGE
    // pins, and the options bag is not one of them — every runtime answers 1
    // for all seven. The derived arity counts the Rust signature, which has to
    // carry the bag as a slot, so the two differ and the override says which.
    #[arity(1)]
    #[construct]
    fn build(this: u64, message: u64, options: u64) -> u64 {
        written_with_cause(this, message, options, "EvalError")
    }
}

/// `URIError`.
#[rtse::class("URIError", extends = register_error)]
impl UriError {
    /// The name every instance answers.
    const name: &str = "URIError";

    /// `new URIError(message, options)` — `options.cause`, ES2022.
    // `length` is 1. `SetFunctionLength` counts the arguments the LANGUAGE
    // pins, and the options bag is not one of them — every runtime answers 1
    // for all seven. The derived arity counts the Rust signature, which has to
    // carry the bag as a slot, so the two differ and the override says which.
    #[arity(1)]
    #[construct]
    fn build(this: u64, message: u64, options: u64) -> u64 {
        written_with_cause(this, message, options, "URIError")
    }
}

/// `AggregateError` — several failures reported as one.
///
/// # Why this one is not another line beside its siblings
///
/// Every other member of the family differs from `Error` in nothing but its
/// name, which is why they are six near-identical declarations. This one takes
/// an EXTRA argument in front and writes a second property: `new
/// AggregateError(errors, message, options)` carries the list, and `Promise.any`
/// is the reason the language has it — a rejection that is several rejections
/// needs somewhere to put them.
///
/// The argument order is the language's and is easy to get backwards: the errors
/// come FIRST, so `message` and `options` are each one position further along
/// than in every other constructor in the family.
#[rtse::class("AggregateError", extends = register_error)]
impl AggregateError {
    /// The name every instance answers.
    const name: &str = "AggregateError";

    /// `new AggregateError(errors, message, options)`.
    // `AggregateError.length` is 2 — the list and the message — for the reason
    // its siblings' is 1: the options bag is a slot here and not an argument
    // the language counts.
    #[arity(2)]
    #[construct]
    fn build(this: u64, errors: u64, message: u64, options: u64) -> u64 {
        // Walked FIRST, and outside every borrow. `errors` is an ITERABLE — the
        // language says so, and a generator is the ordinary spelling — so
        // producing the list runs user code, which is why this cannot happen
        // inside the `with_current` that writes the property. Doing it before
        // the instance exists is also what makes the walk's own throw cheap to
        // propagate: there is nothing half-built to abandon.
        //
        // `super::iterate::iterate` and not a walk written here: it is the
        // crate's single answer to "what does this yield", covering an array, a
        // string, a `Map`, a `Set` and anything declaring `Symbol.iterator`, and
        // it COPIES — which is what the specification's `IteratorToList` does
        // and what the previous version of this constructor could not do. That
        // version stored the argument itself, so `new AggregateError(gen())`
        // gave `.errors` a generator object with no `.length` and no `.map`.
        let listed = super::iterate::iterate(errors);
        // Rule 8: the walk called user code, so ask before looking at the
        // answer. This constructor PROPAGATES rather than handles — a `next()`
        // that threw is the caller's throw, and the compiled call site above
        // re-raises it. Building the error anyway would answer an object for a
        // constructor the language says never returned.
        if super::throw::in_flight() {
            return with_current(|context| undefined_of(context));
        }
        // Rooted across the construction below, which interns strings and
        // allocates: the array is named only by this frame's `u64` until the
        // property write puts it on the instance, and `super::rooted` exists
        // because a machine-stack scan does not reach a Rust local reliably.
        let listed = super::rooted::Rooted::with(vec![listed]);
        let made = written_with_cause(this, message, options, "AggregateError");
        let listed = listed.take();
        with_current(|context| {
            let Some(cell) = Value(made).as_slot() else {
                return made;
            };
            let key = context.well_known("errors");
            super::objects::put(context, cell, key, listed[0]);
            // NON-ENUMERABLE, which the specification spells out and which is
            // observable in the most ordinary way there is: `JSON.stringify(agg)`
            // and `{...agg}` included the whole error list, and
            // `Object.keys(agg)` reported `["errors"]` where every other engine
            // reports nothing. `message` and `stack` are non-enumerable for the
            // same reason and this was the one that was not.
            super::native::hidden(context, cell, key);
            made
        })
    }
}

/// The object, with its message written on it.
///
/// # Why the receiver may have to be made here
///
/// `Error("x")` and `new Error("x")` are the same operation — the language says
/// so explicitly, and it is the spelling a lot of code uses. A plain call hands
/// this `undefined` as the receiver, so an implementation that only filled in an
/// object it was given would answer `undefined` for half the ways the
/// constructor is written.
fn written(this: u64, message: u64, class: &'static str) -> u64 {
    // `ToString(message)` FIRST, and outside every borrow, because it is user
    // code: the language converts the argument with the string hint before it
    // has an object to write onto, so `new Error([1, 2])` carries `"1,2"`.
    //
    // This was `text::to_text` inside the borrow, which is the PRIMITIVE half of
    // the conversion — it answers `None` for every object, and the `None` was
    // read as "no message". So every object argument stored nothing silently,
    // and `new Error(Symbol())` did too where the language raises.
    // `text::to_string_value` is the whole conversion, and its `None` is a throw
    // rather than an absence.
    let absent = with_current(|context| undefined_of(context));
    let mut converted = None;
    if message != absent {
        let Some(text) = super::text::to_string_value(message) else {
            // Rule 8: the conversion raised. Nothing has been written and there
            // is no instance to abandon — the receiver is made below.
            return absent;
        };
        converted = Some(text);
    }
    with_current(|context| {
        let Some(cell) = receiver(context, this, class) else {
            return undefined_of(context);
        };
        if let Some(value) = converted {
            let key = context.well_known("message");
            super::objects::put(context, cell, key, value);
            // Node exposes `message` as an own property, but not as an enumerable
            // one. Keeping the data property and changing only its attributes
            // preserves ordinary reads while making `Object.keys(error)` agree.
            super::native::hidden(context, cell, key);
        }

        // `.stack`, captured HERE — where the error is CONSTRUCTED, not where it
        // is thrown. That is what every engine does and the difference matters:
        // `const e = new Error("x"); … ; throw e;` names the line that made it,
        // which is the one a reader is looking for.
        //
        // The header line is `Name: message`, then a frame per line, which is
        // what Node and Bun print and what a program that splits on `\n    at `
        // expects.
        //
        // DEFERRED. What is captured is the call stack as it stands right now —
        // a `Vec<u64>` of code addresses — and the class name. Rendering it into
        // text and interning that is what the accessor on `Error.prototype` does
        // when, and only when, something asks.
        //
        // Measured by ablation, release, min of 9 over 100 K iterations:
        //
        // ```text
        // return immediately after `receiver`             100 ns
        // the stack RENDERED, not interned or written     420 ns
        // the whole constructor                           790 ns
        // ```
        //
        // So 320 ns to render and 370 to intern and write, on every `new Error`
        // — against `new Map()` at 110 and a plain class instance at 60. Almost
        // nothing reads `.stack`, and a `throw`/`catch` that never looks at it
        // was paying all of it.
        //
        // The header is NOT built here either: `class` is a `&'static str` and
        // the message is read back off the instance at render time, which is
        // also what makes `err.name = "Mine"` before the first read show up —
        // the same reason the message reads through the property path.
        install_stack_accessor(context);
        context.defer_stack(cell, class);

        Value::from_slot(cell).bits()
    })
}

/// [`written`], plus the ES2022 options bag's `cause`.
///
/// `Error(m, { cause })` (called with or without `new`) sets `.cause` from the
/// bag's `cause` property — `Error(m, {})` leaves it unset rather than writing
/// `undefined`, which is why this asks for the property's presence rather than
/// reading it unconditionally.
///
/// # Why `HasProperty` and `Get` rather than the own slot
///
/// Because `InstallErrorCause` is written in terms of both, and the difference
/// is not academic. This asked `objects::own_property`, which reads a slot the
/// object holds ITSELF — so a bag built by `Object.create(base)` and a bag whose
/// `cause` is a getter both reported "no cause" and the error came out without
/// one. The second is worse than a wrong value: the getter never ran, so a bag
/// counting its own reads saw zero.
///
/// The pair is also why this cannot stay inside one borrow: a getter is user
/// code, and rule 8 applies to both crossings.
///
/// # Why the property is non-enumerable
///
/// `CreateNonEnumerableDataPropertyOrThrow` is what the specification names, and
/// the enumerable spelling is observable in the most ordinary way there is:
/// `JSON.stringify(err)` serialised the cause and `Object.keys(err)` reported
/// `["cause"]` where every runtime reports nothing. `message`, `stack` and
/// `errors` are non-enumerable for the same reason and this was the one that
/// was not.
fn written_with_cause(this: u64, message: u64, options: u64, class: &'static str) -> u64 {
    let made = written(this, message, class);
    // Rule 8: `written` converted the message, which is user code. A throw there
    // means there is no instance, and asking the bag for a cause to put on it
    // would run a getter the language never reaches.
    if super::throw::in_flight() {
        return made;
    }
    // An OBJECT, which is what `InstallErrorCause` tests. `as_slot` was the test
    // and it is a different one: a string primitive has a cell too, so
    // `new Error("m", "bag")` took the branch and asked a string for a property.
    if !with_current(|context| super::objects::is_object(context, options)) {
        return made;
    }
    let key = with_current(|context| context.well_known_text("cause"));
    let present = super::computed::has_property(key, options);
    if super::throw::in_flight() || !present {
        return made;
    }
    let cause = super::computed::get_indexed(options, key);
    if super::throw::in_flight() {
        return made;
    }
    with_current(|context| {
        let Some(instance) = Value(made).as_slot() else {
            return;
        };
        let key = context.well_known("cause");
        super::objects::put(context, instance, key, cause);
        super::native::hidden(context, instance, key);
    });
    made
}

/// The object to write onto: the one `new` made, or one made here.
fn receiver(context: &mut Context, this: u64, class: &'static str) -> Option<u32> {
    if let Some(cell) = Value(this).as_slot() {
        return Some(cell);
    }
    let cell = super::native::plain(context)?;
    if let Some(prototype) = super::class_support::prototype(context, class) {
        context.set_prototype(cell, prototype);
    }
    Some(cell)
}

/// Every name this module provides, and the registration behind each.
///
/// A list here rather than a `match` in [`super::global`] because the arm there
/// would name seven functions that differ only in which one they call, and the
/// set of error classes is a fact about this module. `global` asks; this
/// answers.
pub(super) fn provided(name: &str) -> Option<fn(&mut Context) -> u64> {
    Some(match name {
        "Error" => register_error,
        "TypeError" => register_type_error,
        "RangeError" => register_range_error,
        "SyntaxError" => register_syntax_error,
        "ReferenceError" => register_reference_error,
        "EvalError" => register_eval_error,
        "URIError" => register_uri_error,
        "AggregateError" => register_aggregate_error,
        _ => return None,
    })
}
