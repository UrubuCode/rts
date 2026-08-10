//! Every assertion, as an `extern "C"` native.
//!
//! # Why the mode is in the function and not in an argument
//!
//! A native is a bare code address — [`Provided`](rts_core::entry::Provided)
//! carries no closure state — so `deepStrictEqual` and `deepEqual` cannot be one
//! function told which it is. That is not a workaround: it is what makes
//! `assert.strict.equal` and `new Assert({ strict: false }).equal` work when
//! they are DESTRUCTURED off their object, which Node documents as a caveat its
//! own implementation has to take care to preserve. Here the binding is the
//! function identity itself, so there is nothing to lose.
//!
//! Each one is four lines: compare, report on failure, answer `undefined`. The
//! comparisons live in [`super::equality`] and the reporting in
//! [`super::values`], so a mode's meaning is stated once.

use super::equality::{LOOSE, PARTIAL, STRICT, STRICT_OPEN, deep_equal, loose_equal, same_value};
use super::values::{describe_message, render, report_failure, string_of};
use rts_core::entry;

/// `assert(value, message?)` / `assert.ok(value, message?)`.
pub(super) extern "C" fn ok(_e: u64, _this: u64, value: u64, message: u64, _c: u64, _d: u64) -> u64 {
    if !entry::to_boolean(value) {
        let generated = format!("The expression evaluated to a falsy value: {}", render(value));
        report_failure("ok", &describe_message(message, &generated));
    }
    entry::undefined_value()
}

/// `assert.equal(actual, expected, message?)` — legacy `==`, with `NaN`
/// equalling `NaN`.
pub(super) extern "C" fn equal(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    check(loose_equal(actual, expected), "==", actual, expected, message, "loosely equal")
}

/// `assert.notEqual(actual, expected, message?)`.
pub(super) extern "C" fn not_equal(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    check(!loose_equal(actual, expected), "!=", actual, expected, message, "not loosely equal")
}

/// `assert.strictEqual(actual, expected, message?)` — `Object.is`, which is
/// what Node specifies here and is NOT `===`; see
/// [`same_value`](super::equality::same_value).
pub(super) extern "C" fn strict_equal(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    check(same_value(actual, expected), "strictEqual", actual, expected, message, "strictly equal")
}

/// `assert.notStrictEqual(actual, expected, message?)`.
pub(super) extern "C" fn not_strict_equal(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    check(!same_value(actual, expected), "notStrictEqual", actual, expected, message, "not strictly equal")
}

/// `assert.deepEqual(actual, expected, message?)` — the legacy comparison.
pub(super) extern "C" fn deep_equal_loose(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    check(deep_equal(actual, expected, LOOSE), "deepEqual", actual, expected, message, "deeply equal")
}

/// `assert.notDeepEqual(actual, expected, message?)`.
pub(super) extern "C" fn not_deep_equal_loose(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    check(!deep_equal(actual, expected, LOOSE), "notDeepEqual", actual, expected, message, "not deeply equal")
}

/// `assert.deepStrictEqual(actual, expected, message?)`.
pub(super) extern "C" fn deep_strict_equal(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    let held = deep_equal(actual, expected, STRICT);
    check(held, "deepStrictEqual", actual, expected, message, "deeply and strictly equal")
}

/// `assert.notDeepStrictEqual(actual, expected, message?)`.
pub(super) extern "C" fn not_deep_strict_equal(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    let held = deep_equal(actual, expected, STRICT);
    check(!held, "notDeepStrictEqual", actual, expected, message, "not deeply and strictly equal")
}

/// `deepStrictEqual` for an `Assert` built with `skipPrototype: true`.
pub(super) extern "C" fn deep_strict_open(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    let held = deep_equal(actual, expected, STRICT_OPEN);
    check(held, "deepStrictEqual", actual, expected, message, "deeply and strictly equal")
}

/// `notDeepStrictEqual` for an `Assert` built with `skipPrototype: true`.
pub(super) extern "C" fn not_deep_strict_open(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    let held = deep_equal(actual, expected, STRICT_OPEN);
    check(!held, "notDeepStrictEqual", actual, expected, message, "not deeply and strictly equal")
}

/// `assert.partialDeepStrictEqual(actual, expected, message?)` — only the keys
/// `expected` carries are checked.
pub(super) extern "C" fn partial_deep_strict_equal(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    let held = deep_equal(actual, expected, PARTIAL);
    check(held, "partialDeepStrictEqual", actual, expected, message, "a superset of")
}

/// `assert.match(str, regexp, message?)`.
///
/// A non-string first argument is a failure rather than a coercion, which is
/// Node's rule: `assert.match(42, /4/)` throws there. Asking with `string_of`
/// rather than with a text conversion is what makes the two cases different at
/// all.
pub(super) extern "C" fn matches(_e: u64, _this: u64, text: u64, regexp: u64, message: u64, _d: u64) -> u64 {
    let held = string_of(text).is_some() && pattern_matches(regexp, text);
    if !held {
        let generated = format!("The input did not match the regular expression. Input: {}", render(text));
        report_failure("match", &describe_message(message, &generated));
    }
    entry::undefined_value()
}

/// `assert.doesNotMatch(str, regexp, message?)`.
pub(super) extern "C" fn does_not_match(_e: u64, _this: u64, text: u64, regexp: u64, message: u64, _d: u64) -> u64 {
    let held = string_of(text).is_some() && !pattern_matches(regexp, text);
    if !held {
        let generated = format!("The input was expected to not match the regular expression. Input: {}", render(text));
        report_failure("doesNotMatch", &describe_message(message, &generated));
    }
    entry::undefined_value()
}

/// `assert.fail(message?)`. Always reports — there is no condition to check.
pub(super) extern "C" fn fail(_e: u64, _this: u64, message: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    report_failure("fail", &describe_message(message, "Failed"));
    entry::undefined_value()
}

/// `assert.ifError(value)` — passes only for `null`/`undefined`.
///
/// Every other value fails, falsy ones included: Node tightened this in v10 and
/// `assert.ifError(0)` has thrown ever since.
pub(super) extern "C" fn if_error(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if value != absent && value != entry::null_value() {
        let detail = render(value);
        report_failure("ifError", &format!("ifError got unwanted exception: {detail}"));
    }
    absent
}

/// The shared tail of every two-operand check: report when it did not hold, and
/// answer `undefined` either way.
///
/// The generated message is Node's shape — `Expected values to be strictly
/// equal:` followed by the two values — because a failure this module cannot
/// throw is a failure a person reads, and "values are not equal" with no values
/// in it says nothing about which two.
fn check(held: bool, operator: &str, actual: u64, expected: u64, message: u64, relation: &str) -> u64 {
    if !held {
        let generated = format!(
            "Expected values to be {relation}:\n  actual:   {}\n  expected: {}",
            render(actual),
            render(expected)
        );
        report_failure(operator, &describe_message(message, &generated));
    }
    entry::undefined_value()
}

/// Whether `regexp.test(text)` is true, for a value shaped like a `RegExp`.
///
/// Reads the `test` method off the receiver and calls it — nothing here knows
/// what a `RegExp` is beyond that it has a callable `test` property, which is
/// the same duck-typing [`super::equality`]'s `Date` and `RegExp` rules use and the
/// same one every other member read in this crate does.
fn pattern_matches(regexp: u64, text: u64) -> bool {
    let absent = entry::undefined_value();
    let test = super::values::member(regexp, "test");
    if !super::values::is_callable(test) {
        return false;
    }
    entry::to_boolean(entry::call(test, regexp, text, absent, absent, absent))
}
