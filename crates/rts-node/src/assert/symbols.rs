//! node:assert — the `#[rtse::function]` entry points. Values cross as `Poly`
//! (NaN-boxed `PolyValue`) words; each assertion `throws` (via the pending-error
//! slot) on failure, and returns `undefined` otherwise. Optional `message`
//! arguments use `&str` overloads.

use rts_engine::abi::ty::Poly;

use super::words::{
    deep_equal, invoke_and_caught, is_nullish, loose_eq, strict_eq, throw_assertion, truthy,
};

unsafe extern "C" {
    fn __rtsadp_re_test(re_word: u64, subj_word: u64) -> u64;
}

/// Whether `subject` matches the `RegExp` value `re` (via the engine regex test).
fn re_matches(re: u64, subject: u64) -> bool {
    truthy(unsafe { __rtsadp_re_test(re, subject) })
}

fn fail_msg(default: &str, msg: &str) -> String {
    if msg.is_empty() {
        default.to_string()
    } else {
        msg.to_string()
    }
}

/// `assert(value)` / `assert.ok(value)`.
#[rtse::function(module = "node:assert", value = "ok", throws)]
fn ok(value: Poly) {
    if !truthy(value) {
        throw_assertion("The expression evaluated to a falsy value");
    }
}

/// `assert.ok(value, message)`.
#[rtse::function(module = "node:assert", value = "ok", overload = "msg", throws)]
fn ok_msg(value: Poly, message: &str) {
    if !truthy(value) {
        throw_assertion(&fail_msg("The expression evaluated to a falsy value", message));
    }
}

/// `assert.match(string, regexp)`.
#[rtse::function(module = "node:assert", value = "match", throws)]
fn match_(string: Poly, regexp: Poly) {
    if !re_matches(regexp, string) {
        throw_assertion("The input did not match the regular expression");
    }
}

/// `assert.match(string, regexp, message)`.
#[rtse::function(module = "node:assert", value = "match", overload = "msg", throws)]
fn match_msg(string: Poly, regexp: Poly, message: &str) {
    if !re_matches(regexp, string) {
        throw_assertion(&fail_msg("The input did not match the regular expression", message));
    }
}

/// `assert.doesNotMatch(string, regexp)`.
#[rtse::function(module = "node:assert", value = "doesNotMatch", throws)]
fn does_not_match(string: Poly, regexp: Poly) {
    if re_matches(regexp, string) {
        throw_assertion("The input was expected to not match the regular expression");
    }
}

/// `assert.doesNotMatch(string, regexp, message)`.
#[rtse::function(module = "node:assert", value = "doesNotMatch", overload = "msg", throws)]
fn does_not_match_msg(string: Poly, regexp: Poly, message: &str) {
    if re_matches(regexp, string) {
        throw_assertion(&fail_msg("The input was expected to not match the regular expression", message));
    }
}

/// `assert.equal(actual, expected)` — `==`.
#[rtse::function(module = "node:assert", value = "equal", throws)]
fn equal(a: Poly, b: Poly) {
    if !loose_eq(a, b) {
        throw_assertion("Expected values to be loosely equal");
    }
}

/// `assert.notEqual(actual, expected)`.
#[rtse::function(module = "node:assert", value = "notEqual", throws)]
fn not_equal(a: Poly, b: Poly) {
    if loose_eq(a, b) {
        throw_assertion("Expected values to be loosely unequal");
    }
}

/// `assert.strictEqual(actual, expected)` — `===`.
#[rtse::function(module = "node:assert", value = "strictEqual", throws)]
fn strict_equal(a: Poly, b: Poly) {
    if !strict_eq(a, b) {
        throw_assertion("Expected values to be strictly equal");
    }
}

/// `assert.notStrictEqual(actual, expected)`.
#[rtse::function(module = "node:assert", value = "notStrictEqual", throws)]
fn not_strict_equal(a: Poly, b: Poly) {
    if strict_eq(a, b) {
        throw_assertion("Expected values to be strictly unequal");
    }
}

/// `assert.deepEqual(actual, expected)`.
#[rtse::function(module = "node:assert", value = "deepEqual", throws)]
fn deep_equal_(a: Poly, b: Poly) {
    if !deep_equal(a, b, false) {
        throw_assertion("Expected values to be loosely deep-equal");
    }
}

/// `assert.deepStrictEqual(actual, expected)`.
#[rtse::function(module = "node:assert", value = "deepStrictEqual", throws)]
fn deep_strict_equal(a: Poly, b: Poly) {
    if !deep_equal(a, b, true) {
        throw_assertion("Expected values to be strictly deep-equal");
    }
}

/// `assert.notDeepEqual(actual, expected)`.
#[rtse::function(module = "node:assert", value = "notDeepEqual", throws)]
fn not_deep_equal(a: Poly, b: Poly) {
    if deep_equal(a, b, false) {
        throw_assertion("Expected values not to be loosely deep-equal");
    }
}

/// `assert.notDeepStrictEqual(actual, expected)`.
#[rtse::function(module = "node:assert", value = "notDeepStrictEqual", throws)]
fn not_deep_strict_equal(a: Poly, b: Poly) {
    if deep_equal(a, b, true) {
        throw_assertion("Expected values not to be strictly deep-equal");
    }
}

/// `assert.throws(fn)` — `fn` must throw.
#[rtse::function(module = "node:assert", value = "throws", throws)]
fn throws(fn_word: Poly) {
    if !invoke_and_caught(fn_word) {
        throw_assertion("Missing expected exception");
    }
}

/// `assert.doesNotThrow(fn)` — `fn` must NOT throw.
#[rtse::function(module = "node:assert", value = "doesNotThrow", throws)]
fn does_not_throw(fn_word: Poly) {
    if invoke_and_caught(fn_word) {
        throw_assertion("Got unwanted exception");
    }
}

/// `assert.ifError(value)` — throws when `value` is not `null`/`undefined`.
#[rtse::function(module = "node:assert", value = "ifError", throws)]
fn if_error(value: Poly) {
    if !is_nullish(value) {
        throw_assertion("ifError got unwanted exception");
    }
}

/// `assert.fail()` — always throws.
#[rtse::function(module = "node:assert", value = "fail", throws)]
fn fail() {
    throw_assertion("Failed");
}

/// `assert.fail(message)`.
#[rtse::function(module = "node:assert", value = "fail", overload = "msg", throws)]
fn fail_msg_fn(message: &str) {
    throw_assertion(&fail_msg("Failed", message));
}
