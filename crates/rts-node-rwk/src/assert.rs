//! `node:assert` — synchronous invariant checks, without an exception to carry
//! the failure.
//!
//! # What a failed assertion does here
//!
//! Real `assert` throws. This engine's entry points cannot: [`crate::path`]'s
//! `call` answers `undefined` for a callee it cannot invoke and nothing here
//! emits the protected regions a `throw` needs, so a native cannot raise one
//! either. Silently returning as if the check passed would be worse than not
//! having the module — a program's control flow would look unaffected by a
//! failure it should have stopped on.
//!
//! So a failed assertion here does two things, both from
//! [`report_failure`]: it prints `AssertionError [operator]: <detail>` to
//! **stderr**, and it increments a process-wide counter. **What a program
//! sees**: the assertion function still returns `undefined` and the caller's
//! next statement still runs — this module cannot stop it — but the stderr
//! line is the record of what failed and in what order, which is what a test
//! harness capturing stderr, or a person reading the run's output, has to go
//! on. A passing assertion is silent, exactly as real `assert` is.
//!
//! # Not implemented, by name
//!
//! `throws`, `doesNotThrow`, `rejects`, `doesNotReject` — every one of these
//! is defined by catching an exception `fn()` raises, or a promise rejects
//! with. Neither is observable from a native here: nothing hands a native the
//! reason a call unwound (it cannot unwind), and there is no rejection value
//! to read off a promise from this side of the boundary. `equal`, `notEqual`,
//! `deepEqual`, `notDeepEqual` — the *legacy loose* comparisons; `==`-style
//! coercion is not one of the value operations this module was given, and
//! approximating it with `===` would be silently wrong on `assert.equal(1,
//! "1")`, which real Node passes. `partialDeepStrictEqual`, `AssertionError`
//! as a constructible class, `Assert`, `assert.strict`, `node:assert/strict`,
//! `CallTracker` (removed from Node itself).
//!
//! `deepStrictEqual` here has no cycle guard: a self-referential object is a
//! stack overflow, not the "stop and treat as equal" real Node documents.
//! There is also no `[[Prototype]]` comparison, no `Map`/`Set`/`RegExp`/typed-
//! array special casing, and no `Object.is` distinction between `0` and `-0`
//! — [`rts_core_rwk::entry::strict_equals`] is `===`, not `Object.is`, so
//! `deepStrictEqual(0, -0)` wrongly passes here. All of that is real Node
//! behavior this module does not reproduce.

use rts_core_rwk::entry::{Context, Provided};
use std::sync::atomic::{AtomicU64, Ordering};

/// How many assertions have failed since the process started.
///
/// Not exposed to a program — there is no member reading it back. It exists
/// so [`report_failure`]'s stderr line can say which failure this is, for a
/// harness scanning the log.
static FAILURES: AtomicU64 = AtomicU64::new(0);

/// The namespace `node:assert` is.
///
/// Built as a **callable** rather than a plain object — [`make_callable`]
/// over [`ok`] — so `assert(x)` works exactly as `assert.ok(x)` does, which is
/// what real Node's default export is. [`put_member`] still works on it: a
/// callable is a cell with a slot like any other, so hanging the rest of the
/// members off it costs nothing extra.
pub fn namespace(context: &mut Context) -> u64 {
    let callable = rts_core_rwk::entry::make_callable(context, ok);
    let members: &[(&str, Provided)] = &[
        ("ok", ok),
        ("strictEqual", strict_equal),
        ("notStrictEqual", not_strict_equal),
        ("deepStrictEqual", deep_strict_equal),
        ("notDeepStrictEqual", not_deep_strict_equal),
        ("match", matches),
        ("doesNotMatch", does_not_match),
        ("fail", fail),
        ("ifError", if_error),
    ];
    for (name, code) in members {
        let value = rts_core_rwk::entry::make_callable(context, *code);
        rts_core_rwk::entry::put_member(context, callable, name, value);
    }
    callable
}

/// `assert(value, message?)` / `assert.ok(value, message?)`.
extern "C" fn ok(_e: u64, _this: u64, value: u64, message: u64, _c: u64, _d: u64) -> u64 {
    if !rts_core_rwk::entry::to_boolean(value) {
        report_failure("ok", &describe_message(message, "the value is falsy"));
    }
    rts_core_rwk::entry::undefined_value()
}

/// `assert.strictEqual(actual, expected, message?)` — `===`.
extern "C" fn strict_equal(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    if !rts_core_rwk::entry::strict_equals(actual, expected) {
        report_failure("strictEqual", &describe_message(message, "values are not strictly equal"));
    }
    rts_core_rwk::entry::undefined_value()
}

/// `assert.notStrictEqual(actual, expected, message?)`.
extern "C" fn not_strict_equal(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    if rts_core_rwk::entry::strict_equals(actual, expected) {
        report_failure("notStrictEqual", &describe_message(message, "values are strictly equal"));
    }
    rts_core_rwk::entry::undefined_value()
}

/// `assert.deepStrictEqual(actual, expected, message?)`. See the module doc
/// for what this comparison is missing.
extern "C" fn deep_strict_equal(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    if !deep_equal(actual, expected, 0) {
        report_failure("deepStrictEqual", &describe_message(message, "values are not deeply equal"));
    }
    rts_core_rwk::entry::undefined_value()
}

/// `assert.notDeepStrictEqual(actual, expected, message?)`.
extern "C" fn not_deep_strict_equal(_e: u64, _this: u64, actual: u64, expected: u64, message: u64, _d: u64) -> u64 {
    if deep_equal(actual, expected, 0) {
        report_failure("notDeepStrictEqual", &describe_message(message, "values are deeply equal"));
    }
    rts_core_rwk::entry::undefined_value()
}

/// `assert.match(str, regexp, message?)`.
extern "C" fn matches(_e: u64, _this: u64, text: u64, regexp: u64, message: u64, _d: u64) -> u64 {
    if !regexp_test(regexp, text) {
        report_failure("match", &describe_message(message, "the string does not match"));
    }
    rts_core_rwk::entry::undefined_value()
}

/// `assert.doesNotMatch(str, regexp, message?)`.
extern "C" fn does_not_match(_e: u64, _this: u64, text: u64, regexp: u64, message: u64, _d: u64) -> u64 {
    if regexp_test(regexp, text) {
        report_failure("doesNotMatch", &describe_message(message, "the string matches"));
    }
    rts_core_rwk::entry::undefined_value()
}

/// `assert.fail(message?)`. Always reports — there is no condition to check.
extern "C" fn fail(_e: u64, _this: u64, message: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    report_failure("fail", &describe_message(message, "Failed"));
    rts_core_rwk::entry::undefined_value()
}

/// `assert.ifError(value)` — passes only for `null`/`undefined`.
extern "C" fn if_error(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = rts_core_rwk::entry::undefined_value();
    let none = rts_core_rwk::entry::null_value();
    if value != absent && value != none {
        let detail = rts_core_rwk::entry::described(value).unwrap_or_else(|| "<unprintable>".to_owned());
        report_failure("ifError", &format!("ifError got unwanted exception: {detail}"));
    }
    absent
}

/// Whether `regexp.test(text)` is true, for a value shaped like a `RegExp`.
///
/// Reads the `test` method off the receiver and calls it — nothing here knows
/// what a `RegExp` is beyond that it has a callable `test` property, which is
/// the same duck-typing every other member read in this crate does.
fn regexp_test(regexp: u64, text: u64) -> bool {
    let key = string_key("test");
    let test_fn = rts_core_rwk::entry::get_indexed(regexp, key);
    let absent = rts_core_rwk::entry::undefined_value();
    if test_fn == absent {
        return false;
    }
    let answer = rts_core_rwk::entry::call(test_fn, regexp, text, absent, absent, absent);
    rts_core_rwk::entry::to_boolean(answer)
}

/// A message argument as text, falling back to a default when absent.
///
/// `message` may be an `Error` in real Node, thrown as-is instead of wrapped.
/// There is no throw here to preserve that distinction, so an `Error` message
/// prints via [`described`] like any other value — a known, named divergence.
fn describe_message(message: u64, default_text: &str) -> String {
    let absent = rts_core_rwk::entry::undefined_value();
    if message == absent {
        return default_text.to_owned();
    }
    rts_core_rwk::entry::described(message).unwrap_or_else(|| default_text.to_owned())
}

/// The stderr line and counter bump every failing assertion produces.
fn report_failure(operator: &str, detail: &str) {
    let count = FAILURES.fetch_add(1, Ordering::Relaxed) + 1;
    eprintln!("AssertionError [{operator}] #{count}: {detail}");
}

/// Deep-strict-ish equality: `===`, then a key-by-key recursion over own
/// enumerable properties. See the module doc for what this drops relative to
/// real `deepStrictEqual`.
fn deep_equal(actual: u64, expected: u64, depth: u32) -> bool {
    if rts_core_rwk::entry::strict_equals(actual, expected) {
        return true;
    }
    // A bound on recursion depth stands in for the cycle guard real Node has
    // and this does not — it turns a hang into a wrong answer, which is the
    // failure mode this module can afford.
    if depth > 64 {
        return false;
    }
    let both_objects = rts_core_rwk::entry::text_of(rts_core_rwk::entry::type_of(actual)).as_deref() == Some("object")
        && rts_core_rwk::entry::text_of(rts_core_rwk::entry::type_of(expected)).as_deref() == Some("object");
    let none = rts_core_rwk::entry::null_value();
    if !both_objects || actual == none || expected == none {
        return false;
    }
    let actual_keys = collect_array(rts_core_rwk::entry::own_keys(actual));
    let expected_keys = collect_array(rts_core_rwk::entry::own_keys(expected));
    if actual_keys.len() != expected_keys.len() {
        return false;
    }
    for key in &expected_keys {
        let left = rts_core_rwk::entry::get_indexed(actual, *key);
        let right = rts_core_rwk::entry::get_indexed(expected, *key);
        if !deep_equal(left, right, depth + 1) {
            return false;
        }
    }
    true
}

/// A JS array's elements, read out through `.length` and indexed access —
/// the only way this crate is given to walk one from the host side.
fn collect_array(array: u64) -> Vec<u64> {
    let length_key = string_key("length");
    let length_value = rts_core_rwk::entry::get_indexed(array, length_key);
    // Asked of the value, not of its text: reading a number by parsing its
    // decimal back is lossy where a double's shortest decimal is not the double.
    let length = rts_core_rwk::entry::number_of(length_value)
        .map(|value| value as usize)
        .unwrap_or(0);
    (0..length)
        .map(|index| rts_core_rwk::entry::get_indexed(array, rts_core_rwk::entry::make_number(index as f64)))
        .collect()
}

/// An interned string, for use as a property key from outside a borrow.
fn string_key(text: &str) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, text))
}
