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
//! **`cause`.** It is the second argument's `cause` field, which is one more
//! object read, and nothing here reads it yet. It comes with a caller.
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

    /// `new Error(message)`.
    #[construct]
    fn build(this: u64, message: u64) -> u64 {
        written(this, message, "Error")
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

    /// `new TypeError(message)`.
    #[construct]
    fn build(this: u64, message: u64) -> u64 {
        written(this, message, "TypeError")
    }
}

/// `RangeError` — a value outside the set an operation accepts.
#[rtse::class("RangeError", extends = register_error)]
impl RangeError {
    /// The name every instance answers.
    const name: &str = "RangeError";

    /// `new RangeError(message)`.
    #[construct]
    fn build(this: u64, message: u64) -> u64 {
        written(this, message, "RangeError")
    }
}

/// `SyntaxError`.
#[rtse::class("SyntaxError", extends = register_error)]
impl SyntaxError {
    /// The name every instance answers.
    const name: &str = "SyntaxError";

    /// `new SyntaxError(message)`.
    #[construct]
    fn build(this: u64, message: u64) -> u64 {
        written(this, message, "SyntaxError")
    }
}

/// `ReferenceError`.
#[rtse::class("ReferenceError", extends = register_error)]
impl ReferenceError {
    /// The name every instance answers.
    const name: &str = "ReferenceError";

    /// `new ReferenceError(message)`.
    #[construct]
    fn build(this: u64, message: u64) -> u64 {
        written(this, message, "ReferenceError")
    }
}

/// `EvalError`, which nothing raises and every program may still catch.
#[rtse::class("EvalError", extends = register_error)]
impl EvalError {
    /// The name every instance answers.
    const name: &str = "EvalError";

    /// `new EvalError(message)`.
    #[construct]
    fn build(this: u64, message: u64) -> u64 {
        written(this, message, "EvalError")
    }
}

/// `URIError`.
#[rtse::class("URIError", extends = register_error)]
impl UriError {
    /// The name every instance answers.
    const name: &str = "URIError";

    /// `new URIError(message)`.
    #[construct]
    fn build(this: u64, message: u64) -> u64 {
        written(this, message, "URIError")
    }
}

/// `AggregateError` — several failures reported as one.
///
/// # Why this one is not another line beside its siblings
///
/// Every other member of the family differs from `Error` in nothing but its
/// name, which is why they are six near-identical declarations. This one takes
/// a SECOND argument and writes a second property: `new AggregateError(errors,
/// message)` carries the list, and `Promise.any` is the reason the language has
/// it — a rejection that is several rejections needs somewhere to put them.
///
/// The argument order is the language's and is easy to get backwards: the errors
/// come FIRST, unlike every other constructor in the family where the message
/// is the only argument.
#[rtse::class("AggregateError", extends = register_error)]
impl AggregateError {
    /// The name every instance answers.
    const name: &str = "AggregateError";

    /// `new AggregateError(errors, message)`.
    #[construct]
    fn build(this: u64, errors: u64, message: u64) -> u64 {
        let made = written(this, message, "AggregateError");
        with_current(|context| {
            let Some(cell) = Value(made).as_slot() else {
                return made;
            };
            // Written whatever it is, and NOT copied into a fresh array. The
            // language says `errors` is iterated and the result is an array; an
            // iteration here would call user code from an entry point, which is
            // the borrow rule this crate aborts on. So an array arrives as
            // itself and anything else arrives as itself — visible to a program
            // rather than silently emptied.
            let key = context.well_known("errors");
            super::objects::put(context, cell, key, errors);
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
        if message != undefined_of(context)
            && let Some(text) = super::text::to_text(context, Value(message))
        {
            let value = context.intern_value(text).bits();
            let key = context.well_known("message");
            super::objects::put(context, cell, key, value);
        }
        Value::from_slot(cell).bits()
    })
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
