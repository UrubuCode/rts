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

/// `Error.prototype.toString` — `name` and `message` joined the way the
/// specification joins them.
///
/// # Why this does not go through [`joined`]
///
/// Because the two answer different questions and only one of them may run user
/// code. `joined` is what an UNCAUGHT throw prints, and a program that has
/// already failed must not be asked to run a getter to describe its own failure;
/// this is `Error.prototype.toString`, which the specification writes in terms
/// of `Get` and `ToString` — so `err.name = 7` prints `7`, an object with a
/// `toString` prints what it answers, and a `name` accessor runs.
///
/// It went through `joined` and inherited three wrong answers from doing so, all
/// three of them the same mistake — reading a property's ABSENCE and its
/// `undefined` as the same thing. `{ name: undefined }` printed `"undefined: m"`
/// where the language substitutes `"Error"`, `{ message: undefined }` printed a
/// trailing `": undefined"` where it substitutes the empty string, and a `name`
/// of `""` printed a leading `": "` where the language answers the message
/// alone.
fn described(this: u64) -> u64 {
    // `Error.prototype.toString.call(1)` is a `TypeError`, not a description of
    // the number. `as_slot` is the wrong test for it — a string primitive has a
    // cell — so this asks the same "is it an object" every other coercion here
    // asks.
    if !with_current(|context| super::objects::is_object(context, this)) {
        super::throw::type_error("Error.prototype.toString called on non-object");
        return with_current(|context| undefined_of(context));
    }
    let Some(name) = field_text(this, "name", "Error") else {
        return with_current(|context| undefined_of(context));
    };
    let Some(message) = field_text(this, "message", "") else {
        return with_current(|context| undefined_of(context));
    };
    let joined = match (name.is_empty(), message.is_empty()) {
        (true, _) => message,
        (false, true) => name,
        (false, false) => format!("{name}: {message}"),
    };
    with_current(|context| context.intern_value(Str::from_str(&joined)).bits())
}

/// One of `toString`'s two fields: `Get` then `ToString`, with a default for
/// `undefined`.
///
/// The default is what the specification substitutes and it is substituted for
/// `undefined` ALONE — a missing property reads `undefined` through the chain
/// and lands here the same way, which is why one test covers both. Every other
/// value converts, `null` and `0` included: `{ name: null }` describes itself as
/// `"null"` in every runtime.
///
/// `None` is a throw in flight — the getter's or the conversion's — which the
/// caller propagates under rule 8.
fn field_text(this: u64, field: &str, default: &str) -> Option<String> {
    let key = with_current(|context| context.well_known_text(field));
    let found = super::computed::get_indexed(this, key);
    if super::throw::in_flight() {
        return None;
    }
    if found == with_current(|context| undefined_of(context)) {
        return Some(default.to_owned());
    }
    let text = super::text::to_string_value(found)?;
    with_current(|context| {
        super::text::to_text(context, Value(text))
            .and_then(|held| held.to_rust())
            .or(Some(String::new()))
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
        // `undefined` is ABSENT here, not the word. A property that is not there
        // and one holding `undefined` are the same thing to `Error.prototype.
        // toString`, which substitutes its default for both — and reading the
        // word is what printed `undefined: boom` for `err.name = undefined`.
        if found.bits() == undefined_of(context) {
            return None;
        }
        super::text::to_text(context, found)?.to_rust()
    };
    let name = read(context, "name");
    let message = read(context, "message");
    if name.is_none() && message.is_none() {
        return None;
    }
    let name = name.unwrap_or_else(|| "Error".to_owned());
    let message = message.unwrap_or_default();
    // An EMPTY name answers the message alone, which is the third arm the
    // language spells out and the one a `{ name: "" }` reaches: the join is
    // `name: message` only when there are two halves to join.
    Some(match (name.is_empty(), message.is_empty()) {
        (true, _) => message,
        (false, true) => name,
        (false, false) => format!("{name}: {message}"),
    })
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

/// Puts the `stack` accessor on `Error.prototype`, once per context.
///
/// ON THE PROTOTYPE, not on each instance. Per instance was refused by reading
/// `integrity::retype`, which `define_accessor_and_invalidate` calls: it
/// declares a FRESH TYPE for the cell. Doing that per construction would mint a
/// type per Error and invalidate every inline cache that has ever seen one —
/// more expensive than the thing it replaces, and paid by unrelated code. The
/// six subclasses inherit it, because their prototypes chain to this one.
///
/// AT CONSTRUCTION, not at registration, and that is not tidiness.
/// `register_type_error` and its five siblings reach `register_error` directly
/// through the macro's `extends`, so an internal `TypeError` — one this engine
/// throws itself — builds `Error.prototype` without passing through this
/// module's `provided`. Installing there worked under `rts run` and left the
/// 332 tests that share one process reading `undefined` from every `.stack`.
fn install_stack_accessor(context: &mut Context) {
    if context.stack_accessor {
        return;
    }
    if let Some(prototype) = super::class_support::prototype(context, "Error") {
        context.stack_accessor = true;
        super::accessor::define_accessor_in(context, prototype, "stack", stack_get, Some(stack_set));
    }
}

/// `err.stack` — rendered here, on the first read, and never again.
///
/// The first read installs an OWN data property and drops the captured frames,
/// so a second read is an ordinary cached property read rather than a second
/// call through here. That also means a program that reads `.stack` twice pays
/// what it used to pay once, and one that never reads it pays nothing.
extern "C" fn stack_get(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(cell) = Value(this).as_slot() else {
            return undefined_of(context);
        };
        // Already rendered, or written by the setter: the own property answers
        // and this accessor is only reached because the own one is absent.
        let Some((class, frames)) = context.take_stack(cell) else {
            return undefined_of(context);
        };
        let described = joined(context, cell).unwrap_or_else(|| class.to_owned());
        let stack = format!("{described}{}", super::throw::stack_text_of(context, &frames));
        let value = context.intern_value(Str::from_str(&stack)).bits();
        let key = context.well_known("stack");
        super::objects::put(context, cell, key, value);
        super::native::hidden(context, cell, key);
        value
    })
}

/// `err.stack = v` — an own data property, which is what a write to it makes in
/// every engine, and what drops the captured frames.
extern "C" fn stack_set(_e: u64, this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(cell) = Value(this).as_slot() else {
            return undefined_of(context);
        };
        context.take_stack(cell);
        let key = context.well_known("stack");
        super::objects::put(context, cell, key, value);
        super::native::hidden(context, cell, key);
        undefined_of(context)
    })
}
