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

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
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
    #[construct]
    fn build(this: u64, message: u64, options: u64) -> u64 {
        written_with_cause(this, message, options, "Error")
    }

    /// `err.toString()` — `"Error: boom"`, or just the name without a message.
    fn to_string(this: u64) -> u64 {
        described(this)
    }
}

/// `TypeError` — what an operation on the wrong kind of value raises.
#[rtse::class("TypeError", extends = register_error)]
impl TypeError {
    /// The name every instance answers.
    const name: &str = "TypeError";

    /// `new TypeError(message, options)` — `options.cause`, ES2022.
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
    with_current(|context| {
        let Some(cell) = receiver(context, this, class) else {
            return undefined_of(context);
        };
        let mut described = String::new();
        if message != undefined_of(context)
            && let Some(text) = super::text::to_text(context, Value(message))
        {
            described = text.to_rust().unwrap_or_default();
            let value = context.intern_value(text).bits();
            let key = context.well_known("message");
            super::objects::put(context, cell, key, value);
        }

        // `.stack`, captured HERE — where the error is CONSTRUCTED, not where it
        // is thrown. That is what every engine does and the difference matters:
        // `const e = new Error("x"); … ; throw e;` names the line that made it,
        // which is the one a reader is looking for.
        //
        // The header line is `Name: message`, then a frame per line, which is
        // what Node and Bun print and what a program that splits on `\n    at `
        // expects.
        let header = match described.is_empty() {
            true => class.to_owned(),
            false => format!("{class}: {described}"),
        };
        let stack = format!("{header}{}", super::throw::stack_text(context));
        let value = context.intern_value(crate::text::Str::from_str(&stack)).bits();
        let key = context.well_known("stack");
        super::objects::put(context, cell, key, value);

        Value::from_slot(cell).bits()
    })
}

/// [`written`], plus the ES2022 options bag's `cause`.
///
/// `Error(m, { cause })` (called with or without `new`) sets `.cause` from the
/// bag's own `cause` property — `Error(m, {})` leaves it unset rather than
/// writing `undefined`, which is why this checks for the property's presence
/// rather than reading it unconditionally.
fn written_with_cause(this: u64, message: u64, options: u64, class: &'static str) -> u64 {
    let made = written(this, message, class);
    with_current(|context| {
        let (Some(instance), Some(bag)) = (Value(made).as_slot(), Value(options).as_slot()) else {
            return;
        };
        let key = context.well_known("cause");
        if let Some(cause) = super::objects::own_property(context, bag, key) {
            super::objects::put(context, instance, key, cause.0);
        }
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

/// `name` and `message` joined the way the specification joins them.
///
/// Both are read through the ordinary property path, so a program that wrote
/// `err.name = "Mine"` sees `"Mine: boom"` — which is what the language does and
/// what reading the class's own name instead would have got wrong.
fn described(this: u64) -> u64 {
    with_current(|context| {
        let Some(cell) = Value(this).as_slot() else {
            return undefined_of(context);
        };
        let Some(joined) = joined(context, cell) else {
            return undefined_of(context);
        };
        context.intern_value(Str::from_str(&joined)).bits()
    })
}

/// `name: message`, from properties alone.
///
/// `None` for a cell carrying neither, which is what makes this usable from
/// [`super::throw`]: an uncaught value that is not an error must not be
/// described as `"Error"`.
///
/// Nothing here runs user code. Both fields are read through
/// [`super::objects::read_property`], which answers data properties and walks
/// the chain — a getter is the accessor path and is deliberately not this one,
/// because the caller may be a program that has already failed.
pub(super) fn joined(context: &mut Context, cell: u32) -> Option<String> {
    let read = |context: &mut Context, field: &str| {
        let key = context.well_known(field);
        let found = super::objects::read_property(context, cell, key)?;
        super::text::to_text(context, found)?.to_rust()
    };
    let name = read(context, "name");
    let message = read(context, "message");
    match (name, message) {
        (None, None) => None,
        (name, Some(message)) if !message.is_empty() => {
            Some(format!("{}: {message}", name.unwrap_or_else(|| "Error".to_owned())))
        }
        (name, _) => Some(name.unwrap_or_else(|| "Error".to_owned())),
    }
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
