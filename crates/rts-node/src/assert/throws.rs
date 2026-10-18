//! `assert.throws` and `assert.doesNotThrow` — assertions defined by what a
//! function raises.
//!
//! # Why these were refused, and what changed
//!
//! The module doc listed both as *"not approximable"*, with a reason that was
//! true when it was written: *"a throw inside `fn` exits the process before this
//! native resumes, so there is nothing to catch and nothing to inspect"*.
//!
//! That is no longer how a throw travels. A raise is RECORDED — `entry::throw`
//! writes a thread slot — and every compiled call site checks it, so a native
//! that calls user code can ask whether the callee left one behind instead of
//! being skipped over. That is rule 8 of `rts-core`'s README, and it is what
//! makes these two honest rather than fabricated: what is reported here is a
//! throw that was actually seen.
//!
//! Refusing them was not free. Measured 2026-08-24 against Node's own suite,
//! `assert.throws is not a function` was the **second most frequent cause of
//! death across the whole corpus** — 233 files of 3 543, spread over every
//! module, because a test that checks an error path is how a library's edges
//! get tested at all.
//!
//! # Rejections
//!
//! `rejects` and `doesNotReject` react to the promise with `bind`-carried natives
//! (`drive`): a stateless native cannot await, but it can be a `then` reaction.

use rts_core::entry;

use super::values::{self, member, report_failure, string_of};

/// What an expectation says about the error that was raised.
///
/// Two answers: it is what the caller said to expect, or the assertion failed.
/// A class that did not match FAILS the assertion (an `AssertionError`) rather
/// than re-raising the original, which is what Node does.
pub(super) enum Verdict {
    /// The error is what the caller said to expect.
    Matched,
    /// It is not, and that is this assertion failing — with this detail.
    Failed(String),
}

/// Runs `fn` and answers what it raised, or `None` if it returned.
///
/// # Why the throw is TAKEN rather than looked at
///
/// Because the slot is the mechanism by which a throw travels: leaving one in
/// flight would make this assertion's own `return` look like a raise to the
/// caller, and the program would unwind from a line that succeeded. Taking it
/// is what turns "the callee threw" into a value this function owns — which is
/// exactly the contract these two assertions are about.
fn raised_by(callee: u64) -> Option<u64> {
    let absent = entry::undefined_value();
    entry::call(callee, absent, absent, absent, absent, absent);
    match entry::thrown() {
        0 => None,
        _ => Some(entry::take_thrown()),
    }
}

/// Whether `callee` is an error class — the global `Error`, or anything whose
/// prototype chain reaches it.
///
/// # Why the chain, and not "does it have a `prototype`"
///
/// Because a validation function written as `function (e) { … }` has a
/// `prototype` too, and treating that as a constructor would refuse the form
/// much of Node's own corpus uses. The chain is what actually separates
/// `class Mine extends Error` from a function someone wrote to check a field.
///
/// `Error` ITSELF counts here, where Node's own `Error.isPrototypeOf(expected)`
/// answers false for it. A deliberate divergence, and the smaller of the two
/// available: Node then calls `Error` as a validation function, which answers a
/// new error object — truthy — so the assertion passes for a value that is not
/// an error at all.
fn inherits_from_error(callee: u64) -> bool {
    let error_class = entry::with_runtime(|context| {
        let global = entry::global_object(context);
        entry::get_member(context, global, "Error")
    });
    if error_class == entry::undefined_value() {
        return false;
    }
    let mut at = callee;
    // Bounded. A prototype chain is finite and a cycle is a heap this engine
    // cannot build, but an unbounded loop inside a native is a hang — and a
    // hang is a worse answer to read than a wrong one.
    for _ in 0..64 {
        if at == error_class {
            return true;
        }
        let next = entry::get_prototype(at);
        if next == at || !entry::with_runtime(|context| entry::is_object(context, next)) {
            return false;
        }
        at = next;
    }
    false
}

/// Whether `error` satisfies what the caller said to expect.
///
/// # The four shapes, and why a string is not one of them
///
/// Node reads `assert.throws(fn, "text")` as the MESSAGE argument, not as a
/// matcher — it even warns about the mistake, because a string matcher that
/// silently passed would make a whole suite of error tests vacuous. So a string
/// never reaches here: [`throws`] shifts it into `message` first.
///
/// What does reach here: a `RegExp` (its own `test`, called rather than
/// re-implemented — `rts-core` owns what a pattern means), a constructor (an
/// `instanceof` test), a validation function (its answer is the verdict), and
/// an object (every own property must match the error's).
fn satisfies(error: u64, expected: u64) -> Verdict {
    let text = values::render(error);
    // A `RegExp` before the callable test, because a regular expression object
    // is not callable but IS matched by pattern — asking `is_callable` first
    // would fall through to the object arm and compare `source` and `flags` as
    // ordinary properties.
    let tester = member(expected, "test");
    if values::is_callable(tester) && member(expected, "source") != entry::undefined_value() {
        let absent = entry::undefined_value();
        let matched = entry::call(tester, expected, values::string(&text), absent, absent, absent);
        return match entry::to_boolean(matched) {
            true => Verdict::Matched,
            false => Verdict::Failed(format!(
                "The input did not match the regular expression. Input:\n\n'{text}'\n"
            )),
        };
    }
    if values::is_callable(expected) {
        // A CONSTRUCTOR first: `assert.throws(fn, TypeError)` is the common
        // form, and a class is callable too. `instance_of` decides, which is
        // the answer the `instanceof` operator gives — including for a class
        // that defines `Symbol.hasInstance`, since that is where the operator's
        // step 1 lives. Only a function WITH a `prototype` is asked: an arrow
        // function is a validator, and `instanceof` on it is a TypeError.
        if member(expected, "prototype") != entry::undefined_value()
            && entry::instance_of(error, expected)
        {
            return Verdict::Matched;
        }
        // An error CLASS that did not match FAILS the assertion, with Node's
        // wording: the program named one class and met another.
        if inherits_from_error(expected) {
            let wanted = string_of(member(expected, "name")).unwrap_or_default();
            let mut detail = format!("The error is expected to be an instance of \"{wanted}\"");
            match string_of(member(member(error, "constructor"), "name")) {
                Some(name) if !name.is_empty() => detail.push_str(&format!(". Received \"{name}\"")),
                _ => detail.push_str(&format!(". Received \"{text}\"")),
            }
            if let Some(message) = string_of(member(error, "message")) {
                detail.push_str(&format!("\n\nError message:\n\n{message}"));
            }
            return Verdict::Failed(detail);
        }
        // Otherwise a validation function: Node calls it with the error and
        // requires the answer to be exactly `true`.
        let absent = entry::undefined_value();
        let answer = entry::call(expected, absent, error, absent, absent, absent);
        if entry::thrown() != 0 {
            let raised = entry::take_thrown();
            return Verdict::Failed(format!(
                "the validation function itself threw: {}",
                values::render(raised)
            ));
        }
        if answer == entry::boolean_value(true) {
            return Verdict::Matched;
        }
        let named = match string_of(member(expected, "name")) {
            Some(name) if !name.is_empty() => format!("\"{name}\" "),
            _ => String::new(),
        };
        return Verdict::Failed(format!(
            "The {named}validation function is expected to return \"true\". Received {}\n\nCaught error:\n\n{text}",
            values::render(answer)
        ));
    }
    // An object: every own property must be on the error and equal to it.
    // `same_value` and not `===`, for the reason `equality` documents — a test
    // asserting a `NaN` field would never pass under `===`.
    for name in values::own_key_strings(expected) {
        let wanted = member(expected, &name);
        let held = member(error, &name);
        // Object matchers may use a RegExp for a field such as `message`.
        // Treat it like the top-level matcher above instead of comparing the
        // RegExp object identity with the actual string value.
        let tester = member(wanted, "test");
        if values::is_callable(tester) && member(wanted, "source") != entry::undefined_value() {
            let actual = string_of(held).unwrap_or_else(|| values::render(held));
            let matched = entry::call(
                tester,
                wanted,
                values::string(&actual),
                entry::undefined_value(),
                entry::undefined_value(),
                entry::undefined_value(),
            );
            if !entry::to_boolean(matched) {
                return Verdict::Failed(format!(
                    "Expected the error's `{name}` to match {}, got {}",
                    values::render(wanted),
                    actual
                ));
            }
            continue;
        }
        if !super::equality::same_value(held, wanted) {
            return Verdict::Failed(format!(
                "Expected the error's `{name}` to be {}, got {}",
                values::render(wanted),
                values::render(held)
            ));
        }
    }
    Verdict::Matched
}

/// The shift Node performs and warns about: a string in the second position is
/// the MESSAGE, never a matcher.
///
/// See [`satisfies`] for why a string matcher would be worse than no matcher:
/// it would make every error test that used one vacuous.
fn shift(expected: u64, message: u64) -> (u64, u64) {
    let absent = entry::undefined_value();
    match string_of(expected) {
        Some(_) if message == absent => (absent, expected),
        _ => (expected, message),
    }
}

/// `assert.throws(fn[, expected[, message]])`.
pub(super) extern "C" fn throws(
    _e: u64,
    _this: u64,
    callee: u64,
    expected: u64,
    message: u64,
    _d: u64,
) -> u64 {
    let absent = entry::undefined_value();
    if !values::is_callable(callee) {
        report_failure("throws", "The \"fn\" argument must be of type function");
        return absent;
    }
    let (expected, message) = shift(expected, message);

    let Some(error) = raised_by(callee) else {
        let detail = missing_detail("exception", expected, message);
        report_failure("throws", &detail);
        return absent;
    };
    if expected == absent {
        return absent;
    }
    match satisfies(error, expected) {
        Verdict::Matched => {}
        Verdict::Failed(why) => {
            let detail = values::describe_message(message, &why);
            report_failure("throws", &detail);
        }
    }
    absent
}

/// Node's tail for a missing throw/rejection: ` (Name)` when `expected` names a
/// class, then `: message` or a period.
fn missing_detail(kind: &str, expected: u64, message: u64) -> String {
    let mut details = String::new();
    // Not read off `undefined`: that is a raise, and it would replace this one.
    if expected != entry::undefined_value() {
        if let Some(name) = string_of(member(expected, "name")) {
            if !name.is_empty() {
                details.push_str(&format!(" ({name})"));
            }
        }
    }
    match message == entry::undefined_value() {
        true => details.push('.'),
        false => details.push_str(&format!(": {}", values::describe_message(message, ""))),
    }
    format!("Missing expected {kind}{details}")
}

// The three reactions below take `([expected, message], settled)` — the first
// arrives pre-bound (`Function.prototype.bind`), because a stateless native has
// nowhere else to carry what the assertion was asked.

/// `rejects`' reaction to a rejection: the same verdict `throws` gives.
extern "C" fn rejection_check(_e: u64, _this: u64, pack: u64, error: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (expected, message) = unpack(pack);
    if expected == absent {
        return absent;
    }
    if let Verdict::Failed(why) = satisfies(error, expected) {
        report_failure("rejects", &values::describe_message(message, &why));
    }
    absent
}

/// `rejects`' reaction to a fulfilment: it wanted a rejection.
extern "C" fn rejection_missing(_e: u64, _this: u64, pack: u64, _value: u64, _c: u64, _d: u64) -> u64 {
    let (expected, message) = unpack(pack);
    report_failure("rejects", &missing_detail("rejection", expected, message));
    entry::undefined_value()
}

/// `doesNotReject`' reaction to a rejection: it wanted a fulfilment.
extern "C" fn rejection_unwanted(_e: u64, _this: u64, pack: u64, error: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (expected, message) = unpack(pack);
    if expected != absent && !matches!(satisfies(error, expected), Verdict::Matched) {
        entry::throw_value(error);
        return absent;
    }
    let detail = values::describe_message(
        message,
        &format!("Got unwanted rejection.\nActual message: {}", values::render(error)),
    );
    report_failure("doesNotReject", &detail);
    absent
}

/// The packed `(expected, message)`, shifted as `throws` shifts them.
fn unpack(pack: u64) -> (u64, u64) {
    let at = |index: f64| entry::get_indexed(pack, entry::make_number(index));
    shift(at(0.0), at(1.0))
}

/// A reaction that does nothing — `doesNotReject` on a fulfilment.
extern "C" fn nothing(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}

/// `native.bind(undefined, [expected, message])` — packed in one array, because a
/// trailing `undefined` argument is not counted and would shift the settled
/// value into its place.
fn bound(code: super::Provided, expected: u64, message: u64) -> u64 {
    let absent = entry::undefined_value();
    let native = entry::with_runtime(|context| entry::make_callable(context, code));
    let bind = member(native, "bind");
    let pack = entry::make_array(vec![expected, message]);
    entry::call(bind, native, absent, pack, absent, absent)
}

/// Reacts to `subject` (a promise, or a function returning one) with the two
/// reactions and answers the resulting promise.
fn drive(on_fulfilled: super::Provided, on_rejected: super::Provided, subject: u64, expected: u64, message: u64) -> u64 {
    let absent = entry::undefined_value();
    let mut promise = subject;
    if values::is_callable(subject) {
        promise = entry::call(subject, absent, absent, absent, absent, absent);
        if entry::thrown() != 0 {
            // A synchronous raise is a rejection, as it is for an async function.
            let raised = entry::take_thrown();
            let reject = member(values::global_member("Promise"), "reject");
            promise = entry::call(reject, values::global_member("Promise"), raised, absent, absent, absent);
        }
    }
    if !values::is_callable(member(promise, "then")) {
        entry::throw_type_error(&format!(
            "Expected instance of Promise to be returned from the \"promiseFn\" function but got {}.",
            values::render(promise)
        ));
        return absent;
    }
    let fulfilled = bound(on_fulfilled, expected, message);
    let rejected = bound(on_rejected, expected, message);
    entry::call(member(promise, "then"), promise, fulfilled, rejected, absent, absent)
}

/// `assert.rejects(promiseFn, expected?, message?)`.
pub(super) extern "C" fn rejects(_e: u64, _this: u64, subject: u64, expected: u64, message: u64, _d: u64) -> u64 {
    drive(rejection_missing, rejection_check, subject, expected, message)
}

/// `assert.doesNotReject(promiseFn, expected?, message?)`.
pub(super) extern "C" fn does_not_reject(_e: u64, _this: u64, subject: u64, expected: u64, message: u64, _d: u64) -> u64 {
    drive(nothing, rejection_unwanted, subject, expected, message)
}

/// `assert.doesNotThrow(fn[, expected[, message]])`.
///
/// `expected` only NARROWS the assertion here, which is Node's rule: a throw
/// that does not match what was named is not this assertion's business, so it is
/// re-raised rather than reported. Swallowing it would turn an unexpected
/// failure into a pass.
pub(super) extern "C" fn does_not_throw(
    _e: u64,
    _this: u64,
    callee: u64,
    expected: u64,
    message: u64,
    _d: u64,
) -> u64 {
    let absent = entry::undefined_value();
    if !values::is_callable(callee) {
        report_failure("doesNotThrow", "The \"fn\" argument must be of type function");
        return absent;
    }
    let (expected, message) = shift(expected, message);
    let Some(error) = raised_by(callee) else {
        return absent;
    };
    if expected != absent && !matches!(satisfies(error, expected), Verdict::Matched) {
        entry::throw_value(error);
        return absent;
    }
    let detail = values::describe_message(
        message,
        &format!(
            "Got unwanted exception.\nActual message: {}",
            values::render(error)
        ),
    );
    report_failure("doesNotThrow", &detail);
    absent
}
