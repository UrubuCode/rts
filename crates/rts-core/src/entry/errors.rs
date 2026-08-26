//! The `ERR_*` errors Node's own APIs raise when an argument is wrong.
//!
//! # Why this lives in the runtime and not in `rts-node`
//!
//! It was written in `rts-node::errors` first, and for `fs`, `zlib` and
//! `child_process` that is still where it is reached from — those wrappers now
//! delegate here. What moved it is `Buffer`: the class is **not** in `rts-node`
//! (`docs/reference/node/layering.md` §6 puts bytes and codecs in the runtime,
//! because they need no operating system), so its argument checks are written
//! in `entry::buffer` and cannot call a `pub(crate)` function of a crate that
//! depends on this one.
//!
//! The rejected alternative was a second copy beside `Buffer` — which is
//! exactly the drift `rts-node::errors`' own doc argues against: a program does
//! not read the message, it reads the CODE, and two spellings of
//! `ERR_INVALID_ARG_TYPE` fail a test that says nothing about the module it is
//! testing. So the raising half moved down to where every caller can see it,
//! and nothing was copied.
//!
//! # What is deliberately NOT here
//!
//! A validator per type (`validateString`, `validateInteger`, …), which is what
//! Node's own `internal/validators` is. This is only the RAISING half: deciding
//! what a given API accepts is that API's business and lives with it, because
//! the answer differs — `fs.chown` wants an integer uid where `Buffer.alloc`
//! wants a byte count.

use super::with_current;

/// Raises `TypeError [ERR_INVALID_ARG_TYPE]` for a wrong primitive type.
///
/// `name` is the argument as the documentation spells it (`"size"`, `"path"`),
/// `expected` is the type it must have (`"number"`, `"string"`), and `actual`
/// is what arrived. The message is Node's, word for word, because a test
/// comparing it is comparing against that text.
pub fn invalid_arg_type(name: &str, expected: &str, actual: u64) {
    let described = kind_text(actual);
    raise(
        "TypeError",
        "ERR_INVALID_ARG_TYPE",
        &format!("The \"{name}\" argument must be of type {expected}. Received {described}"),
    );
}

/// Raises `TypeError [ERR_INVALID_ARG_TYPE]` for a wrong CLASS.
///
/// The same code and a different sentence, which is Node's own split:
/// *"must be of type number"* for a primitive against *"must be an instance of
/// Buffer or Uint8Array"* for an object. `test-buffer-compare.js` asserts the
/// second wording literally, so the two cannot share one phrasing.
pub fn invalid_arg_instance(name: &str, expected: &str, actual: u64) {
    let described = kind_text(actual);
    raise(
        "TypeError",
        "ERR_INVALID_ARG_TYPE",
        &format!("The \"{name}\" argument must be an instance of {expected}. Received {described}"),
    );
}

/// Raises `RangeError [ERR_OUT_OF_RANGE]`.
pub fn out_of_range(name: &str, expected: &str, actual: u64) {
    let described = value_text(actual);
    raise(
        "RangeError",
        "ERR_OUT_OF_RANGE",
        &format!(
            "The value of \"{name}\" is out of range. It must be {expected}. Received {described}"
        ),
    );
}

/// Raises `TypeError [ERR_INVALID_STATE]` for an operation on detached state.
pub fn invalid_state(message: &str) {
    raise("TypeError", "ERR_INVALID_STATE", message);
}

/// Raises `TypeError [ERR_INVALID_ARG_VALUE]`.
pub fn invalid_arg_value(name: &str, actual: u64, reason: &str) {
    let described = value_text(actual);
    raise(
        "TypeError",
        "ERR_INVALID_ARG_VALUE",
        &format!("The argument '{name}' {reason}. Received {described}"),
    );
}

/// Raises `TypeError [ERR_UNKNOWN_ENCODING]`.
///
/// A name apart from [`invalid_arg_value`] because Node gives it its own code
/// and its own sentence — `Buffer.from('', 'buffer')` is a spelling mistake in
/// a fixed vocabulary, not a value out of range — and the tests match on the
/// code.
pub fn unknown_encoding(encoding: &str) {
    raise(
        "TypeError",
        "ERR_UNKNOWN_ENCODING",
        &format!("Unknown encoding: {encoding}"),
    );
}

/// Raises `RangeError [ERR_BUFFER_OUT_OF_BOUNDS]`.
///
/// Node's own message has two forms: with an argument name for a bound that was
/// passed (`"offset" is outside of buffer bounds`) and without one for an
/// operation that ran off the end by itself.
pub fn buffer_out_of_bounds(name: Option<&str>) {
    let message = match name {
        Some(name) => format!("\"{name}\" is outside of buffer bounds"),
        None => String::from("Attempt to access memory outside buffer bounds"),
    };
    raise("RangeError", "ERR_BUFFER_OUT_OF_BOUNDS", &message);
}

/// Builds an error of `class`, stamps `code` on it, and raises it.
///
/// # Why the class name crosses as text
///
/// Because the classes are the program's own — a `TypeError` a native raises
/// must be the same `TypeError` the program's `catch (e) { e instanceof
/// TypeError }` compares against, and `make_named_error` is what reaches the
/// one the runtime installed. Building an object here and setting a `name`
/// string on it would answer every question about the error correctly except
/// the one that matters.
fn raise(class: &str, code: &str, message: &str) {
    let Some(error) = super::throw::make_named_error(class, message) else {
        // No class to build from: a program so early that the primordials are
        // not installed. Raising nothing would let the call carry on with the
        // argument it was going to refuse, so the plain type error stands in.
        super::throw::type_error(message);
        return;
    };
    let code = code.to_owned();
    with_current(|context| {
        let held = super::modules::make_string(context, &code);
        super::modules::put_member(context, error, "code", held);
    });
    super::throw::throw_value(error);
}

/// What a value is, in the words Node's `ERR_INVALID_ARG_TYPE` uses.
///
/// Node writes `type number` for a primitive, `an instance of Buffer` for an
/// object with a class, and the literal `null`/`undefined` for those two —
/// which reads oddly until you see it in the message: *"Received null"* rather
/// than *"Received type object"*, because `typeof null` is the mistake from
/// 1995 and Node declines to repeat it in a diagnostic.
///
fn kind_text(value: u64) -> String {
    with_current(|context| {
        if value == super::modules::undefined_in(context) {
            return String::from("undefined");
        }
        if value == super::modules::null_in(context) {
            return String::from("null");
        }
        if let Some(text) = super::modules::string_in(context, value) {
            // By CHARACTER and not by byte. `&text[..25]` panics when byte 25
            // lands inside a multi-byte character — and a panic in an
            // `extern "C"` frame has no unwind path, so it takes the process
            // down. A diagnostic about a bad argument that kills the program
            // when the argument contains an accent is worse than the bug it was
            // reporting.
            let shown = match text.chars().count() > 25 {
                true => format!("{}...", text.chars().take(25).collect::<String>()),
                false => text,
            };
            return format!("type string ('{shown}')");
        }
        if let Some(number) = super::modules::number_of(value) {
            return format!("type number ({number})");
        }
        if super::modules::is_callable_in(context, value) {
            return String::from("function");
        }
        if super::modules::is_array_in(context, value) {
            return String::from("an instance of Array");
        }
        if super::modules::is_object(context, value) {
            return String::from("an instance of Object");
        }
        // The value, and not just the type. Node writes `type boolean (true)`,
        // and `node:zlib`'s suite compares the whole sentence — a message that
        // stops at the type fails a test about an argument check that is
        // otherwise correct. Everything that reaches here is a boolean or a
        // symbol; `to_boolean_in` decides which word.
        //
        // `_in` and NOT the ambient `to_boolean`, which takes a borrow of its
        // own — called from inside this one it panics with "RefCell already
        // borrowed", and a panic in an `extern "C"` frame aborts the process.
        // That is what the first version of this line did: a `dgram.send(true,
        // …)` produced a correct refusal and then killed the program while
        // rendering the message for it.
        let shown = super::primitives::to_boolean_in(context, value);
        format!("type boolean ({shown})")
    })
}

/// A value as a message renders it — the same rendering, without the "type".
fn value_text(value: u64) -> String {
    with_current(|context| match super::modules::text_in(context, value) {
        Some(text) => text,
        None => String::from("undefined"),
    })
}
