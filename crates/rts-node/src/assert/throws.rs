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
//! # What is still not here, and why that is not a stub
//!
//! `rejects` and `doesNotReject` stay refused, for the reason that has not
//! changed: they need a rejection reason read from a promise and a promise that
//! settles later, and `entry::settled` mints only an already-settled one. A
//! rejection is not a throw, so nothing in this file reaches them.

use rts_core::entry;

use super::values::{self, member, report_failure, string_of};

/// What an expectation says about the error that was raised.
///
/// Three answers and not two, because "this is not the error you named" is not
/// the same as "this assertion failed" — see [`Verdict::Reraise`].
enum Verdict {
    /// The error is what the caller said to expect.
    Matched,
    /// It is not, and that is this assertion failing — with this detail.
    Failed(String),
    /// It is not, and the ORIGINAL error is what should travel on.
    Reraise,
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
        // step 1 lives.
        if entry::instance_of(error, expected) {
            return Verdict::Matched;
        }
        // An error CLASS that did not match re-raises the original rather than
        // reporting a mismatch. Node's rule, and it reads backwards until you
        // see what it protects: a test that named `TypeError` and met a
        // `RangeError` hit a DIFFERENT bug, and burying that under "expected
        // TypeError" loses the one that matters.
        if inherits_from_error(expected) {
            return Verdict::Reraise;
        }
        // Otherwise a validation function: Node calls it with the error and
        // reads the answer as the verdict.
        let absent = entry::undefined_value();
        let answer = entry::call(expected, absent, error, absent, absent, absent);
        if entry::thrown() != 0 {
            let raised = entry::take_thrown();
            return Verdict::Failed(format!(
                "the validation function itself threw: {}",
                values::render(raised)
            ));
        }
        return match entry::to_boolean(answer) {
            true => Verdict::Matched,
            false => Verdict::Failed(format!(
                "The validation function answered false. Raised: {text}"
            )),
        };
    }
    // An object: every own property must be on the error and equal to it.
    // `same_value` and not `===`, for the reason `equality` documents — a test
    // asserting a `NaN` field would never pass under `===`.
    for name in values::own_key_strings(expected) {
        let wanted = member(expected, &name);
        let held = member(error, &name);
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
        let detail = values::describe_message(message, "Missing expected exception.");
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
        // Put it back exactly as it arrived: the program asked about one error
        // and met another, and the other is the news.
        Verdict::Reraise => entry::throw_value(error),
    }
    absent
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
